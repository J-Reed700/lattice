//! Delete API Key Use Case
//!
//! Removes API keys from secure storage.
//!
//! # Dependencies
//! - `CredentialsPort` - Credential deletion operations
//!
//! # Security
//! - Securely deletes credentials from OS keyring
//! - Audit logs credential deletion
//! - Idempotent (succeeds even if key doesn't exist)
//!
//! # Example
//! ```rust,no_run
//! let use_case = DeleteApiKeyUseCase::new(credentials_port);
//! use_case.execute("anthropic".into()).await?;
//! ```

use crate::application::ports::CredentialsPort;
use crate::features::credentials::dto::CredentialOperationResultDto;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct DeleteApiKeyUseCase {
    credentials: Arc<dyn CredentialsPort>,
}

impl DeleteApiKeyUseCase {
    pub fn new(credentials: Arc<dyn CredentialsPort>) -> Self {
        Self { credentials }
    }

    pub async fn execute(&self, service: String) -> Result<CredentialOperationResultDto, AppError> {
        tracing::debug!(service = %service, "Deleting API key");

        self.credentials.delete_api_key(&service).await?;

        tracing::info!(service = %service, "API key deleted successfully");

        Ok(CredentialOperationResultDto {
            success: true,
            message: Some(format!("API key for {} deleted successfully", service)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    struct MockCredentialsPort {
        api_keys: Arc<Mutex<HashMap<String, String>>>,
        endpoints: Arc<Mutex<HashMap<String, String>>>,
        fail_on_delete: bool,
    }

    impl MockCredentialsPort {
        fn new() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_delete: false,
            }
        }

        fn new_failing() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_delete: true,
            }
        }

        async fn seed_key(&self, service: &str, key: &str) {
            self.api_keys
                .lock()
                .await
                .insert(service.to_string(), key.to_string());
        }

        async fn has_key(&self, service: &str) -> bool {
            self.api_keys.lock().await.contains_key(service)
        }
    }

    #[async_trait]
    impl CredentialsPort for MockCredentialsPort {
        async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
            self.api_keys
                .lock()
                .await
                .insert(service.to_string(), key.to_string());
            Ok(())
        }

        async fn get_api_key(&self, service: &str) -> Result<Option<String>, AppError> {
            Ok(self.api_keys.lock().await.get(service).cloned())
        }

        async fn delete_api_key(&self, service: &str) -> Result<(), AppError> {
            if self.fail_on_delete {
                return Err(AppError::Storage("Failed to delete credential".to_string()));
            }
            self.api_keys.lock().await.remove(service);
            Ok(())
        }

        async fn has_api_key(&self, service: &str) -> Result<bool, AppError> {
            Ok(self.api_keys.lock().await.contains_key(service))
        }

        async fn clear_all_credentials(&self) -> Result<(), AppError> {
            self.api_keys.lock().await.clear();
            self.endpoints.lock().await.clear();
            Ok(())
        }

        async fn set_custom_endpoint(&self, endpoint: &str) -> Result<(), AppError> {
            self.endpoints
                .lock()
                .await
                .insert("custom".to_string(), endpoint.to_string());
            Ok(())
        }

        async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError> {
            Ok(self.endpoints.lock().await.get("custom").cloned())
        }
    }

    #[tokio::test]
    async fn test_delete_api_key_success() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("anthropic", "sk-test-key").await;

        let use_case = DeleteApiKeyUseCase::new(mock_port.clone());

        let result = use_case.execute("anthropic".to_string()).await;

        assert!(result.is_ok());
        let dto = result.unwrap();
        assert!(dto.success);
        assert!(dto.message.is_some());
        assert!(dto.message.unwrap().contains("anthropic"));

        assert!(!mock_port.has_key("anthropic").await);
    }

    #[tokio::test]
    async fn test_delete_api_key_not_found_ok() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = DeleteApiKeyUseCase::new(mock_port);

        let result = use_case.execute("nonexistent".to_string()).await;

        assert!(result.is_ok());
        let dto = result.unwrap();
        assert!(dto.success);
    }

    #[tokio::test]
    async fn test_delete_api_key_empty_service() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = DeleteApiKeyUseCase::new(mock_port);

        // Empty service name - use case doesn't validate, documents behavior
        let result = use_case.execute("".to_string()).await;

        // Currently passes through; infrastructure layer should validate
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_api_key_port_error_propagates() {
        let mock_port = Arc::new(MockCredentialsPort::new_failing());
        let use_case = DeleteApiKeyUseCase::new(mock_port);

        let result = use_case.execute("service".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Storage(msg) => {
                assert!(msg.contains("Failed to delete credential"));
            }
            _ => panic!("Expected Storage error"),
        }
    }

    #[tokio::test]
    async fn test_delete_api_key_multiple_calls_idempotent() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("service", "key").await;

        let use_case = DeleteApiKeyUseCase::new(mock_port.clone());

        // First delete
        let result1 = use_case.execute("service".to_string()).await;
        assert!(result1.is_ok());
        assert!(!mock_port.has_key("service").await);

        // Second delete (idempotent)
        let result2 = use_case.execute("service".to_string()).await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_delete_api_key_leaves_other_keys() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("anthropic", "anthropic-key").await;
        mock_port.seed_key("openai", "openai-key").await;
        mock_port.seed_key("cohere", "cohere-key").await;

        let use_case = DeleteApiKeyUseCase::new(mock_port.clone());

        // Delete only openai
        use_case.execute("openai".to_string()).await.unwrap();

        // Verify only openai was deleted
        assert!(mock_port.has_key("anthropic").await);
        assert!(!mock_port.has_key("openai").await);
        assert!(mock_port.has_key("cohere").await);
    }

    #[tokio::test]
    async fn test_delete_api_key_unicode_service() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("服务", "unicode-key").await;

        let use_case = DeleteApiKeyUseCase::new(mock_port.clone());

        let result = use_case.execute("服务".to_string()).await;

        assert!(result.is_ok());
        assert!(!mock_port.has_key("服务").await);
    }

    #[tokio::test]
    async fn test_delete_api_key_case_sensitive() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("OpenAI", "key1").await;
        mock_port.seed_key("openai", "key2").await;

        let use_case = DeleteApiKeyUseCase::new(mock_port.clone());

        // Delete only lowercase version
        use_case.execute("openai".to_string()).await.unwrap();

        assert!(mock_port.has_key("OpenAI").await);
        assert!(!mock_port.has_key("openai").await);
    }
}
