//! Reading a payload back: the manifest first, then every entry checked before it is written.
use super::{ProgressEvent, MAX_MANIFEST_BYTES};
use crate::features::backup::archive::format::{
    ArchiveError, Manifest, CHAT_IMAGES_ENTRY_PREFIX, DB_ENTRY, FILES_ENTRY_PREFIX, MANIFEST_ENTRY,
    SETTINGS_ENTRY, VAULT_ENTRY_PREFIX, WEB_ARCHIVE_ENTRY_PREFIX, WEB_SNAPSHOTS_ENTRY_PREFIX,
};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// Read only the first tar entry (`manifest.json`) from a payload stream.
///
/// The reader is consumed only as far as the zstd frame buffering requires,
/// which is why callers reopen the file before extracting.
pub fn read_manifest<R: Read>(input: R) -> Result<Manifest, ArchiveError> {
    let decoder = zstd::Decoder::new(input)?;
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive.entries()?;
    let mut first = entries
        .next()
        .ok_or_else(|| ArchiveError::Corrupt("backup payload is empty".to_string()))??;

    let name = checked_entry_name(first.path()?.as_ref())?;
    if name != MANIFEST_ENTRY {
        return Err(ArchiveError::Corrupt(format!(
            "expected {MANIFEST_ENTRY} as the first entry, found {name}"
        )));
    }
    let manifest: Manifest = serde_json::from_reader(first.by_ref().take(MAX_MANIFEST_BYTES))?;
    Ok(manifest)
}

/// Where `extract_payload` put things, all under the scratch directory.
#[derive(Debug, Clone)]
pub struct ExtractedPayload {
    pub manifest: Manifest,
    pub db_path: PathBuf,
    pub files_dir: Option<PathBuf>,
    pub vault_dir: Option<PathBuf>,
    pub web_archive_dir: Option<PathBuf>,
    pub web_snapshots_dir: Option<PathBuf>,
    pub chat_images_dir: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
}

/// Validate a tar entry path and render it as a forward-slashed string.
pub(super) fn checked_entry_name(path: &Path) -> Result<String, ArchiveError> {
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {
                return Err(ArchiveError::Corrupt(
                    "backup entry path contains a `.` component".to_string(),
                ))
            }
            Component::ParentDir => {
                return Err(ArchiveError::Corrupt(
                    "backup entry path contains `..`".to_string(),
                ))
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::Corrupt(
                    "backup entry path is absolute".to_string(),
                ))
            }
        }
    }
    let name = path
        .to_str()
        .ok_or_else(|| ArchiveError::Corrupt("backup entry name is not UTF-8".to_string()))?;
    if name.is_empty() {
        return Err(ArchiveError::Corrupt(
            "backup entry has an empty name".to_string(),
        ));
    }
    Ok(name.replace('\\', "/"))
}

/// Which part of the payload an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryGroup {
    Manifest,
    Db,
    Files,
    Vault,
    WebArchive,
    WebSnapshots,
    ChatImages,
    Settings,
}

/// Reject anything outside the known prefixes. Directory entries arrive
/// without their trailing slash once `Path` has parsed them, hence the bare
/// `"db"`/`"files"`/`"vault"`/`"web-archive"` cases.
pub(super) fn classify_entry(name: &str) -> Result<EntryGroup, ArchiveError> {
    let db_prefix = DB_ENTRY.split('/').next().unwrap_or("db");
    let files_dir = FILES_ENTRY_PREFIX.trim_end_matches('/');
    let vault_dir = VAULT_ENTRY_PREFIX.trim_end_matches('/');
    let web_archive_dir = WEB_ARCHIVE_ENTRY_PREFIX.trim_end_matches('/');
    let web_snapshots_dir = WEB_SNAPSHOTS_ENTRY_PREFIX.trim_end_matches('/');
    let chat_images_dir = CHAT_IMAGES_ENTRY_PREFIX.trim_end_matches('/');

    if name == MANIFEST_ENTRY {
        Ok(EntryGroup::Manifest)
    } else if name == SETTINGS_ENTRY {
        Ok(EntryGroup::Settings)
    } else if name == DB_ENTRY || name == db_prefix {
        Ok(EntryGroup::Db)
    } else if name == files_dir || name.starts_with(FILES_ENTRY_PREFIX) {
        Ok(EntryGroup::Files)
    } else if name == vault_dir || name.starts_with(VAULT_ENTRY_PREFIX) {
        Ok(EntryGroup::Vault)
    } else if name == web_archive_dir || name.starts_with(WEB_ARCHIVE_ENTRY_PREFIX) {
        Ok(EntryGroup::WebArchive)
    } else if name == web_snapshots_dir || name.starts_with(WEB_SNAPSHOTS_ENTRY_PREFIX) {
        Ok(EntryGroup::WebSnapshots)
    } else if name == chat_images_dir || name.starts_with(CHAT_IMAGES_ENTRY_PREFIX) {
        Ok(EntryGroup::ChatImages)
    } else {
        Err(ArchiveError::Corrupt(format!(
            "backup contains an unexpected entry: {name}"
        )))
    }
}

/// Resolve a forward-slashed archive path under `dest_dir`.
fn under(dest_dir: &Path, name: &str) -> PathBuf {
    let mut path = dest_dir.to_path_buf();
    for part in name.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path
}

/// Unpack a payload stream into `dest_dir` (must be empty or absent). Rejects
/// absolute paths, `..`, symlinks/hardlinks, and entries outside the known
/// prefixes. Synchronous; callers run it inside `spawn_blocking`.
///
/// `dest_dir` is created if it does not exist.
pub fn extract_payload<R: Read>(
    input: R,
    dest_dir: &Path,
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<ExtractedPayload, ArchiveError> {
    match std::fs::read_dir(dest_dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(ArchiveError::Other(format!(
                    "restore scratch directory {} is not empty",
                    dest_dir.display()
                )));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ArchiveError::Io(error)),
    }
    std::fs::create_dir_all(dest_dir)?;

    let decoder = zstd::Decoder::new(input)?;
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);

    let mut entries = archive.entries()?;
    progress(ProgressEvent::Scanning);

    let mut first = entries
        .next()
        .ok_or_else(|| ArchiveError::Corrupt("backup payload is empty".to_string()))??;
    let first_name = checked_entry_name(first.path()?.as_ref())?;
    if first_name != MANIFEST_ENTRY {
        return Err(ArchiveError::Corrupt(format!(
            "expected {MANIFEST_ENTRY} as the first entry, found {first_name}"
        )));
    }
    let manifest: Manifest = serde_json::from_reader(first.by_ref().take(MAX_MANIFEST_BYTES))?;

    let bytes_total = manifest.total_plaintext_bytes;
    let mut bytes_done = 0u64;
    let mut saw_db = false;
    let mut saw_files = false;
    let mut saw_vault = false;
    let mut saw_web_archive = false;
    let mut saw_web_snapshots = false;
    let mut saw_chat_images = false;
    let mut saw_settings = false;

    for result in entries {
        let mut entry = result?;
        let entry_type = entry.header().entry_type();
        if !matches!(
            entry_type,
            tar::EntryType::Regular | tar::EntryType::Directory
        ) {
            return Err(ArchiveError::Corrupt(format!(
                "backup contains an unsupported entry type: {entry_type:?}"
            )));
        }

        let name = checked_entry_name(entry.path()?.as_ref())?;
        let group = classify_entry(&name)?;
        if group == EntryGroup::Manifest {
            return Err(ArchiveError::Corrupt(
                "backup contains more than one manifest entry".to_string(),
            ));
        }

        let size = entry.size();
        if !entry.unpack_in(dest_dir)? {
            return Err(ArchiveError::Corrupt(format!(
                "refused to unpack unsafe backup entry: {name}"
            )));
        }

        match group {
            EntryGroup::Db => saw_db = true,
            EntryGroup::Files => saw_files = true,
            EntryGroup::Vault => saw_vault = true,
            EntryGroup::WebArchive => saw_web_archive = true,
            EntryGroup::WebSnapshots => saw_web_snapshots = true,
            EntryGroup::ChatImages => saw_chat_images = true,
            EntryGroup::Settings => saw_settings = true,
            EntryGroup::Manifest => {}
        }

        if entry_type == tar::EntryType::Regular {
            bytes_done = bytes_done.saturating_add(size);
            progress(ProgressEvent::Entry {
                path: name,
                bytes_done,
                bytes_total,
            });
        }
    }

    progress(ProgressEvent::Done);

    if !saw_db {
        return Err(ArchiveError::Corrupt(
            "backup contains no database snapshot".to_string(),
        ));
    }

    Ok(ExtractedPayload {
        manifest,
        db_path: under(dest_dir, DB_ENTRY),
        files_dir: saw_files.then(|| under(dest_dir, FILES_ENTRY_PREFIX)),
        vault_dir: saw_vault.then(|| under(dest_dir, VAULT_ENTRY_PREFIX)),
        web_archive_dir: saw_web_archive.then(|| under(dest_dir, WEB_ARCHIVE_ENTRY_PREFIX)),
        web_snapshots_dir: saw_web_snapshots.then(|| under(dest_dir, WEB_SNAPSHOTS_ENTRY_PREFIX)),
        chat_images_dir: saw_chat_images.then(|| under(dest_dir, CHAT_IMAGES_ENTRY_PREFIX)),
        settings_file: saw_settings.then(|| under(dest_dir, SETTINGS_ENTRY)),
    })
}
