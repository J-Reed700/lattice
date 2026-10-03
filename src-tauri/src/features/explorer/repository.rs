//! The one column Explorer owns: `conversations.explorer_root`.

use crate::shared::{AppError, Result};
use sqlx::SqlitePool;

/// Binds a conversation to a folder (a canonical root), or unbinds it.
pub async fn set_conversation_root(
    pool: &SqlitePool,
    conversation_id: &str,
    root: Option<&str>,
) -> Result<()> {
    let result = sqlx::query("UPDATE conversations SET explorer_root = ? WHERE id = ?")
        .bind(root)
        .bind(conversation_id)
        .execute(pool)
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
pub async fn conversation_root(pool: &SqlitePool, conversation_id: &str) -> Result<Option<String>> {
    sqlx::query_scalar::<_, Option<String>>("SELECT explorer_root FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
        .map_err(|e| AppError::Database(format!("Failed to read the Explorer folder: {e}")))
}
