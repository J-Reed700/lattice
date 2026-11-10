//! The persistent half of the owner cache: `sourcing_owner_cache`.
use lattice_core::shared::persistence::DatabaseHandle;
use sqlx::Row;

pub(super) struct CachedOwnerRow {
    pub origins_json: String,
    pub resolved_by: Option<String>,
    pub updated_at: i64,
}

pub(super) async fn find(
    pool: &impl DatabaseHandle,
    cache_key: &str,
) -> Result<Option<CachedOwnerRow>, sqlx::Error> {
    let pool = pool.sqlx_pool();
    let row = sqlx::query(
        "SELECT origins_json, resolved_by, updated_at FROM sourcing_owner_cache WHERE cache_key = ?",
    )
    .bind(cache_key)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| CachedOwnerRow {
        origins_json: row.get("origins_json"),
        resolved_by: row.get("resolved_by"),
        updated_at: row.get("updated_at"),
    }))
}

pub(super) async fn upsert(
    pool: &impl DatabaseHandle,
    cache_key: &str,
    subject_name: &str,
    subject_kind: &str,
    origins_json: &str,
    resolved_by: &str,
    updated_at: i64,
) -> Result<(), sqlx::Error> {
    let pool = pool.sqlx_pool();
    sqlx::query(
        r#"INSERT INTO sourcing_owner_cache
           (cache_key, subject_name, subject_kind, origins_json, resolved_by, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(cache_key) DO UPDATE SET
               origins_json = excluded.origins_json,
               resolved_by = excluded.resolved_by,
               updated_at = excluded.updated_at"#,
    )
    .bind(cache_key)
    .bind(subject_name)
    .bind(subject_kind)
    .bind(origins_json)
    .bind(resolved_by)
    .bind(updated_at)
    .execute(pool)
    .await?;
    Ok(())
}
