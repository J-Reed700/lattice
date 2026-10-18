//! The core module: the database pool, security context, data directory,
//! background scope and credentials.

use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use lattice_core::shared::runtime::background::BackgroundTasks;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;

// Application Use Cases - Credentials
use crate::features::credentials::use_cases::{
    DeleteApiKeyUseCase, GetApiKeyUseCase, SetApiKeyUseCase, SetCustomEndpointUseCase,
};

// Application Ports
use lattice_core::application::ports::CredentialsPort;

/// Core shared infrastructure used by all modules
///
/// **Fields**: ~10
/// - Database connection pool
/// - Security context and file access config
/// - Application data directory
/// - Background scope for spawned work
/// - Credentials port for API keys
#[derive(Clone)]
pub struct CoreModule {
    // Cross-cutting shared infrastructure — not owned by any feature slice.
    db_pool: SqlitePool,
    db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,
    security_context: Arc<SecurityContext>,
    file_access_config: Arc<FileAccessConfig>,
    data_dir: PathBuf,
    background: Arc<BackgroundTasks>,

    // Feature slice
    credentials: crate::features::credentials::di::CredentialsDi,
}

impl CoreModule {
    /// Build shared Layer-1 infrastructure plus the credentials feature slice.
    pub async fn new(
        db_pool: SqlitePool,
        db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,
        data_dir: PathBuf,
        background: Arc<BackgroundTasks>,
    ) -> lattice_core::shared::error::Result<Self> {
        let security_context = Arc::new(SecurityContext::new());

        // Imported documents live outside data_dir. Create their root before
        // canonicalizing the policy, including on the first launch before any
        // import. Otherwise a policy refresh would discard the missing root.
        let library_root =
            crate::infrastructure::storage::ContentAddressedStorage::default_library_root()?;
        tokio::fs::create_dir_all(&library_root)
            .await
            .map_err(|error| {
                lattice_core::shared::error::AppError::FileStorage(format!(
                    "Failed to create library directory: {error}"
                ))
            })?;
        // The user's vault and indexed folders are added by refresh_allowed_roots.
        let file_access_config =
            Arc::new(FileAccessConfig::new(vec![data_dir.clone(), library_root]));

        let credentials_path = data_dir.join("credentials.json");
        let credentials = crate::features::credentials::di::build(credentials_path);

        Ok(Self {
            db_pool,
            db_conn,
            security_context,
            file_access_config,
            data_dir,
            background,
            credentials,
        })
    }

    // Infrastructure getters
    pub fn db_pool(&self) -> &SqlitePool {
        &self.db_pool
    }

    pub fn db_conn(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::database::DatabaseConnection> {
        &self.db_conn
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        &self.security_context
    }

    pub fn file_access_config(&self) -> &Arc<FileAccessConfig> {
        &self.file_access_config
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// The app's background scope: shutdown closes it and joins its tasks
    /// before the database closes.
    pub fn background(&self) -> &Arc<BackgroundTasks> {
        &self.background
    }

    // Credentials feature getters
    pub fn credentials(&self) -> &Arc<dyn CredentialsPort> {
        &self.credentials.credentials
    }

    pub fn set_api_key_use_case(&self) -> &Arc<SetApiKeyUseCase> {
        &self.credentials.set_api_key_use_case
    }

    pub fn get_api_key_use_case(&self) -> &Arc<GetApiKeyUseCase> {
        &self.credentials.get_api_key_use_case
    }

    pub fn delete_api_key_use_case(&self) -> &Arc<DeleteApiKeyUseCase> {
        &self.credentials.delete_api_key_use_case
    }

    pub fn set_custom_endpoint_use_case(&self) -> &Arc<SetCustomEndpointUseCase> {
        &self.credentials.set_custom_endpoint_use_case
    }
}
