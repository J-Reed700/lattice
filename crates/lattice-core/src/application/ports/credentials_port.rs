use crate::shared::error::AppError;
use async_trait::async_trait;

#[async_trait]
pub trait CredentialsPort: Send + Sync {
    async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError>;
    async fn get_api_key(&self, service: &str) -> Result<Option<String>, AppError>;
    async fn delete_api_key(&self, service: &str) -> Result<(), AppError>;
    async fn has_api_key(&self, service: &str) -> Result<bool, AppError>;
    async fn clear_all_credentials(&self) -> Result<(), AppError>;
    async fn set_custom_endpoint(&self, endpoint: &str) -> Result<(), AppError>;
    async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError>;
}
