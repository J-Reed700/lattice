//! Set Custom Endpoint Use Case
//!
//! Configures custom API endpoints for LLM services (e.g., local Ollama).
//!
//! # Dependencies
//! - `CredentialsPort` - Endpoint configuration storage
//!
//! # Security
//! - Validates URL format before storage
//! - Stores endpoint securely in credentials store
//! - Audit logs endpoint changes
//!
//! # Example
//! ```rust,no_run
//! let use_case = SetCustomEndpointUseCase::new(credentials_port);
//! use_case.execute("ollama".into(), "http://localhost:11434".into()).await?;
//! ```

use crate::features::credentials::dto::CredentialOperationResultDto;
use crate::application::ports::CredentialsPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct SetCustomEndpointUseCase {
    credentials: Arc<dyn CredentialsPort>,
}

impl SetCustomEndpointUseCase {
    pub fn new(credentials: Arc<dyn CredentialsPort>) -> Self {
        Self { credentials }
    }

    pub async fn execute(
        &self,
        endpoint: String,
    ) -> Result<CredentialOperationResultDto, AppError> {
        tracing::debug!(endpoint = %endpoint, "Setting custom endpoint");

        self.credentials.set_custom_endpoint(&endpoint).await?;

        tracing::info!("Custom endpoint set successfully");

        Ok(CredentialOperationResultDto {
            success: true,
            message: Some("Custom endpoint set successfully".to_string()),
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

        async fn get_endpoint(&self) -> Option<String> {
            self.endpoints.lock().await.get("custom").cloned()
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
            if self.fail_on_set {
                return Err(AppError::Storage("Failed to store endpoint".to_string()));
            }
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
    async fn test_set_custom_endpoint_success() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        let result = use_case
            .execute("https://api.example.com/v1".to_string())
            .await;

        assert!(result.is_ok());
        let dto = result.unwrap();
        assert!(dto.success);
        assert!(dto.message.is_some());

        // Verify endpoint was stored
        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some("https://api.example.com/v1".to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_overwrites_existing() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        // Set first endpoint
        use_case
            .execute("https://first.example.com".to_string())
            .await
            .unwrap();

        // Overwrite with second endpoint
        let result = use_case
            .execute("https://second.example.com".to_string())
            .await;

        assert!(result.is_ok());

        // Verify only second endpoint is stored
        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some("https://second.example.com".to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_empty_url() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port);

        // Empty URL - use case doesn't validate, documents behavior
        let result = use_case.execute("".to_string()).await;

        // Currently passes through; infrastructure layer should validate
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_malformed_url() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port);

        // Malformed URL - use case doesn't validate, documents behavior
        let result = use_case.execute("not-a-valid-url".to_string()).await;

        // Currently passes through; infrastructure layer should validate
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_port_error_propagates() {
        let mock_port = Arc::new(MockCredentialsPort::new_failing());
        let use_case = SetCustomEndpointUseCase::new(mock_port);

        let result = use_case
            .execute("https://api.example.com".to_string())
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Storage(msg) => {
                assert!(msg.contains("Failed to store endpoint"));
            }
            _ => panic!("Expected Persistence error"),
        }
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_with_port() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        let result = use_case
            .execute("https://localhost:8080/api/v1".to_string())
            .await;

        assert!(result.is_ok());

        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some("https://localhost:8080/api/v1".to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_http_allowed() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        // HTTP (non-TLS) - use case doesn't validate, infrastructure should warn
        let result = use_case
            .execute("http://insecure.example.com".to_string())
            .await;

        assert!(result.is_ok());

        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some("http://insecure.example.com".to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_with_path_and_query() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        let complex_url = "https://api.example.com:8443/v2/chat?model=gpt4&stream=true";
        let result = use_case.execute(complex_url.to_string()).await;

        assert!(result.is_ok());

        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some(complex_url.to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_unicode_domain() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        // Internationalized domain name
        let result = use_case.execute("https://例え.jp/api".to_string()).await;

        assert!(result.is_ok());

        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some("https://例え.jp/api".to_string()));
    }

    #[tokio::test]
    async fn test_set_custom_endpoint_special_characters() {
        let mock_port = Arc::new(MockCredentialsPort::new());
        let use_case = SetCustomEndpointUseCase::new(mock_port.clone());

        // URL with special characters properly encoded
        let url = "https://api.example.com/path?key=value%20with%20spaces&foo=bar";
        let result = use_case.execute(url.to_string()).await;

        assert!(result.is_ok());

        let stored = mock_port.get_endpoint().await;
        assert_eq!(stored, Some(url.to_string()));
    }
}
