//! Database index inspection helpers.

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;

/// Return application indexes for diagnostics.
pub async fn get_index_stats(pool: &impl DatabaseHandle) -> Result<Vec<IndexInfo>> {
    let pool = pool.sqlx_pool();
    let indexes = sqlx::query_as::<_, IndexInfo>(
        r#"
        SELECT name, tbl_name as table_name, sql
        FROM sqlite_master
        WHERE type = 'index' AND name LIKE 'idx_%'
        ORDER BY tbl_name, name
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(indexes)
}

/// Index information for diagnostics.
#[derive(Debug, sqlx::FromRow)]
pub struct IndexInfo {
    pub name: String,
    pub table_name: String,
    pub sql: Option<String>,
}
