//! "Your folders": every folder picked in the Explorer, with its threads and
//! what its index holds.
//!
//! The rows are `explorer_folders` in `lattice.db`; the indexes are the
//! directories under `<data_dir>/folder-index/`, read without opening them.
//! An index directory with no row (one made before the list existed) is
//! adopted the next time the list is read, under the root its meta records.
//! Nothing here deletes an index or a thread unless the user asked.

use super::dto::{ExplorerFolderDto, ExplorerFolderListDto};
use super::index::manager::FolderIndexManager;
use super::repository;
use crate::features::conversation::commands as conversation_commands;
use crate::interfaces::di::Container;
use crate::shared::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::info;

/// RFC 3339, UTC, to the millisecond: how `explorer_folders` stores times,
/// so they sort as text.
fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The name a folder is listed under until it is renamed: its own.
fn default_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

/// The folder at `root` (canonical) was opened: listed if it is new, marked
/// opened now if not.
pub async fn record_open(pool: &SqlitePool, root: &str) -> Result<()> {
    repository::record_folder_opened(
        pool,
        root,
        &default_name(Path::new(root)),
        &stamp(Utc::now()),
    )
    .await
}

/// Every folder, pinned first and then the most recently opened, each with
/// its thread count and its index: live for the open folder, read from disk
/// for the rest.
pub async fn list(
    pool: &SqlitePool,
    manager: &FolderIndexManager,
) -> Result<ExplorerFolderListDto> {
    let indexes = manager.index_dirs().await;
    let mut rows = repository::list_folders(pool).await?;
    let listed: HashSet<&str> = rows.iter().map(|row| row.root.as_str()).collect();
    let unlisted: Vec<(&Path, i64)> = indexes
        .iter()
        .filter_map(|entry| Some((entry.root.as_deref()?, entry.last_opened)))
        .filter(|(root, _)| !listed.contains(root.to_string_lossy().as_ref()))
        .collect();
    if !unlisted.is_empty() {
        for (root, last_opened) in unlisted {
            let opened = DateTime::from_timestamp_millis(last_opened)
                .filter(|_| last_opened > 0)
                .unwrap_or_else(Utc::now);
            repository::adopt_folder(
                pool,
                &root.to_string_lossy(),
                &default_name(root),
                &stamp(opened),
            )
            .await?;
            info!(root = %root.display(), "Explorer folder adopted from its index");
        }
        rows = repository::list_folders(pool).await?;
    }

    let threads = repository::thread_counts(pool).await?;
    let roots: Vec<PathBuf> = rows.iter().map(|row| PathBuf::from(&row.root)).collect();
    let summaries = manager.summaries(&roots, &indexes).await;
    let folders = rows
        .into_iter()
        .zip(summaries)
        .map(|(row, index)| ExplorerFolderDto {
            exists: Path::new(&row.root).is_dir(),
            thread_count: threads.get(&row.root).copied().unwrap_or(0),
            root: row.root,
            name: row.name,
            pinned: row.pinned,
            added_at: row.added_at,
            last_opened_at: row.last_opened_at,
            index,
            instructions: row.instructions,
            space_id: row
                .space_id
                .unwrap_or_else(|| repository::GENERAL_SPACE_ID.to_string()),
            last_thread_id: row.last_thread_id,
        })
        .collect();
    Ok(ExplorerFolderListDto {
        home: manager
            .config()
            .home
            .as_ref()
            .map(|home| home.to_string_lossy().into_owned()),
        folders,
    })
}

/// Renames a folder in the list. An empty name goes back to the folder's own.
pub async fn rename(pool: &SqlitePool, root: &str, name: &str) -> Result<()> {
    let name = name.trim();
    let name = if name.is_empty() {
        default_name(Path::new(root))
    } else {
        name.chars().take(120).collect()
    };
    repository::rename_folder(pool, root, &name).await
}

/// Longest system prompt a folder keeps, in characters.
const MAX_INSTRUCTIONS_CHARS: usize = 20_000;

/// Sets a folder's instructions and space. Blank instructions are none, and
/// General is stored as no space of its own. The folder's threads move into
/// the space with it, so its library and memory change for them too. Returns
/// how many threads moved.
pub async fn set_settings(
    pool: &SqlitePool,
    root: &str,
    instructions: &str,
    space_id: &str,
) -> Result<u64> {
    let instructions = instructions.trim();
    if instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
        return Err(crate::shared::AppError::InvalidInput(format!(
            "Folder instructions are limited to {MAX_INSTRUCTIONS_CHARS} characters."
        )));
    }
    let space_id = space_id.trim();
    let own_space =
        (!space_id.is_empty() && space_id != repository::GENERAL_SPACE_ID).then_some(space_id);
    if let Some(space) = own_space {
        if !repository::space_is_open(pool, space).await? {
            return Err(crate::shared::AppError::NotFound(format!(
                "Space not found or archived: {space}"
            )));
        }
    }
    let moved = repository::set_folder_settings(
        pool,
        root,
        (!instructions.is_empty()).then_some(instructions),
        own_space,
        &stamp(Utc::now()),
    )
    .await?;
    info!(
        root,
        space = own_space.unwrap_or(repository::GENERAL_SPACE_ID),
        threads_moved = moved,
        "Explorer folder settings saved"
    );
    Ok(moved)
}

/// Binds a thread to a folder and files it in the folder's space, so a new
/// thread never takes the space from whatever the Chat sidebar had selected.
pub async fn bind_thread(
    pool: &SqlitePool,
    conversation_id: &str,
    root: Option<&str>,
) -> Result<()> {
    repository::set_conversation_root(pool, conversation_id, root).await?;
    if let Some(root) = root {
        let space = repository::folder_space(pool, root).await?;
        repository::move_thread_to_space(pool, conversation_id, &space, &stamp(Utc::now())).await?;
    }
    Ok(())
}

/// Remembers the thread the folder's chat is showing, so the folder reopens
/// on it. Only a thread bound to the folder can be its thread.
pub async fn remember_thread(pool: &SqlitePool, root: &str, conversation_id: &str) -> Result<()> {
    if repository::conversation_root(pool, conversation_id)
        .await?
        .as_deref()
        != Some(root)
    {
        return Err(crate::shared::AppError::InvalidInput(format!(
            "Conversation {conversation_id} is not a thread of {root}"
        )));
    }
    repository::set_folder_last_thread(
        pool,
        root,
        &default_name(Path::new(root)),
        conversation_id,
        &stamp(Utc::now()),
    )
    .await
}

pub async fn set_pinned(pool: &SqlitePool, root: &str, pinned: bool) -> Result<()> {
    repository::set_folder_pinned(pool, root, pinned).await
}

/// Deletes the folder's own index and keeps it listed, so the next open
/// builds it again. An enclosing folder's index it reuses is left alone.
pub async fn delete_index(manager: &FolderIndexManager, root: &str) -> Result<()> {
    manager.delete_index(root).await
}

/// Removes a folder from the list: its own index, then, when asked, its
/// threads, then its row. Threads go through the app's conversation delete,
/// which takes their messages and attachments with them; one removal spends
/// one delete's rate limit however many threads it takes. Threads that are
/// kept stay bound to the folder and come back if it is added again. Returns
/// how many threads were deleted.
pub async fn remove(
    container: &Container,
    manager: &FolderIndexManager,
    root: &str,
    delete_threads: bool,
) -> Result<usize> {
    let pool = container.db_pool();
    let threads = if delete_threads {
        repository::folder_threads(pool, root).await?
    } else {
        Vec::new()
    };
    if !threads.is_empty() {
        conversation_commands::check_delete_rate_limit(container).await?;
    }
    manager.delete_index(root).await?;
    let deleted = threads.len();
    for conversation_id in threads {
        conversation_commands::delete_conversation_unmetered(container, conversation_id).await?;
    }
    repository::delete_folder(pool, root).await?;
    info!(root, threads_deleted = deleted, "Explorer folder removed");
    Ok(deleted)
}
