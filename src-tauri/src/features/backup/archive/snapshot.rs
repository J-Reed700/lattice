//! Payload side of a `.lattice-backup` archive: the SQLite snapshot with
//! derivable tables cleared, plus the files library, vault, and settings,
//! packed as a zstd-compressed tar.
//!
//! Layout of the plaintext payload (see `format.rs` for the framing):
//!
//! ```text
//! zstd( tar( manifest.json, db/lattice.db, files/**, vault/**, settings.json ) )
//! ```
//!
//! `manifest.json` is always the first tar entry so a reader can learn the
//! counts and byte totals without decompressing the rest of the stream.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use super::format::{
    ArchiveError, Manifest, DB_ENTRY, FILES_ENTRY_PREFIX, FORMAT_VERSION, MANIFEST_ENTRY,
    SETTINGS_ENTRY, VAULT_ENTRY_PREFIX,
};

/// Tables whose rows are cleared from the snapshot. All are re-derivable
/// from documents, chunks, and conversations after a restore.
pub const EXCLUDED_TABLES: &[&str] = &[
    "text_embeddings",
    "image_embeddings",
    "conversation_memory_vectors",
    "chunk_sparse_terms",
    "cluster_members",
    "clusters",
    "cluster_runs",
    "chat_starter_cache",
];

/// zstd compression level for the payload. 3 is the zstd default: it keeps
/// the CPU cost low enough that a scheduled backup stays invisible, while
/// still roughly halving a SQLite snapshot.
const ZSTD_LEVEL: i32 = 3;

/// Hard ceiling on the manifest entry, so a hostile payload cannot make the
/// reader allocate arbitrarily before the rest of the tar is even inspected.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

/// Sidecar suffixes SQLite may leave next to a database file.
const DB_SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

/// What `snapshot_database` produced.
#[derive(Debug, Clone)]
pub struct SnapshotReport {
    pub path: PathBuf,
    pub bytes: u64,
    pub excluded_tables: Vec<String>,
    pub migration_version: Option<i64>,
}

fn db_err(err: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Database(err.to_string())
}

/// Reject the handful of characters that could end a `VACUUM INTO '...'`
/// statement early. Mirrors `BackupAdapter::validate_sql_safe_path`: the real
/// protection is the single-quote escaping below, this is defence in depth.
fn validate_sql_safe_path(path: &str) -> Result<(), ArchiveError> {
    if path.contains(';') {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL statement terminator (;)".to_string(),
        ));
    }
    if path.contains("--") {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL comment sequence (--)".to_string(),
        ));
    }
    if path.contains("/*") || path.contains("*/") {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL comment sequence (/* */)".to_string(),
        ));
    }
    if path.contains('\0') {
        return Err(ArchiveError::Other(
            "snapshot path contains null byte".to_string(),
        ));
    }
    Ok(())
}

/// Append a suffix to a path's file name (`foo.db` -> `foo.db-wal`).
fn sidecar_path(db: &Path, suffix: &str) -> PathBuf {
    let mut raw = db.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// Delete any `-wal`/`-shm`/`-journal` file left beside `db`. A snapshot must
/// be a single self-contained file: the tar only carries `db/lattice.db`, so a
/// stray sidecar would silently make the archived database stale.
fn remove_db_sidecars(db: &Path) {
    for suffix in DB_SIDECAR_SUFFIXES {
        let path = sidecar_path(db, suffix);
        if !path.exists() {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => debug!(path = %path.display(), "removed snapshot sidecar"),
            Err(error) => warn!(
                path = %path.display(),
                %error,
                "failed to remove snapshot sidecar; the archive may be incomplete"
            ),
        }
    }
}

/// Does `table` exist in the connected database?
async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, ArchiveError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(db_err)?;
    Ok(count > 0)
}

/// Extract the table named by an `content='...'` FTS5 option, if any.
fn fts_external_content_table(sql: &str) -> Option<String> {
    let lowered = sql.to_lowercase();
    let at = lowered.find("content=")?;
    let rest = sql.get(at.checked_add("content=".len())?..)?;
    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let value: String = chars.take_while(|c| *c != quote).collect();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Deal with the FTS5 indexes after the excluded tables have been emptied.
///
/// Decision, from reading the schema (`migrations/20260916000000_init_schema.sql`):
///
/// `chunks_fts`, `documents_fts` and `conversation_search_fts` are *ordinary*
/// FTS5 tables. None of them declares a `content=` option, so each stores its
/// own text in its `%_content` shadow table, and each is kept in sync by
/// AFTER INSERT/UPDATE/DELETE triggers on `text_chunks`, `conversations`,
/// `conversation_messages` and `conversation_message_bookmarks` — every one of
/// which is a table we *keep*. Consequently:
///
/// * they are not external-content indexes over a cleared table, so they are
///   not stale after the deletes above;
/// * `INSERT INTO x(x) VALUES('rebuild')` would only re-derive the index from
///   the very shadow content we are already copying — pure cost, no benefit;
/// * `DELETE FROM x` would throw the text away with nothing left to rebuild
///   it from, since no trigger fires during a restore (the base rows are
///   copied by the snapshot, not re-inserted), permanently breaking search.
///
/// So the FTS content is carried verbatim. The loop below only exists so that
/// a *future* migration introducing `content='<an excluded table>'` is handled
/// correctly (rebuild against the now-empty content table) instead of shipping
/// an index that points at rows the archive no longer has.
async fn reconcile_fts_tables(
    pool: &SqlitePool,
    cleared: &[String],
) -> Result<Vec<String>, ArchiveError> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'table' AND sql IS NOT NULL AND lower(sql) LIKE '%using fts5%'",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut rebuilt = Vec::new();
    for (name, sql) in rows {
        let Some(sql) = sql else { continue };
        let Some(content_table) = fts_external_content_table(&sql) else {
            debug!(table = %name, "fts5 index stores its own content; kept verbatim");
            continue;
        };
        if !cleared.iter().any(|t| t == &content_table) {
            debug!(
                table = %name,
                content = %content_table,
                "external-content fts5 index over a table we keep; left alone"
            );
            continue;
        }
        warn!(
            table = %name,
            content = %content_table,
            "external-content fts5 index over a cleared table; rebuilding it empty"
        );
        let escaped = name.replace('"', "\"\"");
        sqlx::query(&format!(
            "INSERT INTO \"{escaped}\"(\"{escaped}\") VALUES('rebuild')"
        ))
        .execute(pool)
        .await
        .map_err(db_err)?;
        rebuilt.push(name);
    }
    Ok(rebuilt)
}

/// `VACUUM INTO dest` from the live pool, then clear `EXCLUDED_TABLES` (and
/// FTS shadow content where needed) in the copy and `VACUUM` it so the file
/// shrinks. The result has no `-wal`/`-shm` sidecars.
pub async fn snapshot_database(
    pool: &SqlitePool,
    dest: &Path,
) -> Result<SnapshotReport, ArchiveError> {
    let dest_str = dest.to_string_lossy().to_string();
    validate_sql_safe_path(&dest_str)?;

    if dest.exists() {
        return Err(ArchiveError::Other(format!(
            "snapshot destination already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    // `VACUUM INTO` takes a string literal, not a bind parameter. Escape the
    // single quotes exactly like `BackupAdapter::create_backup` does.
    let escaped = dest_str.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(pool)
        .await
        .map_err(|error| {
            ArchiveError::Database(format!("VACUUM INTO {} failed: {error}", dest.display()))
        })?;

    // Scratch pool over the copy. One connection, rollback journal (so no
    // `-wal`/`-shm` appear), foreign keys off: every excluded table is
    // re-derivable, so we do not care about the order the rows disappear in.
    // (For the record, `PRAGMA foreign_key_list` on them gives
    // text_embeddings -> text_chunks, image_embeddings -> documents,
    // conversation_memory_vectors -> conversations, chunk_sparse_terms ->
    // text_chunks, cluster_members -> clusters/documents, clusters ->
    // cluster_runs. The list above is already child-before-parent, so the
    // order would be safe even with enforcement on.)
    let options = SqliteConnectOptions::new()
        .filename(dest)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let scratch = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ArchiveError::Database(format!("failed to open snapshot copy: {error}"))
        })?;

    let outcome = clear_snapshot(&scratch).await;
    scratch.close().await;
    let (excluded_tables, migration_version) = match outcome {
        Ok(value) => value,
        Err(error) => {
            remove_db_sidecars(dest);
            return Err(error);
        }
    };

    remove_db_sidecars(dest);

    let bytes = tokio::fs::metadata(dest).await?.len();
    info!(
        path = %dest.display(),
        bytes,
        cleared = excluded_tables.len(),
        migration_version,
        "prepared backup database snapshot"
    );

    Ok(SnapshotReport {
        path: dest.to_path_buf(),
        bytes,
        excluded_tables,
        migration_version,
    })
}

/// Everything that happens on the scratch pool, so the caller can always
/// close it afterwards.
async fn clear_snapshot(pool: &SqlitePool) -> Result<(Vec<String>, Option<i64>), ArchiveError> {
    let mut cleared: Vec<String> = Vec::new();
    for table in EXCLUDED_TABLES {
        if !table_exists(pool, table).await? {
            debug!(table = %table, "excluded table absent from snapshot; skipping");
            continue;
        }
        let escaped = table.replace('"', "\"\"");
        sqlx::query(&format!("DELETE FROM \"{escaped}\""))
            .execute(pool)
            .await
            .map_err(|error| ArchiveError::Database(format!("failed to clear {table}: {error}")))?;
        cleared.push((*table).to_string());
    }

    reconcile_fts_tables(pool, &cleared).await?;

    // Reclaim the pages the deletes freed. Must run outside a transaction,
    // which it does here because sqlx executes it on a bare connection.
    sqlx::query("VACUUM")
        .execute(pool)
        .await
        .map_err(|error| ArchiveError::Database(format!("VACUUM of snapshot failed: {error}")))?;

    let migration_version = if table_exists(pool, "_sqlx_migrations").await? {
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    Ok((cleared, migration_version))
}

/// Everything the payload writer packs.
#[derive(Debug, Clone, Default)]
pub struct ArchiveInputs {
    /// Output of `snapshot_database`.
    pub db_snapshot: PathBuf,
    pub migration_version: Option<i64>,
    /// Content-addressed files library root (`~/.lattice/files`), if it exists.
    pub files_root: Option<PathBuf>,
    /// Vault markdown folder, if the vault is enabled and the folder exists.
    pub vault_root: Option<PathBuf>,
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

/// Reads exactly `size` bytes from `inner`: truncating if the file grew since
/// it was stat-ed, zero-padding if it shrank. A tar header promises a byte
/// count up front, so a file changing underneath us must not desynchronise
/// the stream — better a padded entry plus a warning than a corrupt archive.
struct ExactReader<R: Read> {
    inner: R,
    remaining: u64,
    padded: u64,
}

impl<R: Read> ExactReader<R> {
    fn new(inner: R, size: u64) -> Self {
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

    // 2. Files library.
    let mut files_count = 0u64;
    let mut files_root = None;
    if let Some(root) = inputs.files_root.as_ref() {
        if root.is_dir() {
            let (count, bytes) = scan_tree(root, FILES_ENTRY_PREFIX, &excludes, &mut planned);
            files_count = count;
            total_bytes = total_bytes.saturating_add(bytes);
            files_root = Some(root.to_string_lossy().to_string());
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

    // 4. settings.json.
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
        bytes = manifest.total_plaintext_bytes,
        "packed backup payload"
    );

    Ok((manifest, inner))
}

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
    pub settings_file: Option<PathBuf>,
}

/// Validate a tar entry path and render it as a forward-slashed string.
fn checked_entry_name(path: &Path) -> Result<String, ArchiveError> {
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
enum EntryGroup {
    Manifest,
    Db,
    Files,
    Vault,
    Settings,
}

/// Reject anything outside the four known prefixes. Directory entries arrive
/// without their trailing slash once `Path` has parsed them, hence the bare
/// `"db"`/`"files"`/`"vault"` cases.
fn classify_entry(name: &str) -> Result<EntryGroup, ArchiveError> {
    let db_prefix = DB_ENTRY.split('/').next().unwrap_or("db");
    let files_dir = FILES_ENTRY_PREFIX.trim_end_matches('/');
    let vault_dir = VAULT_ENTRY_PREFIX.trim_end_matches('/');

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
        settings_file: saw_settings.then(|| under(dest_dir, SETTINGS_ENTRY)),
    })
}

/// Facts about an extracted snapshot, checked before it is swapped in.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub migration_version: Option<i64>,
    pub table_count: u64,
    pub document_count: u64,
    pub conversation_count: u64,
}

async fn count_rows(pool: &SqlitePool, table: &str) -> Result<u64, ArchiveError> {
    if !table_exists(pool, table).await? {
        return Ok(0);
    }
    let escaped = table.replace('"', "\"\"");
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{escaped}\""))
        .fetch_one(pool)
        .await
        .map_err(db_err)?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// `PRAGMA integrity_check` plus a few counts on a snapshot file.
pub async fn validate_snapshot(db: &Path) -> Result<SnapshotInfo, ArchiveError> {
    if !db.is_file() {
        return Err(ArchiveError::Corrupt(format!(
            "snapshot {} is missing",
            db.display()
        )));
    }

    // Read-only, rollback journal: never create a `-wal`/`-shm` beside a file
    // that may live on read-only or sync-backed storage.
    let options = SqliteConnectOptions::new()
        .filename(db)
        .read_only(true)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ArchiveError::Corrupt(format!("{} is not a database: {error}", db.display()))
        })?;

    let result = inspect_snapshot(&pool).await;
    pool.close().await;
    result
}

async fn inspect_snapshot(pool: &SqlitePool) -> Result<SnapshotInfo, ArchiveError> {
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .map_err(|error| ArchiveError::Corrupt(format!("integrity check failed: {error}")))?;
    if integrity != "ok" {
        return Err(ArchiveError::Corrupt(format!(
            "snapshot failed integrity check: {integrity}"
        )));
    }

    let migration_version = if table_exists(pool, "_sqlx_migrations").await? {
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    let tables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
        .fetch_one(pool)
        .await
        .map_err(db_err)?;

    Ok(SnapshotInfo {
        migration_version,
        table_count: u64::try_from(tables).unwrap_or(0),
        document_count: count_rows(pool, "documents").await?,
        conversation_count: count_rows(pool, "conversations").await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    type TestBuilder = tar::Builder<zstd::Encoder<'static, Vec<u8>>>;

    async fn migrated_pool(db_path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn read_only_pool(db_path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .read_only(true)
            .journal_mode(SqliteJournalMode::Delete);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap()
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    // ---------------------------------------------------------------- snapshot

    #[tokio::test]
    async fn snapshot_clears_derivable_tables_and_keeps_everything_else() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("lattice.db");
        let pool = migrated_pool(&live).await;

        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
             VALUES ('doc-1', '/tmp/a.md', 'a.md', 12, '2026-09-16T00:00:00Z', 'abc')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index) \
             VALUES ('chunk-1', 'doc-1', 'hello world', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO tags (id, name) VALUES ('tag-1', 'research')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) \
             VALUES ('emb-1', 'chunk-1', ?, 'bge-small', 4)",
        )
        .bind(vec![0u8, 1, 2, 3])
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO chat_starter_cache (fingerprint, starters_json, created_at) \
             VALUES ('fp', '[]', '2026-09-16T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let dest = dir.path().join("snapshot.db");
        let report = snapshot_database(&pool, &dest).await.unwrap();
        pool.close().await;

        assert_eq!(report.path, dest);
        assert!(report.bytes > 0, "snapshot should not be empty");
        assert!(report.migration_version.is_some());
        assert!(report
            .excluded_tables
            .iter()
            .any(|t| t == "text_embeddings"));
        assert!(report
            .excluded_tables
            .iter()
            .any(|t| t == "chat_starter_cache"));
        for suffix in DB_SIDECAR_SUFFIXES {
            assert!(
                !sidecar_path(&dest, suffix).exists(),
                "snapshot left a {suffix} sidecar behind"
            );
        }

        let snap = read_only_pool(&dest).await;
        assert_eq!(count(&snap, "SELECT COUNT(*) FROM documents").await, 1);
        assert_eq!(count(&snap, "SELECT COUNT(*) FROM tags").await, 1);
        assert_eq!(count(&snap, "SELECT COUNT(*) FROM text_chunks").await, 1);
        assert_eq!(
            count(&snap, "SELECT COUNT(*) FROM text_embeddings").await,
            0
        );
        assert_eq!(
            count(&snap, "SELECT COUNT(*) FROM chat_starter_cache").await,
            0
        );
        // FTS content rides along verbatim: the triggers that fill it only
        // fire on writes to `text_chunks`, which a restore never replays.
        assert_eq!(count(&snap, "SELECT COUNT(*) FROM chunks_fts").await, 1);
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&snap)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        snap.close().await;

        let info = validate_snapshot(&dest).await.unwrap();
        assert_eq!(info.document_count, 1);
        assert_eq!(info.conversation_count, 0);
        assert_eq!(info.migration_version, report.migration_version);
        assert!(info.table_count > 10, "schema should survive the snapshot");
    }

    #[tokio::test]
    async fn snapshot_refuses_an_existing_destination_and_unsafe_paths() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("lattice.db");
        let pool = migrated_pool(&live).await;

        let taken = dir.path().join("taken.db");
        std::fs::write(&taken, b"x").unwrap();
        assert!(matches!(
            snapshot_database(&pool, &taken).await,
            Err(ArchiveError::Other(_))
        ));

        let nasty = dir.path().join("evil'; DROP TABLE documents; --.db");
        assert!(matches!(
            snapshot_database(&pool, &nasty).await,
            Err(ArchiveError::Other(_))
        ));
        pool.close().await;
    }

    #[test]
    fn detects_external_content_fts_tables() {
        assert_eq!(
            fts_external_content_table("CREATE VIRTUAL TABLE t USING fts5(c, content='documents')")
                .as_deref(),
            Some("documents")
        );
        assert_eq!(
            fts_external_content_table("CREATE VIRTUAL TABLE t USING fts5(c, content=\"docs\")")
                .as_deref(),
            Some("docs")
        );
        // The schema this app actually ships: no `content=` option at all.
        assert_eq!(
            fts_external_content_table(
                "CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED, content)"
            ),
            None
        );
    }

    // ----------------------------------------------------------------- payload

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        db: PathBuf,
        files_root: PathBuf,
        vault_root: PathBuf,
        settings: PathBuf,
        inputs: ArchiveInputs,
    }

    const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    const PDF_BYTES: &[u8] = b"%PDF-1.4 pretend this is a pdf";
    const NOTE_NAME: &str = "\u{fc}n\u{ef}code note.md";
    const NOTE_BYTES: &[u8] = "# h\u{e9}llo w\u{f6}rld\n".as_bytes();
    const SETTINGS_BYTES: &[u8] = br#"{"theme":"dark"}"#;
    const DB_BYTES: &[u8] = b"SQLite format 3\0not really, but bytes are bytes";

    fn fixture() -> Fixture {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();

        let db = root.join("snapshot.db");
        write_file(&db, DB_BYTES);

        let files_root = root.join("files");
        write_file(&files_root.join(SHA).join("name.pdf"), PDF_BYTES);

        let vault_root = root.join("vault");
        write_file(
            &vault_root.join("notes").join("sub").join(NOTE_NAME),
            NOTE_BYTES,
        );
        write_file(
            &vault_root.join("excluded").join("secret.md"),
            b"must not ship",
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            vault_root.join("notes").join("sub").join(NOTE_NAME),
            vault_root.join("shortcut.md"),
        )
        .unwrap();

        let settings = root.join("settings.json");
        write_file(&settings, SETTINGS_BYTES);

        let inputs = ArchiveInputs {
            db_snapshot: db.clone(),
            migration_version: Some(20_260_916_010_000),
            files_root: Some(files_root.clone()),
            vault_root: Some(vault_root.clone()),
            settings_file: Some(settings.clone()),
            exclude: vec![vault_root.join("excluded")],
            app_version: "1.2.3".to_string(),
            created_at: "2026-09-16T12:00:00Z".to_string(),
        };

        Fixture {
            _dir: dir,
            root,
            db,
            files_root,
            vault_root,
            settings,
            inputs,
        }
    }

    #[test]
    fn payload_round_trips_and_honours_excludes_and_symlinks() {
        let fx = fixture();
        let mut events: Vec<ProgressEvent> = Vec::new();
        let (manifest, bytes) =
            write_payload(Vec::new(), &fx.inputs, &mut |event| events.push(event)).unwrap();

        let expected_bytes =
            (DB_BYTES.len() + PDF_BYTES.len() + NOTE_BYTES.len() + SETTINGS_BYTES.len()) as u64;
        assert_eq!(manifest.version, FORMAT_VERSION);
        assert_eq!(manifest.db_entry, DB_ENTRY);
        assert_eq!(manifest.files_count, 1);
        assert_eq!(
            manifest.vault_file_count, 1,
            "symlink and excluded subtree must not count"
        );
        assert!(manifest.settings_included);
        assert_eq!(manifest.total_plaintext_bytes, expected_bytes);
        assert_eq!(manifest.migration_version, Some(20_260_916_010_000));
        assert_eq!(manifest.excluded_tables.len(), EXCLUDED_TABLES.len());
        assert!(manifest.files_root.is_some());
        assert!(manifest.vault_root.is_some());

        assert!(matches!(events.first(), Some(ProgressEvent::Scanning)));
        assert!(matches!(events.last(), Some(ProgressEvent::Done)));
        let entries: Vec<&ProgressEvent> = events
            .iter()
            .filter(|e| matches!(e, ProgressEvent::Entry { .. }))
            .collect();
        assert_eq!(entries.len(), 4, "db + 1 file + 1 vault note + settings");
        if let Some(ProgressEvent::Entry {
            bytes_done,
            bytes_total,
            ..
        }) = entries.last()
        {
            assert_eq!(*bytes_done, expected_bytes);
            assert_eq!(*bytes_total, expected_bytes);
        } else {
            panic!("expected a final Entry event");
        }

        // read_manifest sees the same manifest without unpacking anything.
        let from_stream = read_manifest(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(from_stream, manifest);

        let dest = fx.root.join("extracted");
        let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
        assert_eq!(extracted.manifest, manifest);
        assert_eq!(std::fs::read(&extracted.db_path).unwrap(), DB_BYTES);

        let files_dir = extracted.files_dir.clone().unwrap();
        assert_eq!(
            std::fs::read(files_dir.join(SHA).join("name.pdf")).unwrap(),
            PDF_BYTES
        );
        let vault_dir = extracted.vault_dir.clone().unwrap();
        assert_eq!(
            std::fs::read(vault_dir.join("notes").join("sub").join(NOTE_NAME)).unwrap(),
            NOTE_BYTES
        );
        assert!(
            !vault_dir.join("excluded").exists(),
            "excluded subtree leaked"
        );
        assert!(
            !vault_dir.join("shortcut.md").exists(),
            "symlink was packed"
        );
        assert_eq!(
            std::fs::read(extracted.settings_file.clone().unwrap()).unwrap(),
            SETTINGS_BYTES
        );

        // Sources untouched.
        assert!(fx.db.exists() && fx.files_root.exists() && fx.settings.exists());
        assert!(fx.vault_root.join("excluded").join("secret.md").exists());
    }

    #[test]
    fn payload_without_optional_inputs_still_round_trips() {
        let fx = fixture();
        let inputs = ArchiveInputs {
            db_snapshot: fx.db.clone(),
            migration_version: None,
            files_root: None,
            vault_root: None,
            settings_file: None,
            exclude: Vec::new(),
            app_version: "1.2.3".to_string(),
            created_at: "2026-09-16T12:00:00Z".to_string(),
        };
        let (manifest, bytes) = write_payload(Vec::new(), &inputs, &mut |_| {}).unwrap();
        assert_eq!(manifest.files_count, 0);
        assert_eq!(manifest.vault_file_count, 0);
        assert!(!manifest.settings_included);

        let dest = fx.root.join("extracted-minimal");
        let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
        assert!(extracted.files_dir.is_none());
        assert!(extracted.vault_dir.is_none());
        assert!(extracted.settings_file.is_none());
        assert_eq!(std::fs::read(&extracted.db_path).unwrap(), DB_BYTES);
    }

    #[test]
    fn write_payload_fails_without_a_snapshot() {
        let dir = tempdir().unwrap();
        let inputs = ArchiveInputs {
            db_snapshot: dir.path().join("nope.db"),
            ..ArchiveInputs::default()
        };
        assert!(matches!(
            write_payload(Vec::new(), &inputs, &mut |_| {}),
            Err(ArchiveError::Other(_))
        ));
    }

    // ---------------------------------------------------------- hostile tarballs

    fn manifest_fixture() -> Manifest {
        Manifest {
            version: FORMAT_VERSION,
            created_at: "2026-09-16T12:00:00Z".to_string(),
            app_version: "1.2.3".to_string(),
            db_entry: DB_ENTRY.to_string(),
            excluded_tables: Vec::new(),
            migration_version: None,
            files_root: None,
            files_count: 0,
            vault_root: None,
            vault_file_count: 0,
            settings_included: false,
            total_plaintext_bytes: 0,
        }
    }

    fn append_manifest(builder: &mut TestBuilder) {
        let json = serde_json::to_vec(&manifest_fixture()).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(json.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        builder
            .append_data(&mut header, MANIFEST_ENTRY, json.as_slice())
            .unwrap();
    }

    fn append_db(builder: &mut TestBuilder) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(DB_BYTES.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        builder
            .append_data(&mut header, DB_ENTRY, DB_BYTES)
            .unwrap();
    }

    /// tar-rs refuses to *write* `..` or absolute names, so those headers are
    /// stamped by hand to model what a hostile archive would contain.
    fn append_raw_name(builder: &mut TestBuilder, name: &[u8], data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        {
            let raw = header.as_mut_bytes();
            raw[..name.len()].copy_from_slice(name);
        }
        header.set_cksum();
        builder.append(&header, data).unwrap();
    }

    fn pack<F: FnOnce(&mut TestBuilder)>(with_manifest_first: bool, fill: F) -> Vec<u8> {
        let encoder = zstd::Encoder::new(Vec::new(), 1).unwrap();
        let mut builder = tar::Builder::new(encoder);
        if with_manifest_first {
            append_manifest(&mut builder);
        }
        fill(&mut builder);
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap()
    }

    fn extract_into_fresh(
        bytes: Vec<u8>,
        label: &str,
    ) -> (tempfile::TempDir, Result<ExtractedPayload, ArchiveError>) {
        let dir = tempdir().unwrap();
        let dest = dir.path().join(label);
        let result = extract_payload(Cursor::new(bytes), &dest, &mut |_| {});
        (dir, result)
    }

    #[test]
    fn extract_rejects_parent_dir_entries() {
        let bytes = pack(true, |b| {
            append_db(b);
            append_raw_name(b, b"../evil", b"pwned");
        });
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
    }

    #[test]
    fn extract_rejects_absolute_entries() {
        let bytes = pack(true, |b| {
            append_db(b);
            append_raw_name(b, b"/etc/evil", b"pwned");
        });
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
    }

    #[test]
    fn extract_rejects_symlink_entries() {
        let bytes = pack(true, |b| {
            append_db(b);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_mtime(0);
            b.append_link(&mut header, "vault/shortcut.md", "/etc/passwd")
                .unwrap();
        });
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
    }

    #[test]
    fn extract_rejects_entries_outside_the_known_prefixes() {
        let bytes = pack(true, |b| {
            append_db(b);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(3);
            header.set_mode(0o644);
            header.set_mtime(0);
            b.append_data(&mut header, "models/llm.gguf", &b"abc"[..])
                .unwrap();
        });
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
    }

    #[test]
    fn extract_requires_the_manifest_first() {
        let bytes = pack(false, |b| {
            append_db(b);
            append_manifest(b);
        });
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
        assert!(matches!(
            read_manifest(Cursor::new(pack(false, append_db))),
            Err(ArchiveError::Corrupt(_))
        ));
    }

    #[test]
    fn extract_requires_a_database_entry() {
        let bytes = pack(true, |_| {});
        let (_dir, result) = extract_into_fresh(bytes, "dest");
        assert!(
            matches!(result, Err(ArchiveError::Corrupt(_))),
            "{result:?}"
        );
    }

    #[test]
    fn extract_refuses_a_non_empty_destination() {
        let fx = fixture();
        let (_manifest, bytes) = write_payload(Vec::new(), &fx.inputs, &mut |_| {}).unwrap();
        let dest = fx.root.join("occupied");
        write_file(&dest.join("already-here.txt"), b"hi");
        let result = extract_payload(Cursor::new(bytes), &dest, &mut |_| {});
        assert!(matches!(result, Err(ArchiveError::Other(_))), "{result:?}");
    }

    #[test]
    fn entry_names_are_validated() {
        assert!(checked_entry_name(Path::new("vault/a/b.md")).is_ok());
        assert!(checked_entry_name(Path::new("../x")).is_err());
        assert!(checked_entry_name(Path::new("/x")).is_err());
        assert!(checked_entry_name(Path::new("")).is_err());
        assert!(classify_entry("manifest.json").is_ok());
        assert!(classify_entry("db/lattice.db").is_ok());
        assert!(classify_entry("files/aa/b.pdf").is_ok());
        assert!(classify_entry("vault").is_ok());
        assert!(classify_entry("settings.json").is_ok());
        assert!(classify_entry("db/other.db").is_err());
        assert!(classify_entry("elsewhere/x").is_err());
    }

    // -------------------------------------------------------- validate_snapshot

    #[tokio::test]
    async fn validate_snapshot_rejects_non_databases_and_truncated_files() {
        let dir = tempdir().unwrap();

        let missing = dir.path().join("absent.db");
        assert!(matches!(
            validate_snapshot(&missing).await,
            Err(ArchiveError::Corrupt(_))
        ));

        let garbage = dir.path().join("garbage.db");
        std::fs::write(&garbage, b"this is definitely not a sqlite database at all").unwrap();
        assert!(validate_snapshot(&garbage).await.is_err());

        let live = dir.path().join("lattice.db");
        let pool = migrated_pool(&live).await;
        let good = dir.path().join("good.db");
        snapshot_database(&pool, &good).await.unwrap();
        pool.close().await;
        assert!(validate_snapshot(&good).await.is_ok());

        let raw = std::fs::read(&good).unwrap();
        assert!(raw.len() > 8192, "schema snapshot should span many pages");
        let truncated = dir.path().join("truncated.db");
        std::fs::write(&truncated, &raw[..4096]).unwrap();
        assert!(
            validate_snapshot(&truncated).await.is_err(),
            "a truncated database must not validate"
        );
    }

    #[test]
    fn exact_reader_pads_a_shrinking_source() {
        let mut reader = ExactReader::new(Cursor::new(b"abc".to_vec()), 6);
        let mut out = Vec::new();
        io::copy(&mut reader, &mut out).unwrap();
        assert_eq!(out, b"abc\0\0\0");
        assert_eq!(reader.padded, 3);

        let mut reader = ExactReader::new(Cursor::new(b"abcdef".to_vec()), 3);
        let mut out = Vec::new();
        io::copy(&mut reader, &mut out).unwrap();
        assert_eq!(out, b"abc");
        assert_eq!(reader.padded, 0);
    }
}
