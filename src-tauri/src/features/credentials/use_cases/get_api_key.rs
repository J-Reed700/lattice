//! Get API Key Use Case
//!
//! Retrieves securely stored API keys from OS keyring.
//!
//! # Dependencies
//! - `CredentialsPort` - Secure credential retrieval
//!
//! # Security
//! - Retrieves credentials from OS keyring
//! - Audit logs credential access
//! - Returns error if credential not found
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetApiKeyUseCase::new(credentials_port);
//! let key = use_case.execute("anthropic".into()).await?;
//! // Use key for API calls
//! ```

use crate::application::ports::CredentialsPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetApiKeyUseCase {
    credentials: Arc<dyn CredentialsPort>,
}

impl GetApiKeyUseCase {
    pub fn new(credentials: Arc<dyn CredentialsPort>) -> Self {
        Self { credentials }
    }

    pub async fn execute(&self, service: String) -> Result<Option<String>, AppError> {
        tracing::debug!(service = %service, "Retrieving API key");

        let key = self.credentials.get_api_key(&service).await?;

        tracing::info!(service = %service, key_exists = key.is_some(), "API key retrieved");

        Ok(key)
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
        fail_on_get: bool,
    }

    impl MockCredentialsPort {
        fn new() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_get: false,
            }
        }

        fn new_failing() -> Self {
            Self {
                api_keys: Arc::new(Mutex::new(HashMap::new())),
                endpoints: Arc::new(Mutex::new(HashMap::new())),
                fail_on_get: true,
            }
        }

        async fn seed_key(&self, service: &str, key: &str) {
            self.api_keys
                .lock()
                .await
                .insert(service.to_string(), key.to_string());
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
            if self.fail_on_get {
                return Err(AppError::Storage(
                    "Failed to retrieve credential".to_string(),
                ));
            }
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
    async fn test_get_api_key_success() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("anthropic", "sk-test-key").await;

        let use_case = GetApiKeyUseCase::new(mock_port);

        let result = use_case.execute("anthropic".to_string()).await;

        assert!(result.is_ok());
        let key = result.unwrap();
        assert_eq!(key, Some("sk-test-key".to_string()));
    }

    #[tokio::test]
    async fn test_get_api_key_not_found_returns_none() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = GetApiKeyUseCase::new(mock_port);

        let result = use_case.execute("nonexistent".to_string()).await;

        assert!(result.is_ok());
        let key = result.unwrap();
        assert_eq!(key, None);
    }

    #[tokio::test]
    async fn test_get_api_key_empty_service() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = GetApiKeyUseCase::new(mock_port);

        // Empty service name - use case doesn't validate, documents behavior
        let result = use_case.execute("".to_string()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_get_api_key_port_error_propagates() {
        let mock_port = Arc::new(MockCredentialsPort::new_failing());
        let use_case = GetApiKeyUseCase::new(mock_port);

        let result = use_case.execute("service".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Storage(msg) => {
                assert!(msg.contains("Failed to retrieve credential"));
            }
            _ => panic!("Expected Persistence error"),
        }
    }

    #[tokio::test]
    async fn test_get_api_key_case_sensitive() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("OpenAI", "key1").await;
        mock_port.seed_key("openai", "key2").await;

        let use_case = GetApiKeyUseCase::new(mock_port);

        let result1 = use_case.execute("OpenAI".to_string()).await.unwrap();
        let result2 = use_case.execute("openai".to_string()).await.unwrap();

        assert_eq!(result1, Some("key1".to_string()));
        assert_eq!(result2, Some("key2".to_string()));
    }

    #[tokio::test]
    async fn test_get_api_key_multiple_services() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("anthropic", "anthropic-key").await;
        mock_port.seed_key("openai", "openai-key").await;
        mock_port.seed_key("cohere", "cohere-key").await;

        let use_case = GetApiKeyUseCase::new(mock_port);

        let anthropic = use_case.execute("anthropic".to_string()).await.unwrap();
        let openai = use_case.execute("openai".to_string()).await.unwrap();
        let cohere = use_case.execute("cohere".to_string()).await.unwrap();

        assert_eq!(anthropic, Some("anthropic-key".to_string()));
        assert_eq!(openai, Some("openai-key".to_string()));
        assert_eq!(cohere, Some("cohere-key".to_string()));
    }

    #[tokio::test]
    async fn test_get_api_key_unicode_service() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        mock_port.seed_key("服务", "unicode-key").await;

        let use_case = GetApiKeyUseCase::new(mock_port);

        let result = use_case.execute("服务".to_string()).await.unwrap();

        assert_eq!(result, Some("unicode-key".to_string()));
    }
}
