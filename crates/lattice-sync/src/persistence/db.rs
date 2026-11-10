use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn create_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
}

pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    let _: i64 = sqlx::query_scalar("SELECT 1::bigint")
        .fetch_one(pool)
        .await?;
    Ok(())
}
