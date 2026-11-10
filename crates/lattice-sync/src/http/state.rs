use std::sync::Arc;

use axum::http::HeaderValue;

use super::auth::AuthConfig;
use crate::persistence::db::Database;
use crate::sync::service::SyncService;

#[derive(Clone)]
pub struct AppState {
    pub pool: Database,
    pub sync_service: Arc<dyn SyncService>,
    pub auth: AuthConfig,
    pub cors_allowed_origins: Vec<HeaderValue>,
}

impl AppState {
    pub fn new(
        pool: Database,
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
