//! Cloud-sync awareness: detecting evicted ("dataless") placeholder files
//! before a restore, and recognising when a path lives inside a cloud-synced
//! folder.
//!
//! Why this module exists: on every desktop platform a cloud sync client can
//! evict a file's contents and leave behind an entry that still `stat()`s with
//! its full logical size. Reading such a file does not fail — it *blocks* for
//! the length of the download with no progress signal, and fails outright when
//! the machine is offline. Measured on macOS 26.6.2, reading **one byte** of an
//! evicted 4 KB iCloud file took **5.6 seconds**
//! (`docs/research/2026-09-16-cloud-durability/raw-backup-options.md` §2.3).
//! Extrapolate that to a multi-gigabyte `.lattice-backup` and the restore looks
//! like a hang. So: probe the metadata *before* opening, tell the user which
//! app has to download the file, and never wrap the read in a short timeout.
//!
//! Detection per platform:
//!
//! | Platform | Signal |
//! |---|---|
//! | macOS | `st_flags & SF_DATALESS (0x40000000)`, or logical size > 0 with `st_blocks == 0` |
//! | Windows | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `_RECALL_ON_OPEN` / `_OFFLINE` |
//! | Linux | size > 0 with `st_blocks == 0` (rclone/insync style FUSE mounts) |
//!
//! Since macOS 12.3 every third-party client (Dropbox, Google Drive, OneDrive,
//! Box) is a File Provider extension under `~/Library/CloudStorage/`, so they
//! all share the macOS dataless mechanism.
//!
//! See `docs/design/2026-09-16-encrypted-backup-archive.md`.

use std::path::{Component, Path};
use std::time::Duration;

use super::format::ArchiveError;

/// How often [`wait_for_hydration`] re-checks the file while a download runs.
pub const HYDRATION_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Used in the `NotHydrated` message when the path heuristics cannot name the
/// sync client. Deliberately vague rather than wrong.
pub const UNKNOWN_PROVIDER_LABEL: &str = "your cloud storage app";

/// `SF_DATALESS` from `<sys/stat.h>`: the contents are evicted to iCloud (or to
/// a File Provider extension) and a read would materialise them.
#[cfg(target_os = "macos")]
const SF_DATALESS: u32 = 0x4000_0000;

/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` — a full placeholder; reading hydrates.
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_OPEN` — a dehydrated file; opening hydrates.
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
/// `FILE_ATTRIBUTE_OFFLINE` — the legacy HSM marker OneDrive still sets.
#[cfg(windows)]
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;

/// Environment variables the OneDrive client exports on Windows. The value is
/// the absolute path of the sync root, which may live outside the profile.
const ONEDRIVE_ENV_VARS: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];

/// Key used for the root component so an absolute path never compares equal to
/// the same components spelled relatively.
const ROOT_KEY: &str = "/";

/// Whether a file's bytes are actually on the local disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAvailability {
    /// Contents are local; reads will not block on a network download.
    Local,
    /// The file exists but its contents are evicted; reads block until a
    /// sync client downloads it. `provider` is a best-effort name.
    Placeholder { provider: Option<CloudProvider> },
    /// No such file.
    Missing,
    /// Could not tell (unsupported platform or metadata error).
    Unknown,
}

/// Cloud sync products we recognise by path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudProvider {
    ICloud,
    Dropbox,
    OneDrive,
    GoogleDrive,
    Box,
    Other(String),
}

impl CloudProvider {
    /// Human-readable product name.
    pub fn display_name(&self) -> String {
        match self {
            CloudProvider::ICloud => "iCloud Drive".to_string(),
            CloudProvider::Dropbox => "Dropbox".to_string(),
            CloudProvider::OneDrive => "OneDrive".to_string(),
            CloudProvider::GoogleDrive => "Google Drive".to_string(),
            CloudProvider::Box => "Box".to_string(),
            CloudProvider::Other(name) => name.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Availability
// ---------------------------------------------------------------------------

/// Inspect platform metadata (macOS `SF_DATALESS` / zero blocks, Windows
/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` and friends) without reading the
/// file. Never blocks on a download.
pub fn file_availability(path: &Path) -> FileAvailability {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return FileAvailability::Missing;
        }
        Err(err) => {
            tracing::debug!(
                path = %path.display(),
                error = %err,
                "cannot stat path; treating cloud availability as unknown"
            );
            return FileAvailability::Unknown;
        }
    };

    // A directory listing is always materialised; only its children can be
    // placeholders.
    if meta.is_dir() {
        return FileAvailability::Local;
    }

    // `symlink_metadata` stats the link itself, and on APFS a symlink reports a
    // non-zero size with `st_blocks == 0` — exactly the shape of an evicted
    // file. Stat the target instead. `fs::metadata` follows the link without
    // ever opening the file, so this still cannot trigger a download.
    let meta = if meta.is_symlink() {
        match std::fs::metadata(path) {
            Ok(target) if target.is_dir() => return FileAvailability::Local,
            Ok(target) => target,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return FileAvailability::Missing;
            }
            Err(err) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "cannot stat symlink target; treating cloud availability as unknown"
                );
                return FileAvailability::Unknown;
            }
        }
    } else {
        meta
    };

    availability_from_metadata(path, &meta)
}

/// Build the `Placeholder` variant, naming the sync client when the path says
/// which one owns the file.
fn placeholder_at(path: &Path) -> FileAvailability {
    let provider = classify_cloud_location(path);
    tracing::debug!(
        path = %path.display(),
        provider = ?provider,
        "file is a cloud placeholder; contents are not on disk"
    );
    FileAvailability::Placeholder { provider }
}

#[cfg(target_os = "macos")]
fn availability_from_metadata(path: &Path, meta: &std::fs::Metadata) -> FileAvailability {
    use std::os::macos::fs::MetadataExt as _;

    // The authoritative signal: `ls -lO` shows `dataless`, `stat -f %Sf` shows
    // the flag. Set by iCloud's "Optimise Mac Storage" and by every File
    // Provider extension (Dropbox, Google Drive, OneDrive, Box) since 12.3.
    if meta.st_flags() & SF_DATALESS != 0 {
        return placeholder_at(path);
    }

    // Belt and braces: an evicted file reports its full logical size with zero
    // allocated blocks. `stat()`'s size lies; the block count does not.
    //
    // Known false positive: a fully sparse file — every block a hole — looks
    // identical through `lstat`. We accept that. A `.lattice-backup` archive is
    // written sequentially and is never sparse, and the cost of guessing wrong
    // is one "download this first" prompt, never a multi-minute stall inside a
    // blocking `read()`.
    if meta.len() > 0 && meta.st_blocks() == 0 {
        return placeholder_at(path);
    }

    FileAvailability::Local
}

#[cfg(windows)]
fn availability_from_metadata(path: &Path, meta: &std::fs::Metadata) -> FileAvailability {
    use std::os::windows::fs::MetadataExt as _;

    // OneDrive Files On-Demand items are reparse points owned by the `CldFlt`
    // filter driver. `attrib +u` (online-only) sets the recall attributes;
    // `FILE_ATTRIBUTE_OFFLINE` is the older HSM marker that is still set.
    const PLACEHOLDER_ATTRIBUTES: u32 = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_OFFLINE;

    if meta.file_attributes() & PLACEHOLDER_ATTRIBUTES != 0 {
        return placeholder_at(path);
    }

    FileAvailability::Local
}

#[cfg(target_os = "linux")]
fn availability_from_metadata(path: &Path, meta: &std::fs::Metadata) -> FileAvailability {
    use std::os::linux::fs::MetadataExt as _;

    // Linux has no standard placeholder attribute. rclone/insync style FUSE
    // mounts expose the remote size with no allocated blocks, which is the only
    // portable hint available. An empty file is trivially local; so is anything
    // with blocks behind it. Same sparse-file caveat as macOS.
    if meta.len() == 0 || meta.st_blocks() > 0 {
        return FileAvailability::Local;
    }

    placeholder_at(path)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn availability_from_metadata(_path: &Path, _meta: &std::fs::Metadata) -> FileAvailability {
    // No known placeholder mechanism on this target: say so rather than guess.
    FileAvailability::Unknown
}

// ---------------------------------------------------------------------------
// Cloud folder classification
// ---------------------------------------------------------------------------

/// Best-effort: is `path` inside a folder a cloud sync client mirrors?
///
/// Path heuristics only (`~/Library/CloudStorage/*`, `~/Library/Mobile
/// Documents`, `~/Dropbox`, `~/OneDrive*`, `%OneDrive%`, ...). The only I/O is
/// resolving the home directory and `canonicalize`, so that a symlinked home or
/// a symlinked destination still matches.
pub fn classify_cloud_location(path: &Path) -> Option<CloudProvider> {
    let home = dirs::home_dir().unwrap_or_default();
    let env = |name: &str| std::env::var(name).ok();
    classify_resolving(path, &home, &env)
}

/// [`classify_cloud_location`] with the home directory and environment injected,
/// still performing the `canonicalize` fallbacks.
pub fn classify_resolving(
    path: &Path,
    home: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<CloudProvider> {
    // Either side may be reached through a symlink (`/tmp` -> `/private/tmp`,
    // a relocated home, a symlinked backup destination), so try the path and the
    // home both as given and as resolved. `canonicalize` fails for paths that do
    // not exist yet, which is fine — the as-given comparison still runs.
    let canonical_path = std::fs::canonicalize(path).ok();
    let canonical_home = std::fs::canonicalize(home).ok();
    let paths: [&Path; 2] = [path, canonical_path.as_deref().unwrap_or(path)];
    let homes: [&Path; 2] = [home, canonical_home.as_deref().unwrap_or(home)];

    for candidate_path in paths {
        for candidate_home in homes {
            if let Some(provider) = classify_with_home(candidate_path, candidate_home, env) {
                return Some(provider);
            }
        }
    }
    None
}

/// The pure core of the classification: no I/O at all.
///
/// `home` is the user's home directory and `env` reads environment variables
/// (the OneDrive client publishes its sync root that way on Windows). Rules are
/// evaluated in order and the first match wins.
pub fn classify_with_home(
    path: &Path,
    home: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<CloudProvider> {
    let keys = path_keys(path);
    let home_keys = path_keys(home);

    if let Some(rel) = strip_root(&keys, &home_keys) {
        if let Some(provider) = classify_home_relative(rel) {
            return Some(provider);
        }
    }

    // Windows: `%USERPROFILE%` is normally the same directory as `home`, but
    // honour it separately when they differ (roaming profiles, a `HOME` set by a
    // dev shell). This covers `%USERPROFILE%\Dropbox`, `\Google Drive` and
    // `\iCloudDrive`.
    if let Some(profile) = env("USERPROFILE") {
        let profile_keys = path_keys(Path::new(profile.trim()));
        if let Some(rel) = strip_root(&keys, &profile_keys) {
            if let Some(provider) = classify_home_relative(rel) {
                return Some(provider);
            }
        }
    }

    // `/Volumes/GoogleDrive*`: the pre-File-Provider Drive File Stream mount,
    // which lives outside the home directory.
    let unrooted: &[String] = match keys.first() {
        Some(first) if first == ROOT_KEY => keys.get(1..).unwrap_or(&[]),
        _ => keys.as_slice(),
    };
    if let (Some(volumes), Some(volume)) = (unrooted.first(), unrooted.get(1)) {
        if key_eq(volumes, "Volumes") && key_starts_with(volume, "GoogleDrive") {
            return Some(CloudProvider::GoogleDrive);
        }
    }

    // Windows OneDrive sync roots, which can be redirected to another drive.
    for var in ONEDRIVE_ENV_VARS {
        let Some(value) = env(var) else { continue };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if strip_root(&keys, &path_keys(Path::new(trimmed))).is_some() {
            return Some(CloudProvider::OneDrive);
        }
    }

    None
}

/// Classify a path already known to be inside the home directory, given its
/// components relative to that home.
fn classify_home_relative(rel: &[String]) -> Option<CloudProvider> {
    let first = rel.first()?;

    if key_eq(first, "Library") {
        let second = rel.get(1)?;
        // macOS third-party File Provider roots:
        // `~/Library/CloudStorage/<Vendor>-<account or team>`.
        if key_eq(second, "CloudStorage") {
            // The container itself is not a synced folder; only its children.
            let vendor_dir = rel.get(2)?;
            return Some(provider_from_cloudstorage_dir(vendor_dir));
        }
        // iCloud Drive and every per-app iCloud container.
        if key_eq(second, "Mobile Documents") {
            return Some(CloudProvider::ICloud);
        }
        return None;
    }

    // `~/iCloud Drive` (Finder alias) and `%USERPROFILE%\iCloudDrive`
    // (iCloud for Windows).
    if key_eq(first, "iCloud Drive") || key_eq(first, "iCloudDrive") {
        return Some(CloudProvider::ICloud);
    }
    // `~/Dropbox`, `~/Dropbox (Personal)`, `~/Dropbox (Acme Inc)`.
    if key_eq(first, "Dropbox") || key_starts_with(first, "Dropbox (") {
        return Some(CloudProvider::Dropbox);
    }
    // `~/OneDrive`, `~/OneDrive - Contoso`.
    if key_eq(first, "OneDrive") || key_starts_with(first, "OneDrive - ") {
        return Some(CloudProvider::OneDrive);
    }
    // Drive for desktop mount points plus the Linux CLI conventions.
    if key_eq(first, "Google Drive")
        || key_eq(first, "My Drive")
        || key_eq(first, "gdrive")
        || key_eq(first, "google-drive")
    {
        return Some(CloudProvider::GoogleDrive);
    }
    if key_eq(first, "Box") || key_eq(first, "Box Sync") {
        return Some(CloudProvider::Box);
    }
    // Insync mirrors Drive, OneDrive *or* Dropbox into `~/Insync/<account>/...`,
    // so the root alone cannot say which service. Name the client instead of
    // guessing a vendor — the string only ever reaches the user as "not
    // downloaded by <name> yet".
    if key_eq(first, "Insync") {
        return Some(CloudProvider::Other("Insync".to_string()));
    }

    None
}

/// Map a `~/Library/CloudStorage/<dir>` entry to a provider.
///
/// The naming convention (`GoogleDrive-<email>`, `Dropbox`, `Dropbox-<Team>`,
/// `OneDrive-Personal`, `OneDrive-<Tenant>`, `Box-Box`) is inferred from
/// community reports, not from any vendor specification, so we prefix-match on
/// the vendor segment and keep unknown vendors verbatim rather than hardcoding
/// full directory names.
fn provider_from_cloudstorage_dir(dir: &str) -> CloudProvider {
    let vendor = dir.split('-').next().unwrap_or(dir);
    if key_eq(vendor, "Dropbox") {
        CloudProvider::Dropbox
    } else if key_eq(vendor, "OneDrive") {
        CloudProvider::OneDrive
    } else if key_eq(vendor, "GoogleDrive") {
        CloudProvider::GoogleDrive
    } else if key_eq(vendor, "Box") {
        CloudProvider::Box
    } else if vendor.is_empty() {
        CloudProvider::Other(dir.to_string())
    } else {
        CloudProvider::Other(vendor.to_string())
    }
}

/// Split a path into comparable component keys. The root and any drive prefix
/// become keys of their own so `/a/b` never matches `a/b` and `C:\x` never
/// matches `D:\x`.
fn path_keys(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir => Some(ROOT_KEY.to_string()),
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_string()),
            Component::CurDir => None,
        })
        .collect()
}

/// Components of `keys` below `root`, or `None` when `keys` is not inside it.
/// An empty `root` never matches, so a missing home directory cannot make every
/// path look like a cloud folder.
fn strip_root<'a>(keys: &'a [String], root: &[String]) -> Option<&'a [String]> {
    if root.is_empty() || root.len() > keys.len() {
        return None;
    }
    let head = keys.get(..root.len())?;
    if head.iter().zip(root.iter()).all(|(a, b)| key_eq(a, b)) {
        keys.get(root.len()..)
    } else {
        None
    }
}

/// Compare one path component. Windows paths are case-insensitive; POSIX
/// components are compared verbatim (APFS is usually case-insensitive too, but
/// the on-disk spelling of every folder listed here is fixed by its vendor).
#[cfg(windows)]
fn key_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(not(windows))]
fn key_eq(a: &str, b: &str) -> bool {
    a == b
}

/// Prefix test for one path component, with the same case rules as [`key_eq`].
#[cfg(windows)]
fn key_starts_with(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[cfg(not(windows))]
fn key_starts_with(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
}

// ---------------------------------------------------------------------------
// Hydration
// ---------------------------------------------------------------------------

/// Wait until `path` is hydrated, nudging the sync client by opening the
/// file for reading on a blocking thread. Reports elapsed time through
/// `progress`. Returns `NotHydrated` on timeout.
pub async fn wait_for_hydration(
    path: &Path,
    timeout: Duration,
    progress: &mut dyn FnMut(Duration),
) -> Result<(), ArchiveError> {
    wait_for_hydration_with(
        path,
        timeout,
        HYDRATION_POLL_INTERVAL,
        progress,
        &file_availability,
    )
    .await
}

/// [`wait_for_hydration`] with the availability probe and poll interval
/// injected, so the loop can be tested without a real placeholder file (which
/// cannot be created: `chflags dataless` is not a thing a user process can do).
pub async fn wait_for_hydration_with(
    path: &Path,
    timeout: Duration,
    poll_interval: Duration,
    progress: &mut dyn FnMut(Duration),
    availability: &dyn Fn(&Path) -> FileAvailability,
) -> Result<(), ArchiveError> {
    let started = std::time::Instant::now();
    let mut nudged = false;

    loop {
        match availability(path) {
            FileAvailability::Local => {
                if nudged {
                    tracing::info!(
                        path = %path.display(),
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "cloud placeholder finished downloading"
                    );
                }
                return Ok(());
            }
            FileAvailability::Missing => {
                return Err(ArchiveError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{} no longer exists", path.display()),
                )));
            }
            FileAvailability::Unknown => {
                // We cannot tell. Do not stall the user on a guess; let the
                // caller open the file and surface a real error if it fails.
                tracing::debug!(
                    path = %path.display(),
                    "cloud availability unknown; proceeding without waiting"
                );
                return Ok(());
            }
            FileAvailability::Placeholder { provider } => {
                if !nudged {
                    nudged = true;
                    tracing::info!(
                        path = %path.display(),
                        provider = ?provider,
                        timeout_ms = timeout.as_millis() as u64,
                        "file is a cloud placeholder; requesting download"
                    );
                    spawn_hydration_nudge(path);
                    // Report immediately so the caller can show "downloading…"
                    // before the first poll interval elapses.
                    progress(started.elapsed());
                }

                let elapsed = started.elapsed();
                if elapsed >= timeout {
                    let name = provider
                        .map(|p| p.display_name())
                        .unwrap_or_else(|| UNKNOWN_PROVIDER_LABEL.to_string());
                    tracing::warn!(
                        path = %path.display(),
                        provider = %name,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "timed out waiting for a cloud placeholder to download"
                    );
                    return Err(ArchiveError::NotHydrated(name));
                }

                let remaining = timeout.saturating_sub(elapsed);
                tokio::time::sleep(poll_interval.min(remaining)).await;
                progress(started.elapsed());
            }
        }
    }
}

/// Ask the sync client for the file's contents by reading its first byte on a
/// blocking thread.
///
/// The handle is dropped on purpose: a blocking-pool task cannot be cancelled,
/// and this one may sit inside `read()` for the entire download (5.6 s for one
/// byte of a 4 KB evicted file, measured). Detaching it lets the timeout path
/// return while the download keeps running in the background — which is what we
/// want, because the next attempt then finds the file hydrated.
fn spawn_hydration_nudge(path: &Path) {
    let owned = path.to_path_buf();
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            drop(handle.spawn_blocking(move || nudge_hydration(&owned)));
        }
        Err(_) => {
            tracing::debug!(
                path = %path.display(),
                "no tokio runtime available; skipping the hydration nudge"
            );
        }
    }
}

/// Open the file and read one byte. This is the only portable way to ask for a
/// download: on macOS it materialises the dataless object, on Windows it
/// triggers the `CldFlt` recall, and Dropbox publishes no API at all.
///
/// Note: `brctl download` is *not* used. It no longer exists — on macOS 26.6.2
/// `brctl` only offers `diagnose, log, dump, status, accounts, quota, monitor`,
/// and `fileproviderctl` has no `materialize`
/// (`docs/research/2026-09-16-cloud-durability/raw-backup-options.md` §2.3).
/// The blessed alternative is `FileManager.startDownloadingUbiquitousItem`,
/// which needs an Objective-C bridge we do not have here.
fn nudge_hydration(path: &Path) {
    use std::io::Read as _;

    match std::fs::File::open(path) {
        Ok(mut file) => {
            let mut byte = [0u8; 1];
            match file.read(&mut byte) {
                Ok(read) => tracing::debug!(
                    path = %path.display(),
                    read,
                    "hydration nudge completed"
                ),
                Err(err) => tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "hydration nudge could not read the file"
                ),
            }
        }
        Err(err) => tracing::debug!(
            path = %path.display(),
            error = %err,
            "hydration nudge could not open the file"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Build a path from components so the tests are spelling-agnostic across
    /// hosts (`/a/b` on POSIX, `\a\b` on Windows).
    fn p(parts: &[&str]) -> PathBuf {
        let mut path = PathBuf::new();
        for part in parts {
            path.push(part);
        }
        path
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn home() -> PathBuf {
        p(&["/", "home", "u"])
    }

    // -- classification -----------------------------------------------------

    #[test]
    fn classify_with_home_recognises_every_cloud_root() {
        let home = home();
        let cases: Vec<(PathBuf, CloudProvider)> = vec![
            // macOS File Provider roots under ~/Library/CloudStorage.
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "CloudStorage",
                    "Dropbox",
                    "b.lattice-backup",
                ]),
                CloudProvider::Dropbox,
            ),
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "CloudStorage",
                    "Dropbox-Acme Inc",
                    "b",
                ]),
                CloudProvider::Dropbox,
            ),
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "CloudStorage",
                    "OneDrive-Personal",
                    "b",
                ]),
                CloudProvider::OneDrive,
            ),
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "CloudStorage",
                    "GoogleDrive-a@b.com",
                    "b",
                ]),
                CloudProvider::GoogleDrive,
            ),
            (
                p(&["/", "home", "u", "Library", "CloudStorage", "Box-Box", "b"]),
                CloudProvider::Box,
            ),
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "CloudStorage",
                    "pCloud-josh",
                    "b",
                ]),
                CloudProvider::Other("pCloud".to_string()),
            ),
            // iCloud.
            (
                p(&[
                    "/",
                    "home",
                    "u",
                    "Library",
                    "Mobile Documents",
                    "com~apple~CloudDocs",
                    "b",
                ]),
                CloudProvider::ICloud,
            ),
            (
                p(&["/", "home", "u", "iCloud Drive", "b"]),
                CloudProvider::ICloud,
            ),
            // Windows: %USERPROFILE%\iCloudDrive.
            (
                p(&["/", "home", "u", "iCloudDrive", "b"]),
                CloudProvider::ICloud,
            ),
            // Dropbox, including the team-suffixed folder names.
            (
                p(&["/", "home", "u", "Dropbox", "b"]),
                CloudProvider::Dropbox,
            ),
            (
                p(&["/", "home", "u", "Dropbox (Personal)", "b"]),
                CloudProvider::Dropbox,
            ),
            (
                p(&["/", "home", "u", "Dropbox (Acme Inc)", "b"]),
                CloudProvider::Dropbox,
            ),
            // OneDrive, personal and tenant-joined.
            (
                p(&["/", "home", "u", "OneDrive", "b"]),
                CloudProvider::OneDrive,
            ),
            (
                p(&["/", "home", "u", "OneDrive - Contoso", "b"]),
                CloudProvider::OneDrive,
            ),
            // Google Drive, desktop client and Linux conventions.
            (
                p(&["/", "home", "u", "Google Drive", "b"]),
                CloudProvider::GoogleDrive,
            ),
            (
                p(&["/", "home", "u", "My Drive", "b"]),
                CloudProvider::GoogleDrive,
            ),
            (
                p(&["/", "home", "u", "gdrive", "b"]),
                CloudProvider::GoogleDrive,
            ),
            (
                p(&["/", "home", "u", "google-drive", "b"]),
                CloudProvider::GoogleDrive,
            ),
            // Box.
            (p(&["/", "home", "u", "Box", "b"]), CloudProvider::Box),
            (p(&["/", "home", "u", "Box Sync", "b"]), CloudProvider::Box),
            // Insync (Linux) mirrors several services; name the client.
            (
                p(&["/", "home", "u", "Insync", "a@b.com", "Google Drive", "b"]),
                CloudProvider::Other("Insync".to_string()),
            ),
            // Legacy Drive File Stream mount, outside the home directory.
            (
                p(&["/", "Volumes", "GoogleDrive", "My Drive", "b"]),
                CloudProvider::GoogleDrive,
            ),
            (
                p(&["/", "Volumes", "GoogleDrive-a@b.com", "b"]),
                CloudProvider::GoogleDrive,
            ),
        ];

        for (path, expected) in cases {
            assert_eq!(
                classify_with_home(&path, &home, &no_env),
                Some(expected),
                "path {}",
                path.display()
            );
        }
    }

    #[test]
    fn classify_with_home_ignores_ordinary_folders() {
        let home = home();
        let cases = vec![
            p(&["/", "home", "u", "Documents", "b.lattice-backup"]),
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "Application Support",
                "Lattice",
                "b",
            ]),
            // The CloudStorage container itself is not a synced folder.
            p(&["/", "home", "u", "Library", "CloudStorage"]),
            p(&["/", "home", "u", "DropboxNotReally", "b"]),
            p(&["/", "home", "u", "OneDriveArchive", "b"]),
            p(&["/", "tmp", "b.lattice-backup"]),
            p(&["/", "Volumes", "Backup Disk", "b"]),
            // Another user's Dropbox is not below this home.
            p(&["/", "home", "other", "Dropbox", "b"]),
        ];
        for path in cases {
            assert_eq!(
                classify_with_home(&path, &home, &no_env),
                None,
                "path {}",
                path.display()
            );
        }
    }

    #[test]
    fn classify_with_home_requires_a_home_to_match_home_relative_rules() {
        // An unresolvable home must not turn every path into a cloud folder.
        assert_eq!(
            classify_with_home(&p(&["/", "Dropbox", "b"]), Path::new(""), &no_env),
            None
        );
    }

    #[test]
    fn classify_with_home_honours_windows_onedrive_env_vars() {
        let home = home();
        for var in ONEDRIVE_ENV_VARS {
            let root = p(&["/", "mnt", "work", "OneDriveRoot"]);
            let value = root.to_string_lossy().into_owned();
            let env = move |name: &str| {
                if name == var {
                    Some(value.clone())
                } else {
                    None
                }
            };
            let file = root.join("Lattice Backups").join("b.lattice-backup");
            assert_eq!(
                classify_with_home(&file, &home, &env),
                Some(CloudProvider::OneDrive),
                "env var {var}"
            );
            // A sibling directory with the same prefix is not inside the root.
            let sibling = p(&["/", "mnt", "work", "OneDriveRootOther", "b"]);
            assert_eq!(
                classify_with_home(&sibling, &home, &env),
                None,
                "env var {var}"
            );
        }
    }

    #[test]
    fn classify_with_home_honours_userprofile_when_it_differs_from_home() {
        let profile = p(&["/", "Users", "roaming"]);
        let value = profile.to_string_lossy().into_owned();
        let env = |name: &str| {
            if name == "USERPROFILE" {
                Some(value.clone())
            } else {
                None
            }
        };
        // The Windows rules: %USERPROFILE%\Dropbox, \Google Drive, \iCloudDrive.
        assert_eq!(
            classify_with_home(&profile.join("Dropbox").join("b"), &home(), &env),
            Some(CloudProvider::Dropbox)
        );
        assert_eq!(
            classify_with_home(&profile.join("Google Drive").join("b"), &home(), &env),
            Some(CloudProvider::GoogleDrive)
        );
        assert_eq!(
            classify_with_home(&profile.join("iCloudDrive").join("b"), &home(), &env),
            Some(CloudProvider::ICloud)
        );
        assert_eq!(
            classify_with_home(&profile.join("Documents"), &home(), &env),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn classify_with_home_is_case_insensitive_on_windows() {
        let home = home();
        assert_eq!(
            classify_with_home(&p(&["/", "home", "u", "dropbox", "b"]), &home, &no_env),
            Some(CloudProvider::Dropbox)
        );
        assert_eq!(
            classify_with_home(&p(&["/", "HOME", "U", "OneDrive", "b"]), &home, &no_env),
            Some(CloudProvider::OneDrive)
        );
    }

    #[cfg(windows)]
    #[test]
    fn classify_with_home_keeps_drive_letters_apart() {
        let home = p(&["C:", "/", "Users", "u"]);
        assert_eq!(
            classify_with_home(
                &p(&["C:", "/", "Users", "u", "Dropbox", "b"]),
                &home,
                &no_env
            ),
            Some(CloudProvider::Dropbox)
        );
        assert_eq!(
            classify_with_home(
                &p(&["D:", "/", "Users", "u", "Dropbox", "b"]),
                &home,
                &no_env
            ),
            None
        );
    }

    #[test]
    fn provider_display_names_are_product_names() {
        assert_eq!(CloudProvider::ICloud.display_name(), "iCloud Drive");
        assert_eq!(CloudProvider::GoogleDrive.display_name(), "Google Drive");
        assert_eq!(
            CloudProvider::Other("pCloud".to_string()).display_name(),
            "pCloud"
        );
    }

    #[cfg(unix)]
    #[test]
    fn classify_resolving_matches_through_a_symlinked_home() {
        let tmp = tempfile::tempdir().unwrap();
        let real_home = tmp.path().join("real-home");
        std::fs::create_dir_all(real_home.join("Dropbox")).unwrap();
        let link_home = tmp.path().join("link-home");
        std::os::unix::fs::symlink(&real_home, &link_home).unwrap();

        let file = link_home.join("Dropbox").join("b.lattice-backup");
        std::fs::write(&file, b"x").unwrap();

        // The configured home is the real directory; the path arrived via the
        // symlink. Only the canonicalising pass can connect the two.
        assert_eq!(classify_with_home(&file, &real_home, &no_env), None);
        assert_eq!(
            classify_resolving(&file, &real_home, &no_env),
            Some(CloudProvider::Dropbox)
        );
    }

    // -- availability -------------------------------------------------------

    #[test]
    fn file_availability_reports_local_for_ordinary_files_and_directories() {
        let tmp = tempfile::tempdir().unwrap();

        let file = tmp.path().join("archive.lattice-backup");
        std::fs::write(&file, b"contents are on disk").unwrap();
        assert_eq!(file_availability(&file), FileAvailability::Local);

        // Zero-length is trivially local: there are no bytes to download.
        let empty = tmp.path().join("empty.lattice-backup");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(file_availability(&empty), FileAvailability::Local);

        assert_eq!(file_availability(tmp.path()), FileAvailability::Local);
    }

    #[test]
    fn file_availability_reports_missing_for_absent_paths() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            file_availability(&tmp.path().join("nope.lattice-backup")),
            FileAvailability::Missing
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_availability_stats_the_symlink_target() {
        // Regression guard: APFS reports a symlink's own inode with a non-zero
        // size and `st_blocks == 0`, which is exactly the shape of an evicted
        // file. Stating the link instead of its target would call every symlink
        // a placeholder.
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("archive.lattice-backup");
        std::fs::write(&target, b"contents are on disk").unwrap();

        let link = tmp.path().join("link.lattice-backup");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(file_availability(&link), FileAvailability::Local);

        let dangling = tmp.path().join("dangling.lattice-backup");
        std::os::unix::fs::symlink(tmp.path().join("gone"), &dangling).unwrap();
        assert_eq!(file_availability(&dangling), FileAvailability::Missing);
    }

    // -- hydration ----------------------------------------------------------

    #[tokio::test]
    async fn wait_for_hydration_returns_immediately_for_a_local_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("archive.lattice-backup");
        std::fs::write(&file, b"local").unwrap();

        let mut ticks: Vec<Duration> = Vec::new();
        {
            let mut progress = |elapsed: Duration| ticks.push(elapsed);
            wait_for_hydration(&file, Duration::from_secs(5), &mut progress)
                .await
                .unwrap();
        }
        assert!(ticks.is_empty(), "a local file must not report progress");
    }

    #[tokio::test]
    async fn wait_for_hydration_with_succeeds_once_the_file_becomes_local() {
        let remaining = AtomicUsize::new(3);
        let availability = |_: &Path| {
            if remaining.load(Ordering::Relaxed) == 0 {
                FileAvailability::Local
            } else {
                remaining.fetch_sub(1, Ordering::Relaxed);
                FileAvailability::Placeholder {
                    provider: Some(CloudProvider::ICloud),
                }
            }
        };

        let mut ticks: Vec<Duration> = Vec::new();
        {
            let mut progress = |elapsed: Duration| ticks.push(elapsed);
            wait_for_hydration_with(
                Path::new("/nonexistent/lattice/archive.lattice-backup"),
                Duration::from_secs(30),
                Duration::from_millis(1),
                &mut progress,
                &availability,
            )
            .await
            .unwrap();
        }

        // One immediate report plus one per poll.
        assert_eq!(ticks.len(), 4);
        assert!(
            ticks.windows(2).all(|w| w[0] <= w[1]),
            "elapsed must not go backwards"
        );
    }

    #[tokio::test]
    async fn wait_for_hydration_with_times_out_naming_the_provider() {
        let availability = |_: &Path| FileAvailability::Placeholder {
            provider: Some(CloudProvider::OneDrive),
        };
        let mut progress = |_: Duration| {};
        let err = wait_for_hydration_with(
            Path::new("/nonexistent/lattice/archive.lattice-backup"),
            Duration::from_millis(30),
            Duration::from_millis(5),
            &mut progress,
            &availability,
        )
        .await
        .unwrap_err();

        match err {
            ArchiveError::NotHydrated(name) => assert_eq!(name, "OneDrive"),
            other => panic!("expected NotHydrated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_for_hydration_with_falls_back_to_a_generic_provider_label() {
        let availability = |_: &Path| FileAvailability::Placeholder { provider: None };
        let mut progress = |_: Duration| {};
        let err = wait_for_hydration_with(
            Path::new("/nonexistent/lattice/archive.lattice-backup"),
            Duration::from_millis(10),
            Duration::from_millis(5),
            &mut progress,
            &availability,
        )
        .await
        .unwrap_err();

        match err {
            ArchiveError::NotHydrated(name) => assert_eq!(name, UNKNOWN_PROVIDER_LABEL),
            other => panic!("expected NotHydrated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_for_hydration_with_reports_missing_and_tolerates_unknown() {
        let mut progress = |_: Duration| {};

        let missing = |_: &Path| FileAvailability::Missing;
        let err = wait_for_hydration_with(
            Path::new("/nonexistent/lattice/archive.lattice-backup"),
            Duration::from_millis(10),
            Duration::from_millis(5),
            &mut progress,
            &missing,
        )
        .await
        .unwrap_err();
        match err {
            ArchiveError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
            other => panic!("expected Io(NotFound), got {other:?}"),
        }

        // Unknown must not block the restore behind a guess.
        let unknown = |_: &Path| FileAvailability::Unknown;
        wait_for_hydration_with(
            Path::new("/nonexistent/lattice/archive.lattice-backup"),
            Duration::from_millis(10),
            Duration::from_millis(5),
            &mut progress,
            &unknown,
        )
        .await
        .unwrap();
    }
}
