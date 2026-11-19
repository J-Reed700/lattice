//! What Explorer keeps in `lattice.db`: `conversations.explorer_root`, and
//! the folders list in `explorer_folders`.

use lattice_core::shared::persistence::{Database, WriteTx};
use lattice_core::shared::{AppError, Result};
use sqlx::{Row, SqliteConnection};
use std::collections::HashMap;

/// Binds a conversation to a folder (a canonical root), or unbinds it.
pub async fn set_conversation_root(
    db: &Database,
    conversation_id: &str,
    root: Option<&str>,
) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("set the Explorer folder"))?;
    set_conversation_root_in(&mut tx, conversation_id, root).await?;
    tx.commit()
        .await
        .map_err(folder_error("set the Explorer folder"))
}

/// [`set_conversation_root`] inside the caller's unit of work.
pub async fn set_conversation_root_in(
    tx: &mut WriteTx,
    conversation_id: &str,
    root: Option<&str>,
) -> Result<()> {
    let result = sqlx::query("UPDATE conversations SET explorer_root = ? WHERE id = ?")
        .bind(root)
        .bind(conversation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to set the Explorer folder: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Conversation not found: {conversation_id}"
        )));
    }
    Ok(())
}

/// The folder a conversation is bound to; `None` for an ordinary chat.
pub async fn conversation_root(db: &Database, conversation_id: &str) -> Result<Option<String>> {
    let mut read = db
        .begin_read()
        .await
        .map_err(folder_error("read the Explorer folder"))?;
    let root = conversation_root_in(&mut read, conversation_id).await?;
    read.commit()
        .await
        .map_err(folder_error("read the Explorer folder"))?;
    Ok(root)
}

/// [`conversation_root`] inside the caller's unit of work.
pub async fn conversation_root_in(
    conn: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar::<_, Option<String>>("SELECT explorer_root FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_optional(&mut *conn)
        .await
        .map(Option::flatten)
        .map_err(|e| AppError::Database(format!("Failed to read the Explorer folder: {e}")))
}

/// One row of `explorer_folders`. Times are RFC 3339, UTC, to the
/// millisecond, so they sort as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub root: String,
    pub name: String,
    pub pinned: bool,
    pub added_at: String,
    pub last_opened_at: String,
    pub instructions: Option<String>,
    /// `None` is General.
    pub space_id: Option<String>,
    /// The thread the folder's chat last showed; `None` when it has none or
    /// that thread was deleted.
    pub last_thread_id: Option<String>,
}

/// The space a folder with no space of its own files its threads in.
pub const GENERAL_SPACE_ID: &str = "space_general";

fn folder_error<E: std::fmt::Display>(action: &str) -> impl Fn(E) -> AppError + '_ {
    move |e| AppError::Database(format!("Failed to {action}: {e}"))
}

fn folder_not_found(root: &str) -> AppError {
    AppError::NotFound(format!("Folder not in the Explorer's list: {root}"))
}

/// Adds the folder (named `name`), or marks it opened at `now`.
pub async fn record_folder_opened(db: &Database, root: &str, name: &str, now: &str) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("record the Explorer folder"))?;
    sqlx::query(
        "INSERT INTO explorer_folders (root, name, pinned, added_at, last_opened_at)
         VALUES (?1, ?2, 0, ?3, ?3)
         ON CONFLICT(root) DO UPDATE SET last_opened_at = excluded.last_opened_at",
    )
    .bind(root)
    .bind(name)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(folder_error("record the Explorer folder"))?;
    tx.commit()
        .await
        .map_err(folder_error("record the Explorer folder"))
}

/// Adds a folder whose index is on disk but which has no row (one indexed
/// before the list existed). A row already there wins.
pub async fn adopt_folder(db: &Database, root: &str, name: &str, opened_at: &str) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("add the Explorer folder"))?;
    sqlx::query(
        "INSERT INTO explorer_folders (root, name, pinned, added_at, last_opened_at)
         VALUES (?1, ?2, 0, ?3, ?3)
         ON CONFLICT(root) DO NOTHING",
    )
    .bind(root)
    .bind(name)
    .bind(opened_at)
    .execute(&mut *tx)
    .await
    .map_err(folder_error("add the Explorer folder"))?;
    tx.commit()
        .await
        .map_err(folder_error("add the Explorer folder"))
}

/// Every folder: pinned first, then the most recently opened.
pub async fn list_folders(db: &Database) -> Result<Vec<FolderRow>> {
    let mut read = db
        .begin_read()
        .await
        .map_err(folder_error("list the Explorer folders"))?;
    let rows = sqlx::query(
        "SELECT root, name, pinned, added_at, last_opened_at, instructions, space_id,
                last_thread_id
         FROM explorer_folders
         ORDER BY pinned DESC, last_opened_at DESC, root",
    )
    .fetch_all(&mut *read)
    .await
    .map_err(folder_error("list the Explorer folders"))?;
    read.commit()
        .await
        .map_err(folder_error("list the Explorer folders"))?;
    Ok(rows
        .into_iter()
        .map(|row| FolderRow {
            root: row.get("root"),
            name: row.get("name"),
            pinned: row.get::<i64, _>("pinned") != 0,
            added_at: row.get("added_at"),
            instructions: row.get("instructions"),
            space_id: row.get("space_id"),
            last_opened_at: row.get("last_opened_at"),
            last_thread_id: row.get("last_thread_id"),
        })
        .collect())
}

pub async fn rename_folder(db: &Database, root: &str, name: &str) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("rename the Explorer folder"))?;
    let result = sqlx::query("UPDATE explorer_folders SET name = ? WHERE root = ?")
        .bind(name)
        .bind(root)
        .execute(&mut *tx)
        .await
        .map_err(folder_error("rename the Explorer folder"))?;
    if result.rows_affected() == 0 {
        return Err(folder_not_found(root));
    }
    tx.commit()
        .await
        .map_err(folder_error("rename the Explorer folder"))
}

pub async fn set_folder_pinned(db: &Database, root: &str, pinned: bool) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("pin the Explorer folder"))?;
    let result = sqlx::query("UPDATE explorer_folders SET pinned = ? WHERE root = ?")
        .bind(i64::from(pinned))
        .bind(root)
        .execute(&mut *tx)
        .await
        .map_err(folder_error("pin the Explorer folder"))?;
    if result.rows_affected() == 0 {
        return Err(folder_not_found(root));
    }
    tx.commit()
        .await
        .map_err(folder_error("pin the Explorer folder"))
}

/// Records the thread the folder's chat is showing. A folder not listed yet
/// is added (named `name`, opened at `now`), as opening it would add it.
pub async fn set_folder_last_thread_in(
    tx: &mut WriteTx,
    root: &str,
    name: &str,
    conversation_id: &str,
    now: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO explorer_folders (root, name, pinned, added_at, last_opened_at, last_thread_id)
         VALUES (?1, ?2, 0, ?3, ?3, ?4)
         ON CONFLICT(root) DO UPDATE SET last_thread_id = excluded.last_thread_id",
    )
    .bind(root)
    .bind(name)
    .bind(now)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(folder_error("remember the folder's thread"))
}

/// Sets a folder's instructions and space, and moves its threads into that
/// space, inside the caller's unit of work. `space_id` is `None` for General.
/// Returns how many threads moved.
pub async fn set_folder_settings_in(
    tx: &mut WriteTx,
    root: &str,
    instructions: Option<&str>,
    space_id: Option<&str>,
    now: &str,
) -> Result<u64> {
    let result =
        sqlx::query("UPDATE explorer_folders SET instructions = ?, space_id = ? WHERE root = ?")
            .bind(instructions)
            .bind(space_id)
            .bind(root)
            .execute(&mut **tx)
            .await
            .map_err(folder_error("save the folder's settings"))?;
    if result.rows_affected() == 0 {
        return Err(folder_not_found(root));
    }
    let target = space_id.unwrap_or(GENERAL_SPACE_ID);
    let moved = sqlx::query(
        "UPDATE conversations SET space_id = ?, updated_at = ?
         WHERE explorer_root = ? AND space_id != ?",
    )
    .bind(target)
    .bind(now)
    .bind(root)
    .bind(target)
    .execute(&mut **tx)
    .await
    .map_err(folder_error("move the folder's threads to its space"))?
    .rows_affected();
    Ok(moved)
}

/// The space a folder's threads belong to: its own, or General when it has
/// none or is not listed yet.
pub async fn folder_space_in(conn: &mut SqliteConnection, root: &str) -> Result<String> {
    let space: Option<Option<String>> =
        sqlx::query_scalar("SELECT space_id FROM explorer_folders WHERE root = ?")
            .bind(root)
            .fetch_optional(&mut *conn)
            .await
            .map_err(folder_error("read the folder's space"))?;
    Ok(space
        .flatten()
        .unwrap_or_else(|| GENERAL_SPACE_ID.to_string()))
}

/// Whether `space_id` names a space a folder can file its threads in.
pub async fn space_is_open_in(conn: &mut SqliteConnection, space_id: &str) -> Result<bool> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_spaces WHERE id = ? AND is_archived = 0",
    )
    .bind(space_id)
    .fetch_one(&mut *conn)
    .await
    .map(|count| count > 0)
    .map_err(folder_error("check the space"))
}

/// Moves a thread into the space its folder files threads in.
pub async fn move_thread_to_space_in(
    tx: &mut WriteTx,
    conversation_id: &str,
    space_id: &str,
    now: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE conversations SET space_id = ?, updated_at = ? WHERE id = ? AND space_id != ?",
    )
    .bind(space_id)
    .bind(now)
    .bind(conversation_id)
    .bind(space_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(folder_error("move the thread to its folder's space"))
}

/// Drops the folder's row. Its threads and index are the caller's business.
pub async fn delete_folder(db: &Database, root: &str) -> Result<()> {
    let mut tx = db
        .begin_write()
        .await
        .map_err(folder_error("remove the Explorer folder"))?;
    sqlx::query("DELETE FROM explorer_folders WHERE root = ?")
        .bind(root)
        .execute(&mut *tx)
        .await
        .map_err(folder_error("remove the Explorer folder"))?;
    tx.commit()
        .await
        .map_err(folder_error("remove the Explorer folder"))
}

/// How many threads each folder has, keyed by root.
pub async fn thread_counts(db: &Database) -> Result<HashMap<String, u32>> {
    let mut read = db
        .begin_read()
        .await
        .map_err(folder_error("count the Explorer threads"))?;
    let rows = sqlx::query(
        "SELECT explorer_root, COUNT(*) AS threads FROM conversations
         WHERE explorer_root IS NOT NULL AND tangent_parent_id IS NULL GROUP BY explorer_root",
    )
    .fetch_all(&mut *read)
    .await
    .map_err(folder_error("count the Explorer threads"))?;
    read.commit()
        .await
        .map_err(folder_error("count the Explorer threads"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let threads: i64 = row.get("threads");
            (
                row.get("explorer_root"),
                u32::try_from(threads).unwrap_or(u32::MAX),
            )
        })
        .collect())
}

/// The ids of the folder's top-level threads. Their tangents cascade with
/// them, so returning both would try to delete a child that is already gone.
pub async fn folder_threads(db: &Database, root: &str) -> Result<Vec<String>> {
    let mut read = db
        .begin_read()
        .await
        .map_err(folder_error("list the Explorer threads"))?;
    let threads = sqlx::query_scalar(
        "SELECT id FROM conversations WHERE explorer_root = ? AND tangent_parent_id IS NULL",
    )
    .bind(root)
    .fetch_all(&mut *read)
    .await
    .map_err(folder_error("list the Explorer threads"))?;
    read.commit()
        .await
        .map_err(folder_error("list the Explorer threads"))?;
    Ok(threads)
}

fn save_error(error: impl std::fmt::Display) -> AppError {
    AppError::Database(format!("Failed to save the Explorer folder: {error}"))
}

/// Sets a folder's instructions and space and moves its threads with it, in
/// one unit of work. A named space must be open.
pub async fn save_folder_settings(
    db: &Database,
    root: &str,
    instructions: Option<&str>,
    own_space: Option<&str>,
    now: &str,
) -> Result<u64> {
    let mut tx = db.begin_write().await.map_err(save_error)?;
    if let Some(space) = own_space {
        if !space_is_open_in(&mut tx, space).await? {
            return Err(AppError::NotFound(format!(
                "Space not found or archived: {space}"
            )));
        }
    }
    let moved = set_folder_settings_in(&mut tx, root, instructions, own_space, now).await?;
    tx.commit().await.map_err(save_error)?;
    Ok(moved)
}

/// Binds a thread to a folder and files it in the folder's space, in one unit
/// of work.
pub async fn bind_thread_to_folder(
    db: &Database,
    conversation_id: &str,
    root: Option<&str>,
    now: &str,
) -> Result<()> {
    let mut tx = db.begin_write().await.map_err(save_error)?;
    set_conversation_root_in(&mut tx, conversation_id, root).await?;
    if let Some(root) = root {
        let space = folder_space_in(&mut tx, root).await?;
        move_thread_to_space_in(&mut tx, conversation_id, &space, now).await?;
    }
    tx.commit().await.map_err(save_error)
}

/// Records the thread a folder's chat shows, if the thread is bound to the
/// folder. `Ok(false)` when it is not.
pub async fn remember_folder_thread(
    db: &Database,
    root: &str,
    name: &str,
    conversation_id: &str,
    now: &str,
) -> Result<bool> {
    let mut tx = db.begin_write().await.map_err(save_error)?;
    if conversation_root_in(&mut tx, conversation_id)
        .await?
        .as_deref()
        != Some(root)
    {
        return Ok(false);
    }
    set_folder_last_thread_in(&mut tx, root, name, conversation_id, now).await?;
    tx.commit().await.map_err(save_error)?;
    Ok(true)
}
