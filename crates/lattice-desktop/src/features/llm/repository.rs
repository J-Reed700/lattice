use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::driver::DriverPool;

pub async fn chat_model_row(
    pool: &impl DriverPool,
    conversation_id: &str,
) -> Result<Option<(Option<String>, String)>> {
    let pool = pool.sqlx_pool();
    Ok(
        sqlx::query_as("SELECT model_choice, title FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_optional(pool)
            .await?,
    )
}
