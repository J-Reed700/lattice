use crate::shared::error::AppError;
use async_trait::async_trait;

pub struct UpdateInfoData {
    pub available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
}

#[async_trait]
pub trait UpdateCheckerPort: Send + Sync {
    async fn check_for_updates(&self) -> Result<UpdateInfoData, AppError>;
    fn get_current_version(&self) -> String;
}
