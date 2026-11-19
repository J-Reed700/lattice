use lattice_core::shared::error::{Result, ResultExt};
use lattice_core::shared::persistence::Database;

pub async fn database_size_bytes(db: &Database) -> Result<i64> {
    let mut read = db.begin_read().await?;
    let (page_count, page_size): (i64, i64) =
        sqlx::query_as("SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()")
            .fetch_one(&mut *read)
            .await
            .context("Failed to read database size")?;
    read.commit().await?;

    Ok(page_count * page_size)
}
