use lattice_core::shared::error::AppError;

pub async fn mark_interrupted_verifications(pool: &sqlx::SqlitePool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE conversation_messages \
         SET metadata = json_set(metadata, '$.verification.pending', json('false'), \
                                          '$.verification.interrupted', json('true')) \
         WHERE json_valid(metadata) \
           AND json_extract(metadata, '$.verification.pending') = 1",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn fail_unfinished_turns(
    pool: &sqlx::SqlitePool,
    resuming_json: String,
) -> Result<u64, AppError> {
    let result = sqlx::query(
        "UPDATE conversation_messages SET status = 'failed' \
         WHERE status IN ('pending', 'streaming') \
           AND id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(resuming_json)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
