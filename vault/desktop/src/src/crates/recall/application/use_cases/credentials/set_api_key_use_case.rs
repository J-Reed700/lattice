//! Set API Key Use Case
//!
//! Securely stores API keys for external services using OS keyring.
//!
//! # Dependencies
//! - `CredentialsPort` - Secure credential storage via OS keyring
//!
//! # Security
//! - Stores credentials in OS keyring (Keychain on macOS, Credential Manager on Windows)
//! - Never logs or exposes API keys in plain text
//! - Audit logs credential operations (without key values)
//!
//! # Example
//! ```rust,no_run
//! let use_case = SetApiKeyUseCase::new(credentials_port);
//! use_case.execute("anthropic".into(), "sk-ant-...".into()).await?;
//! ```

use crate::application::dtos::credential_dto::CredentialOperationResultDto;
use crate::application::ports::CredentialsPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct SetApiKeyUseCase {
    credentials: Arc<dyn CredentialsPort>,
}

impl SetApiKeyUseCase {
    pub fn new(credentials: Arc<dyn CredentialsPort>) -> Self {
        Self { credentials }
    }

    pub async fn execute(
        &self,
        service: String,
        key: String,
    ) -> Result<CredentialOperationResultDto, AppError> {
        tracing::debug!(service = %service, "Setting API key");

        self.credentials.set_api_key(&service, &key).await?;

        tracing::info!(service = %service, "API key set successfully");

        Ok(CredentialOperationResultDto {
            success: true,
            message: Some(format!("API key for {} set successfully", service)),
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
        fail_on_set: bool,
    }

    impl MockCredentialsPort {
        fn new() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_set: false,
            }
        }

        fn new_failing() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_set: true,
            }
        }
    }

    #[async_trait]
    impl CredentialsPort for MockCredentialsPort {
        async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
            if self.fail_on_set {
                return Err(AppError::Storage("Failed to store credential".to_string()));
            }
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
    async fn test_set_api_key_success() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port.clone());

        let result = use_case
            .execute("anthropic".to_string(), "sk-test-key".to_string())
            .await;

        assert!(result.is_ok());
        let dto = result.unwrap();
        assert!(dto.success);
        assert!(dto.message.is_some());
        assert!(dto.message.unwrap().contains("anthropic"));

        // Verify the key was actually stored
        let stored = mock_port.get_api_key("anthropic").await.unwrap();
        assert_eq!(stored, Some("sk-test-key".to_string()));
    }

    #[tokio::test]
    async fn test_set_api_key_overwrites_existing() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port.clone());

        // Set first key
        use_case
            .execute("openai".to_string(), "first-key".to_string())
            .await
            .unwrap();

        // Overwrite with second key
        let result = use_case
            .execute("openai".to_string(), "second-key".to_string())
            .await;

        assert!(result.is_ok());

        // Verify only the second key is stored
        let stored = mock_port.get_api_key("openai").await.unwrap();
        assert_eq!(stored, Some("second-key".to_string()));
    }

    #[tokio::test]
    async fn test_set_api_key_empty_service() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port);

        // Empty service name - use case doesn't validate, but documents behavior
        let result = use_case.execute("".to_string(), "key".to_string()).await;

        // Currently passes through; infrastructure layer should validate
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_api_key_empty_key() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port);

        // Empty key - use case doesn't validate, but documents behavior
        let result = use_case
            .execute("service".to_string(), "".to_string())
            .await;

        // Currently passes through; infrastructure layer should validate
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_api_key_port_error_propagates() {
        let mock_port = Arc::new(MockCredentialsPort::new_failing());
        let use_case = SetApiKeyUseCase::new(mock_port);

        let result = use_case
            .execute("service".to_string(), "key".to_string())
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Storage(msg) => {
                assert!(msg.contains("Failed to store credential"));
            }
            _ => panic!("Expected Persistence error"),
        }
    }

    #[tokio::test]
    async fn test_set_api_key_special_characters() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port.clone());

        // Test with special characters in key
        let special_key = "sk-test_key.with/special+chars=123";
        let result = use_case
            .execute("service".to_string(), special_key.to_string())
            .await;

        assert!(result.is_ok());

        let stored = mock_port.get_api_key("service").await.unwrap();
        assert_eq!(stored, Some(special_key.to_string()));
    }

    #[tokio::test]
    async fn test_set_api_key_unicode_service_name() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetApiKeyUseCase::new(mock_port.clone());

        // Test with unicode in service name
        let result = use_case
            .execute("服务名称".to_string(), "key".to_string())
            .await;

        assert!(result.is_ok());

        let stored = mock_port.get_api_key("服务名称").await.unwrap();
        assert_eq!(stored, Some("key".to_string()));
    }
}
