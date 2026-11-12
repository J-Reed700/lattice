use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;

pub async fn mark_interrupted_verifications(db: &Database) -> Result<u64, AppError> {
    let mut tx = db.begin_write().await?;
    let result = sqlx::query(
        "UPDATE conversation_messages \
         SET metadata = json_set(metadata, '$.verification.pending', json('false'), \
                                          '$.verification.interrupted', json('true')) \
         WHERE json_valid(metadata) \
           AND json_extract(metadata, '$.verification.pending') = 1",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

pub async fn fail_unfinished_turns(db: &Database, resuming_json: String) -> Result<u64, AppError> {
    let mut tx = db.begin_write().await?;
    let result = sqlx::query(
        "UPDATE conversation_messages SET status = 'failed' \
         WHERE status IN ('pending', 'streaming') \
           AND id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(resuming_json)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}
