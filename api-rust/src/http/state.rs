use std::sync::Arc;

use sqlx::PgPool;

use crate::sync::service::SyncService;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub sync_service: Arc<dyn SyncService>,
}

impl AppState {
    pub fn new(pool: PgPool, sync_service: Arc<dyn SyncService>) -> Self {
        Self { pool, sync_service }
    }
}
