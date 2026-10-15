use crate::shared::error::AppError;
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Clone)]
pub struct BackupInfoData {
    pub path: String,
    pub name: String,
    pub created_at: String,
    pub version: String,
    pub file_count: usize,
    pub size: u64,
}

#[async_trait]
pub trait BackupPort: Send + Sync {
    async fn create_backup(&self, path: Option<PathBuf>) -> Result<String, AppError>;
    async fn restore_backup(&self, path: PathBuf) -> Result<(), AppError>;
    async fn list_backups(&self, data_dir: PathBuf) -> Result<Vec<BackupInfoData>, AppError>;
}
