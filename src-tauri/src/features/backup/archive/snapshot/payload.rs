//! Writing the payload: scanning the data directory and library into a zstd tar.
use super::{is_blob_hash, EXCLUDED_TABLES, ZSTD_LEVEL};
use crate::features::backup::archive::format::{
    ArchiveError, Manifest, CHAT_IMAGES_ENTRY_PREFIX, DB_ENTRY, FILES_ENTRY_PREFIX, FORMAT_VERSION,
    MANIFEST_ENTRY, SETTINGS_ENTRY, VAULT_ENTRY_PREFIX, WEB_ARCHIVE_ENTRY_PREFIX,
    WEB_SNAPSHOTS_ENTRY_PREFIX,
};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

/// Everything the payload writer packs.
#[derive(Debug, Clone, Default)]
pub struct ArchiveInputs {
    /// Output of `snapshot_database`.
    pub db_snapshot: PathBuf,
    pub migration_version: Option<i64>,
    /// Content-addressed files library root (`~/.lattice/files`), if it exists.
    pub files_root: Option<PathBuf>,
    /// Blob hashes the archived database references, from
    /// [`referenced_blob_hashes`]. Only these blob directories are packed.
    /// `None` skips the reference check and packs every blob directory; it is
    /// what payload tests with no database use, and the fallback when the
    /// snapshot cannot be asked.
    pub referenced_blob_hashes: Option<HashSet<String>>,
    /// Vault markdown folder, if the vault is enabled and the folder exists.
    pub vault_root: Option<PathBuf>,
    /// Saved web articles (`~/.lattice/web-archive`), if the folder exists.
    /// Web documents' `file_path` points into it, and the reader opens the
    /// page snapshot from there.
    pub web_archive_root: Option<PathBuf>,
    /// `settings.json`, if present.
    pub settings_file: Option<PathBuf>,
    /// Subtrees to skip while walking (for example the archive destination
    /// itself, in case the user chose a folder inside the vault).
    pub exclude: Vec<PathBuf>,
    pub app_version: String,
    pub created_at: String,
}

/// Progress callback events. Byte counts are plaintext bytes.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Scanning,
    Entry {
        path: String,
        bytes_done: u64,
        bytes_total: u64,
    },
    Done,
}

/// One tar entry decided during the scan pass.
#[derive(Debug, Clone)]
struct PlannedEntry {
    source: PathBuf,
    /// Tar entry name, always forward-slashed; directories end in `/`.
    name: String,
    is_dir: bool,
}

/// Canonicalize where possible, otherwise keep the path as given. Both the
/// walk roots and the excludes go through this, so the `starts_with` test
/// below compares like with like (on macOS a `TempDir` under `/var` only
/// matches an exclude under `/private/var` once both have been resolved).
fn canonical_or_owned(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_excluded(path: &Path, excludes: &[PathBuf]) -> bool {
    excludes.iter().any(|ex| path.starts_with(ex))
}

/// Join `prefix` and the components of `rel` with forward slashes. Returns
/// `None` when a component is not valid UTF-8 or is not a plain name, since
/// such an entry cannot be given a portable tar name.
fn entry_name(prefix: &str, rel: &Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("{prefix}{}", parts.join("/")))
}

/// Walk `root`, appending planned entries. Returns `(file_count, byte_total)`.
fn scan_tree(
    root: &Path,
    prefix: &str,
    excludes: &[PathBuf],
    out: &mut Vec<PlannedEntry>,
) -> (u64, u64) {
    let root = canonical_or_owned(root);
    let mut files = 0u64;
    let mut bytes = 0u64;

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .sort_by_file_name()
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| !is_excluded(entry.path(), excludes));

    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                warn!(%error, root = %root.display(), "skipping unreadable path while scanning");
                continue;
            }
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            debug!(path = %entry.path().display(), "skipping symlink (not followed)");
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let Some(name) = entry_name(prefix, rel) else {
            warn!(path = %entry.path().display(), "skipping path with a non-UTF-8 name");
            continue;
        };

        if file_type.is_dir() {
            out.push(PlannedEntry {
                source: entry.path().to_path_buf(),
                name: format!("{name}/"),
                is_dir: true,
            });
        } else if file_type.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(size);
            out.push(PlannedEntry {
                source: entry.path().to_path_buf(),
                name,
                is_dir: false,
            });
        } else {
            debug!(path = %entry.path().display(), "skipping non-regular file");
        }
    }

    (files, bytes)
}

/// What `scan_library` decided about a files-library root.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LibraryScan {
    /// Blob directories packed.
    blobs: u64,
    /// Files packed (what the manifest's `files_count` reports).
    files: u64,
    /// Plaintext bytes packed.
    bytes: u64,
    /// Top-level entries left out: orphan blobs and anything that is not a
    /// blob directory at all.
    skipped_blobs: u64,
    /// Bytes those skipped entries occupy on disk.
    skipped_bytes: u64,
}

/// Plan the files-library entries.
///
/// The library is content-addressed — `{root}/{sha256}/{filename}` — and the
/// archived database is the only authority on which of those hashes still
/// matter. So a top-level entry is packed only when it is a directory, its
/// name is a blob hash, and `referenced` contains that hash. A blob left
/// behind by an import that crashed, or by a document deleted before the
/// orphan sweep ran, is not the user's data any more and does not belong in
/// their backup; neither does a `.DS_Store` somebody's file manager dropped
/// in the library root.
///
/// `referenced` of `None` skips the reference check (but not the shape
/// check): payload tests have no database to ask, and a snapshot that cannot
/// answer must not cost the user their files.
fn scan_library(
    root: &Path,
    referenced: Option<&HashSet<String>>,
    excludes: &[PathBuf],
    out: &mut Vec<PlannedEntry>,
) -> LibraryScan {
    let root = canonical_or_owned(root);
    let mut scan = LibraryScan::default();

    let read_dir = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            warn!(%error, root = %root.display(), "files library is unreadable; nothing archived from it");
            return scan;
        }
    };

    // `read_dir` order is whatever the filesystem feels like; sort so two
    // archives of the same library have the same entry order.
    let mut children: Vec<(String, PathBuf, bool, bool)> = Vec::new();
    for entry in read_dir {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warn!(%error, root = %root.display(), "skipping unreadable library entry");
                continue;
            }
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            warn!(path = %entry.path().display(), "skipping library entry with a non-UTF-8 name");
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            warn!(path = %entry.path().display(), "cannot stat library entry; skipped");
            continue;
        };
        children.push((
            name,
            entry.path(),
            file_type.is_dir(),
            file_type.is_symlink(),
        ));
    }
    children.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, path, is_dir, is_symlink) in children {
        if is_excluded(&path, excludes) {
            debug!(path = %path.display(), "library entry is excluded; skipped");
            continue;
        }
        if is_symlink {
            debug!(path = %path.display(), "skipping symlink (not followed)");
            continue;
        }

        let wanted = match referenced {
            Some(hashes) => hashes.contains(&name),
            None => true,
        };
        if !is_dir || !is_blob_hash(&name) || !wanted {
            scan.skipped_blobs = scan.skipped_blobs.saturating_add(1);
            scan.skipped_bytes = scan.skipped_bytes.saturating_add(tree_bytes(&path, is_dir));
            debug!(
                path = %path.display(),
                "library entry is not a referenced blob; not archived"
            );
            continue;
        }

        let Some(dir_name) = entry_name(FILES_ENTRY_PREFIX, Path::new(&name)) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            warn!(path = %path.display(), "blob directory vanished while scanning; skipped");
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        out.push(PlannedEntry {
            source: path.clone(),
            name: format!("{dir_name}/"),
            is_dir: true,
        });
        let (files, bytes) = scan_tree(&path, &format!("{dir_name}/"), excludes, out);
        scan.blobs = scan.blobs.saturating_add(1);
        scan.files = scan.files.saturating_add(files);
        scan.bytes = scan.bytes.saturating_add(bytes);
    }

    scan
}

/// Bytes a skipped library entry occupies. Metadata only: a blob that is not
/// going into the archive is never opened.
fn tree_bytes(path: &Path, is_dir: bool) -> u64 {
    if !is_dir {
        return std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    }
    WalkDir::new(path)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .fold(0u64, |total, meta| total.saturating_add(meta.len()))
}

/// Reads exactly `size` bytes from `inner`: truncating if the file grew since
/// it was stat-ed, zero-padding if it shrank. A tar header promises a byte
/// count up front, so a file changing underneath us must not desynchronise
/// the stream — better a padded entry plus a warning than a corrupt archive.
pub(super) struct ExactReader<R: Read> {
    inner: R,
    remaining: u64,
    pub(super) padded: u64,
}

impl<R: Read> ExactReader<R> {
    pub(super) fn new(inner: R, size: u64) -> Self {
        Self {
            inner,
            remaining: size,
            padded: 0,
        }
    }
}

impl<R: Read> Read for ExactReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let slice = buf
            .get_mut(..want)
            .ok_or_else(|| io::Error::other("short buffer"))?;
        let read = self.inner.read(slice)?;
        if read == 0 {
            slice.fill(0);
            self.remaining = self.remaining.saturating_sub(want as u64);
            self.padded = self.padded.saturating_add(want as u64);
            return Ok(want);
        }
        self.remaining = self.remaining.saturating_sub(read as u64);
        Ok(read)
    }
}

/// Build a tar header for `meta` that preserves mtime and mode but never
/// leaks the local uid/gid.
fn header_for(meta: &std::fs::Metadata) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_metadata_in_mode(meta, tar::HeaderMode::Complete);
    header.set_uid(0);
    header.set_gid(0);
    header
}

/// Write `manifest.json` + entries as a zstd-compressed tar into `out`.
/// Synchronous; callers run it inside `spawn_blocking`. Returns the manifest
/// and the inner writer (so the caller can `finish` the encryptor).
///
/// The zstd frame is closed before `out` is handed back, so the caller only
/// has to finish whatever sits *below* this writer.
pub fn write_payload<W: Write>(
    out: W,
    inputs: &ArchiveInputs,
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<(Manifest, W), ArchiveError> {
    progress(ProgressEvent::Scanning);

    let excludes: Vec<PathBuf> = inputs
        .exclude
        .iter()
        .map(|path| canonical_or_owned(path))
        .collect();

    let mut planned: Vec<PlannedEntry> = Vec::new();

    // 1. The database snapshot, always first after the manifest.
    let db_meta = std::fs::metadata(&inputs.db_snapshot).map_err(|error| {
        ArchiveError::Other(format!(
            "database snapshot {} is missing: {error}",
            inputs.db_snapshot.display()
        ))
    })?;
    if !db_meta.is_file() {
        return Err(ArchiveError::Other(format!(
            "database snapshot {} is not a file",
            inputs.db_snapshot.display()
        )));
    }
    let mut total_bytes = db_meta.len();
    planned.push(PlannedEntry {
        source: inputs.db_snapshot.clone(),
        name: DB_ENTRY.to_string(),
        is_dir: false,
    });

    // 2. Files library: only the blobs the archived database references.
    let mut files_count = 0u64;
    let mut files_root = None;
    if let Some(root) = inputs.files_root.as_ref() {
        if root.is_dir() {
            let scan = scan_library(
                root,
                inputs.referenced_blob_hashes.as_ref(),
                &excludes,
                &mut planned,
            );
            files_count = scan.files;
            total_bytes = total_bytes.saturating_add(scan.bytes);
            files_root = Some(root.to_string_lossy().to_string());
            if scan.skipped_blobs > 0 {
                info!(
                    packed_blobs = scan.blobs,
                    skipped_blobs = scan.skipped_blobs,
                    skipped_bytes = scan.skipped_bytes,
                    "left unreferenced library entries out of the archive"
                );
            }
        } else {
            warn!(path = %root.display(), "files library root is missing; not archived");
        }
    }

    // 3. Vault.
    let mut vault_file_count = 0u64;
    let mut vault_root = None;
    if let Some(root) = inputs.vault_root.as_ref() {
        if root.is_dir() {
            let (count, bytes) = scan_tree(root, VAULT_ENTRY_PREFIX, &excludes, &mut planned);
            vault_file_count = count;
            total_bytes = total_bytes.saturating_add(bytes);
            vault_root = Some(root.to_string_lossy().to_string());
        } else {
            warn!(path = %root.display(), "vault folder is missing; not archived");
        }
    }

    // 4. Saved web articles.
    let mut web_archive_file_count = 0u64;
    if let Some(root) = inputs.web_archive_root.as_ref() {
        if root.is_dir() {
            let (count, bytes) = scan_tree(root, WEB_ARCHIVE_ENTRY_PREFIX, &excludes, &mut planned);
            web_archive_file_count = count;
            total_bytes = total_bytes.saturating_add(bytes);
        } else {
            warn!(path = %root.display(), "web archive folder is missing; not archived");
        }
    }
    // 4b. Frozen web page copies, beside the web archive. Absent until the
    // first capture, so a missing folder is not worth a warning.
    if let Some(root) = inputs
        .web_archive_root
        .as_deref()
        .and_then(crate::features::backup::archive::format::web_snapshots_root_beside)
    {
        if root.is_dir() {
            let (_, bytes) = scan_tree(&root, WEB_SNAPSHOTS_ENTRY_PREFIX, &excludes, &mut planned);
            total_bytes = total_bytes.saturating_add(bytes);
        }
    }
    // 4c. Pictures chat answers show, beside the web archive. Absent until
    // the first `find_image`, so a missing folder is not worth a warning.
    if let Some(root) = inputs
        .web_archive_root
        .as_deref()
        .and_then(crate::features::backup::archive::format::chat_images_root_beside)
    {
        if root.is_dir() {
            let (_, bytes) = scan_tree(&root, CHAT_IMAGES_ENTRY_PREFIX, &excludes, &mut planned);
            total_bytes = total_bytes.saturating_add(bytes);
        }
    }

    // 5. settings.json.
    let mut settings_included = false;
    if let Some(settings) = inputs.settings_file.as_ref() {
        match std::fs::metadata(settings) {
            Ok(meta) if meta.is_file() => {
                settings_included = true;
                total_bytes = total_bytes.saturating_add(meta.len());
                planned.push(PlannedEntry {
                    source: settings.clone(),
                    name: SETTINGS_ENTRY.to_string(),
                    is_dir: false,
                });
            }
            Ok(_) => warn!(path = %settings.display(), "settings path is not a file; skipped"),
            Err(error) => {
                warn!(path = %settings.display(), %error, "settings file unreadable; skipped");
            }
        }
    }

    // `total_plaintext_bytes` counts file contents only: the manifest itself
    // and tar padding are excluded, and the numbers come from the scan pass,
    // so a file that vanishes before it is packed still shows up here.
    let manifest = Manifest {
        version: FORMAT_VERSION,
        created_at: inputs.created_at.clone(),
        app_version: inputs.app_version.clone(),
        db_entry: DB_ENTRY.to_string(),
        excluded_tables: EXCLUDED_TABLES.iter().map(|t| (*t).to_string()).collect(),
        migration_version: inputs.migration_version,
        files_root,
        files_count,
        vault_root,
        vault_file_count,
        settings_included,
        total_plaintext_bytes: total_bytes,
    };

    let encoder = zstd::Encoder::new(out, ZSTD_LEVEL)?;
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);

    // The manifest must be byte-for-byte reproducible for identical inputs,
    // so its header carries no timestamp and no local mode.
    let manifest_json = serde_json::to_vec(&manifest)?;
    let mut manifest_header = tar::Header::new_gnu();
    manifest_header.set_entry_type(tar::EntryType::Regular);
    manifest_header.set_size(manifest_json.len() as u64);
    manifest_header.set_mode(0o644);
    manifest_header.set_mtime(0);
    manifest_header.set_uid(0);
    manifest_header.set_gid(0);
    builder.append_data(
        &mut manifest_header,
        MANIFEST_ENTRY,
        manifest_json.as_slice(),
    )?;

    let mut bytes_done = 0u64;
    for entry in &planned {
        if entry.is_dir {
            let Ok(meta) = std::fs::metadata(&entry.source) else {
                warn!(path = %entry.source.display(), "directory vanished during backup; skipped");
                continue;
            };
            let mut header = header_for(&meta);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            builder.append_data(&mut header, entry.name.as_str(), io::empty())?;
            continue;
        }

        let mut file = match File::open(&entry.source) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                warn!(path = %entry.source.display(), "file vanished during backup; skipped");
                continue;
            }
            Err(error) => {
                warn!(path = %entry.source.display(), %error, "file unreadable; skipped");
                continue;
            }
        };
        let meta = match file.metadata() {
            Ok(meta) => meta,
            Err(error) => {
                warn!(path = %entry.source.display(), %error, "cannot stat file; skipped");
                continue;
            }
        };
        if !meta.is_file() {
            warn!(path = %entry.source.display(), "no longer a regular file; skipped");
            continue;
        }

        let size = meta.len();
        let mut header = header_for(&meta);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(size);

        let mut reader = ExactReader::new(&mut file, size);
        builder.append_data(&mut header, entry.name.as_str(), &mut reader)?;
        if reader.padded > 0 {
            warn!(
                path = %entry.source.display(),
                padded = reader.padded,
                "file shrank while being archived; entry zero-padded"
            );
        }

        bytes_done = bytes_done.saturating_add(size);
        progress(ProgressEvent::Entry {
            path: entry.name.clone(),
            bytes_done,
            bytes_total: total_bytes,
        });
    }

    let encoder = builder.into_inner()?;
    let inner = encoder.finish()?;
    progress(ProgressEvent::Done);

    info!(
        files = manifest.files_count,
        vault_files = manifest.vault_file_count,
        web_archive_files = web_archive_file_count,
        bytes = manifest.total_plaintext_bytes,
        "packed backup payload"
    );

    Ok((manifest, inner))
}
