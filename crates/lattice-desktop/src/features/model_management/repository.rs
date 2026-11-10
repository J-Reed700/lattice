use sqlx::Row;

use super::cache_adapter::ModelCatalogStats;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::DatabaseHandle;

pub async fn initialize_cache_schema(pool: &impl DatabaseHandle) -> Result<(), AppError> {
    let pool = pool.sqlx_pool();
    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS model_catalog_cache (
                query TEXT PRIMARY KEY,
                results_json TEXT NOT NULL,
                cached_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_model_catalog_cache_expires
            ON model_catalog_cache(expires_at);
            "#,
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to initialize cache schema: {}", e)))?;

    Ok(())
}

pub async fn cached_results_json(
    pool: &impl DatabaseHandle,
    cache_key: &str,
    now: i64,
) -> Result<Option<String>, AppError> {
    let pool = pool.sqlx_pool();
    let row = sqlx::query(
        r#"
            SELECT results_json, expires_at
            FROM model_catalog_cache
            WHERE query = ? AND expires_at > ?
            "#,
    )
    .bind(cache_key)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Cache lookup failed: {}", e)))?;

    Ok(row.map(|row| row.get("results_json")))
}

pub async fn store_results_json(
    pool: &impl DatabaseHandle,
    cache_key: &str,
    results_json: &str,
    now: i64,
    expires_at: i64,
) -> Result<(), AppError> {
    let pool = pool.sqlx_pool();
    sqlx::query(
        r#"
            INSERT INTO model_catalog_cache (query, results_json, cached_at, expires_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(query) DO UPDATE SET
                results_json = excluded.results_json,
                cached_at = excluded.cached_at,
                expires_at = excluded.expires_at
            "#,
    )
    .bind(cache_key)
    .bind(results_json)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to store cache: {}", e)))?;
    Ok(())
}

pub async fn delete_expired(pool: &impl DatabaseHandle, now: i64) -> Result<u64, AppError> {
    let pool = pool.sqlx_pool();
    let result = sqlx::query(
        r#"
            DELETE FROM model_catalog_cache
            WHERE expires_at <= ?
            "#,
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to clear expired cache: {}", e)))?;
    Ok(result.rows_affected())
}

pub async fn delete_all(pool: &impl DatabaseHandle) -> Result<u64, AppError> {
    let pool = pool.sqlx_pool();
    let result = sqlx::query("DELETE FROM model_catalog_cache")
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to clear cache: {}", e)))?;
    Ok(result.rows_affected())
}

pub async fn cache_stats(
    pool: &impl DatabaseHandle,
    now: i64,
) -> Result<ModelCatalogStats, AppError> {
    let pool = pool.sqlx_pool();
    let row = sqlx::query(
        r#"
            SELECT
                COUNT(*) as total_entries,
                SUM(CASE WHEN expires_at > ? THEN 1 ELSE 0 END) as valid_entries,
                SUM(CASE WHEN expires_at <= ? THEN 1 ELSE 0 END) as expired_entries
            FROM model_catalog_cache
            "#,
    )
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to get cache stats: {}", e)))?;

    Ok(ModelCatalogStats {
        total_entries: row.get::<i64, _>("total_entries") as u64,
        valid_entries: row.get::<i64, _>("valid_entries") as u64,
        expired_entries: row.get::<i64, _>("expired_entries") as u64,
    })
}
