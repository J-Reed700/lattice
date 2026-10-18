use std::sync::Arc;

use axum::http::HeaderValue;
use sqlx::PgPool;

use super::auth::AuthConfig;
use crate::sync::service::SyncService;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub sync_service: Arc<dyn SyncService>,
    pub auth: AuthConfig,
    pub cors_allowed_origins: Vec<HeaderValue>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        sync_service: Arc<dyn SyncService>,
        auth: AuthConfig,
        cors_allowed_origins: Vec<HeaderValue>,
    ) -> Self {
        Self {
            pool,
            sync_service,
            auth,
            cors_allowed_origins,
        }
    }
}
