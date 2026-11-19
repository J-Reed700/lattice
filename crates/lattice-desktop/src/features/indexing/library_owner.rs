//! Which database owns the files the startup sweeps delete.
//!
//! The sweeps in `bootstrap` remove files under the lattice home that no row of
//! this database names. In a database that never imported them, a fresh one
//! made when a corrupt `lattice.db` was moved aside or the one of a second copy
//! of the app, that is every file. A leaked file costs disk and a deleted one is
//! gone, so a database sweeps only files it owns: it holds an id, the home holds
//! a file with the same id, and [`may_sweep`] says yes only where they agree.
//! Startup asks once and keeps the answer in the `SweepGate` that every sweep
//! reads afterwards.

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::fs::roots;
use lattice_core::shared::persistence::Database;

mod repository;
use repository::{holds_documents, recorded_owner};

/// Where the files the startup sweeps delete live, and the file that names
/// their owner.
#[derive(Debug, Clone)]
pub struct LibraryFiles {
    pub owner_file: PathBuf,
    /// The imported files, web snapshots and chat pictures.
    pub roots: Vec<PathBuf>,
}

impl LibraryFiles {
    /// The locations under the lattice home, which `LATTICE_HOME` moves.
    pub fn current() -> Result<Self> {
        Ok(Self {
            owner_file: roots::library_owner_file()?,
            roots: vec![
                roots::library_root()?,
                roots::web_snapshots_root()?,
                roots::chat_images_root()?,
            ],
        })
    }
}

/// Whether this database owns the files in `files`, so the sweeps may delete
/// what it does not reference. A database with no id yet takes the files when
/// nothing else claims them, writing its id to the database and to the home.
/// Any error means no; it is logged, never raised.
pub async fn may_sweep(database: &Database, files: &LibraryFiles) -> bool {
    match claim(database, files).await {
        Ok(None) => true,
        Ok(Some(reason)) => {
            tracing::warn!(
                reason,
                owner_file = %files.owner_file.display(),
                "Not deleting files that no row of this database names"
            );
            false
        }
        Err(error) => {
            tracing::warn!(
                %error,
                owner_file = %files.owner_file.display(),
                "Not deleting files that no row of this database names: could not tell whether this database owns them"
            );
            false
        }
    }
}

/// `None` when this database owns the files, else why it does not.
async fn claim(database: &Database, files: &LibraryFiles) -> Result<Option<&'static str>> {
    let recorded = recorded_owner(database).await?;
    let filed = filed_owner(&files.owner_file).await?;
    let pending = sidecar_owner_path(&files.owner_file, ".pending");
    let restore_marker = sidecar_owner_path(&files.owner_file, ".restore");
    match (recorded, filed) {
        (Some(recorded), Some(filed)) if recorded == filed => {
            let _ = tokio::fs::remove_file(&pending).await;
            let _ = tokio::fs::remove_file(&restore_marker).await;
            Ok(None)
        }
        (Some(_), Some(_)) => Ok(Some("the owner file names a different database")),
        (None, Some(filed)) => {
            // A backup restore clears `library_owner` from the restored snapshot
            // and drops `owner.restore` so this host's existing `owner` file is
            // adopted cleanly on restart instead of permanently disabling sweeps.
            if matches!(tokio::fs::try_exists(&restore_marker).await, Ok(true)) {
                adopt_restored_owner(database, &filed, &restore_marker).await?;
                return Ok(None);
            }
            Ok(Some(
                "the owner file names another database and this one has no id yet",
            ))
        }
        (Some(recorded), None) => {
            // Crash recovery: if `adopt()` committed `recorded` to SQLite and
            // crashed right before promoting `owner.pending` -> `owner`, finish
            // promoting it (without overwriting) even when the library already
            // contains files.
            if filed_owner(&pending).await?.as_deref() == Some(recorded.as_str()) {
                promote_pending_owner_file(&pending, &files.owner_file, &recorded).await?;
                tracing::info!(
                    owner_id = %recorded,
                    owner_file = %files.owner_file.display(),
                    "Completed pending library owner file promotion after crash"
                );
                return Ok(None);
            }
            if holds_files(&files.roots).await? {
                return Ok(Some(
                    "the owner file is missing and the library already holds files",
                ));
            }
            write_owner_file(&files.owner_file, &recorded).await?;
            Ok(None)
        }
        (None, None) => {
            let _ = tokio::fs::remove_file(&restore_marker).await;
            if holds_files(&files.roots).await? && !holds_documents(database).await? {
                return Ok(Some(
                    "this database has no documents and the library already holds files",
                ));
            }
            adopt(database, &files.owner_file).await?;
            Ok(None)
        }
    }
}

/// The id this database holds, if it has one.
/// The id in the owner file, if the file exists.
async fn filed_owner(path: &Path) -> Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(Some(text.trim().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(file_error("read", path, &error)),
    }
}

/// Whether any of `directories` holds a file, at any depth.
async fn holds_files(directories: &[PathBuf]) -> Result<bool> {
    for root in directories {
        let mut pending = vec![root.clone()];
        while let Some(directory) = pending.pop() {
            let mut entries = match tokio::fs::read_dir(&directory).await {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(file_error("read", &directory, &error)),
            };
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|error| file_error("read", &directory, &error))?
            {
                let kind = entry
                    .file_type()
                    .await
                    .map_err(|error| file_error("read", &entry.path(), &error))?;
                if !kind.is_dir() {
                    return Ok(true);
                }
                pending.push(entry.path());
            }
        }
    }
    Ok(false)
}

fn sidecar_owner_path(owner_file: &Path, suffix: &str) -> PathBuf {
    let mut raw = owner_file.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// Give this database an id and the home a file naming it.
///
/// Writes `<owner_file>.pending` and syncs it to disk first, commits the row
/// in SQLite (with zero non-SQL awaits inside `WriteTx`), and then promotes
/// `.pending` to `owner_file` via non-overwriting `hard_link` (falling back to
/// `create_new(true)` if cross-device/unsupported). A crash before commit
/// leaves SQLite at `None`; a crash after commit is recovered by `claim`.
async fn adopt(database: &Database, owner_file: &Path) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let pending = sidecar_owner_path(owner_file, ".pending");
    write_pending_owner_file(&pending, &id).await?;

    if let Err(error) = commit_owner(database, &id).await {
        let _ = tokio::fs::remove_file(&pending).await;
        return Err(error);
    }

    promote_pending_owner_file(&pending, owner_file, &id).await?;
    tracing::info!(
        owner_id = %id,
        owner_file = %owner_file.display(),
        "This database now owns the library files"
    );
    Ok(())
}

async fn commit_owner(database: &Database, id: &str) -> Result<()> {
    let mut tx = database.begin_write().await?;
    repository::record_owner(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

/// Promote `pending` to `owner_file` without ever overwriting an existing
/// `owner_file` created concurrently by a racing instance.
async fn promote_pending_owner_file(pending: &Path, owner_file: &Path, id: &str) -> Result<()> {
    match tokio::fs::hard_link(pending, owner_file).await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(pending).await;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = tokio::fs::remove_file(pending).await;
            Err(file_error("create", owner_file, &error))
        }
        Err(_) => {
            let result = write_owner_file(owner_file, id).await;
            let _ = tokio::fs::remove_file(pending).await;
            result
        }
    }
}

async fn adopt_restored_owner(
    database: &Database,
    filed: &str,
    restore_marker: &Path,
) -> Result<()> {
    let mut tx = database.begin_write().await?;
    repository::record_owner(&mut tx, filed).await?;
    tx.commit().await?;
    let _ = tokio::fs::remove_file(restore_marker).await;
    tracing::info!(
        owner_id = %filed,
        "Adopted host library owner id after backup restore"
    );
    Ok(())
}

/// Write and fsync `<owner_file>.pending`, replacing any stale staging file
/// left by an earlier pre-commit crash.
async fn write_pending_owner_file(path: &Path, id: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| file_error("create", parent, &error))?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await
        .map_err(|error| file_error("create", path, &error))?;
    if let Err(error) = fill(&mut file, id).await {
        drop(file);
        let _ = tokio::fs::remove_file(path).await;
        return Err(file_error("write", path, &error));
    }
    Ok(())
}

/// Create the owner file holding `id`. A file already there is never replaced.
async fn write_owner_file(path: &Path, id: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| file_error("create", parent, &error))?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map_err(|error| file_error("create", path, &error))?;
    if let Err(error) = fill(&mut file, id).await {
        drop(file);
        let _ = tokio::fs::remove_file(path).await;
        return Err(file_error("write", path, &error));
    }
    Ok(())
}

async fn fill(file: &mut tokio::fs::File, id: &str) -> std::io::Result<()> {
    file.write_all(format!("{id}\n").as_bytes()).await?;
    file.sync_all().await
}

fn file_error(action: &str, path: &Path, error: &std::io::Error) -> AppError {
    AppError::FileStorage(format!("Failed to {action} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests;
