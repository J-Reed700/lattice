use lattice_core::shared::error::{Result, ResultExt};
use lattice_core::shared::persistence::DatabaseHandle;

pub async fn database_size_bytes(pool: &impl DatabaseHandle) -> Result<i64> {
    let pool = pool.sqlx_pool();
    let (page_count, page_size): (i64, i64) =
        sqlx::query_as("SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()")
            .fetch_one(pool)
            .await
            .context("Failed to read database size")?;

    Ok(page_count * page_size)
}
