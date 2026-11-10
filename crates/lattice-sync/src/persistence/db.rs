use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Clone, Debug)]
pub struct Database(PgPool);

impl Database {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .connect(database_url)
            .await
            .map(Self)
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.0).await
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        let _: i64 = sqlx::query_scalar("SELECT 1::bigint")
            .fetch_one(&self.0)
            .await?;
        Ok(())
    }

    pub(crate) fn sqlx_pool(&self) -> &PgPool {
        &self.0
    }
}

impl From<PgPool> for Database {
    fn from(pool: PgPool) -> Self {
        Self(pool)
    }
}
