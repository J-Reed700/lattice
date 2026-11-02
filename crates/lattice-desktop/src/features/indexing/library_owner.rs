//! Which database owns the files the startup sweeps delete.
//!
//! The sweeps in `bootstrap` remove files under the lattice home that no row of
//! this database names. In a database that never imported them, a fresh one
//! made when a corrupt `lattice.db` was moved aside or the one of a second copy
//! of the app, that is every file. A leaked file costs disk and a deleted one is
//! gone, so a database sweeps only files it owns: it holds an id, the home holds
//! a file with the same id, and [`may_sweep`] says yes only where they agree.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::fs::roots;

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
pub async fn may_sweep(pool: &SqlitePool, files: &LibraryFiles) -> bool {
    match claim(pool, files).await {
        Ok(None) => true,
        Ok(Some(reason)) => {
            tracing::warn!(
                reason,
                owner_file = %files.owner_file.display(),
                "Not collecting unreferenced library files"
            );
            false
        }
        Err(error) => {
            tracing::warn!(
                %error,
                owner_file = %files.owner_file.display(),
                "Not collecting unreferenced library files: could not tell whether this database owns them"
            );
            false
        }
    }
}

/// `None` when this database owns the files, else why it does not.
async fn claim(pool: &SqlitePool, files: &LibraryFiles) -> Result<Option<&'static str>> {
    let recorded = recorded_owner(pool).await?;
    let filed = filed_owner(&files.owner_file).await?;
    match (recorded, filed) {
        (Some(recorded), Some(filed)) if recorded == filed => Ok(None),
        (Some(_), Some(_)) => Ok(Some("the owner file names a different database")),
        (None, Some(_)) => Ok(Some(
            "the owner file names another database and this one has no id yet",
        )),
        (Some(recorded), None) => {
            if holds_files(&files.roots).await? {
                return Ok(Some(
                    "the owner file is missing and the library already holds files",
                ));
            }
            write_owner_file(&files.owner_file, &recorded).await?;
            Ok(None)
        }
        (None, None) => {
            if holds_files(&files.roots).await? && !holds_documents(pool).await? {
                return Ok(Some(
                    "this database has no documents and the library already holds files",
                ));
            }
            adopt(pool, &files.owner_file).await?;
            Ok(None)
        }
    }
}

/// The id this database holds, if it has one.
async fn recorded_owner(pool: &SqlitePool) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT owner_id FROM library_owner WHERE id = 1")
            .fetch_optional(pool)
            .await?,
    )
}

/// The id in the owner file, if the file exists.
async fn filed_owner(path: &Path) -> Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(Some(text.trim().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(file_error("read", path, &error)),
    }
}

async fn holds_documents(pool: &SqlitePool) -> Result<bool> {
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents)")
            .fetch_one(pool)
            .await?,
    )
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

/// Give this database an id and the home a file naming it. The file is written
/// before the id commits, so a file that cannot be written leaves the database
/// as it was and the next start tries again.
async fn adopt(pool: &SqlitePool, owner_file: &Path) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, ?)")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    write_owner_file(owner_file, &id).await?;
    tx.commit().await?;
    tracing::info!(
        owner_id = %id,
        owner_file = %owner_file.display(),
        "This database now owns the library files"
    );
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
