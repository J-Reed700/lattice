use futures::FutureExt;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

pub async fn chat_model_row(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<(Option<String>, String)>> {
    Ok(
        sqlx::query_as("SELECT model_choice, title FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_optional(pool)
            .boxed()
            .await?,
    )
}
