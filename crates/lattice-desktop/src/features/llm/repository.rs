use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

pub async fn chat_model_row(
    db: &Database,
    conversation_id: &str,
) -> Result<Option<(Option<String>, String)>> {
    let mut read = db.begin_read().await?;
    let row = sqlx::query_as("SELECT model_choice, title FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_optional(&mut *read)
        .await?;
    read.commit().await?;
    Ok(row)
}
