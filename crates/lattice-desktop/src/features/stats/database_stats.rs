use async_trait::async_trait;
use lattice_core::application::ports::DatabaseStatsPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Database, DatabaseHandle};

pub struct DatabaseStatsAdapter {
    pool: Database,
}

impl DatabaseStatsAdapter {
    pub fn new(pool: impl DatabaseHandle) -> Self {
        let pool = pool.database();
        Self { pool }
    }
}

#[async_trait]
impl DatabaseStatsPort for DatabaseStatsAdapter {
    async fn get_database_size_bytes(&self) -> Result<i64> {
        super::repository::database_size_bytes(&self.pool).await
    }
}
