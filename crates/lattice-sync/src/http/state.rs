use std::sync::Arc;

use axum::http::HeaderValue;

use super::auth::AuthConfig;
use crate::persistence::db::Database;
use crate::sync::service::SyncService;

#[derive(Clone)]
pub struct AppState {
    pub database: Database,
    pub sync_service: Arc<dyn SyncService>,
    pub auth: AuthConfig,
    pub cors_allowed_origins: Vec<HeaderValue>,
}

impl AppState {
    pub fn new(
        database: Database,
        sync_service: Arc<dyn SyncService>,
        auth: AuthConfig,
        cors_allowed_origins: Vec<HeaderValue>,
    ) -> Self {
        Self {
            database,
            sync_service,
            auth,
            cors_allowed_origins,
        }
    }
}
