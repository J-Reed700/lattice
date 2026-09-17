//! Credentials Adapter Implementation
//!
//! Implements CredentialsPort using OS keyring for secure storage.
//!
//! # Security
//! - API keys stored in OS-level keyring (Windows Credential Manager, macOS Keychain, Linux Secret Service)
//! - Custom endpoints stored in config file (not sensitive)
//! - Never stores credentials in plaintext

use crate::application::ports::credentials_port::CredentialsPort;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "com.lattice.lattice";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EndpointConfig {
    custom_endpoint: Option<String>,
}

/// Credentials adapter using OS keyring
pub struct CredentialsAdapter {
    service: String,
    config_path: PathBuf,
}

impl CredentialsAdapter {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
            config_path,
        }
    }

    pub fn with_service(service: String, config_path: PathBuf) -> Self {
        Self {
            service,
            config_path,
        }
    }

    fn load_endpoint_config(&self) -> Result<EndpointConfig> {
        if !self.config_path.exists() {
            return Ok(EndpointConfig {
                custom_endpoint: None,
            });
        }

        let content = fs::read_to_string(&self.config_path)
            .map_err(|e| AppError::FileSystem(format!("Failed to read config: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| AppError::Serialization(format!("Failed to parse config: {}", e)))
    }

    fn save_endpoint_config(&self, config: &EndpointConfig) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::FileSystem(format!("Failed to create config dir: {}", e)))?;
        }

        let content = serde_json::to_string_pretty(config)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize config: {}", e)))?;

        fs::write(&self.config_path, content)
            .map_err(|e| AppError::FileSystem(format!("Failed to write config: {}", e)))
    }
}

#[async_trait]
impl CredentialsPort for CredentialsAdapter {
    async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
        if key.is_empty() {
            return Err(AppError::InvalidInput(
                "API key cannot be empty".to_string(),
            ));
        }

        let entry = Entry::new(&self.service, service)
            .map_err(|e| AppError::Security(format!("Failed to create keyring entry: {}", e)))?;

        entry
            .set_password(key)
            .map_err(|e| AppError::Security(format!("Failed to store API key: {}", e)))?;

        info!("API key '{}' stored securely", service);
        Ok(())
    }

    async fn get_api_key(&self, service: &str) -> Result<Option<String>, AppError> {
        let entry = Entry::new(&self.service, service)
            .map_err(|e| AppError::Security(format!("Failed to create keyring entry: {}", e)))?;

        match entry.get_password() {
            Ok(password) => {
                info!("API key '{}' retrieved successfully", service);
                Ok(Some(password))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(keyring::Error::Ambiguous(_)) => {
                warn!("Multiple entries found for key '{}'", service);
                Err(AppError::Security("Ambiguous keyring entry".to_string()))
            }
            Err(e) => {
                error!("Failed to retrieve API key '{}': {}", service, e);
                Err(AppError::Security(format!(
                    "Failed to retrieve API key: {}",
                    e
                )))
            }
        }
    }

    async fn delete_api_key(&self, service: &str) -> Result<(), AppError> {
        let entry = Entry::new(&self.service, service)
            .map_err(|e| AppError::Security(format!("Failed to create keyring entry: {}", e)))?;

        match entry.delete_credential() {
            Ok(()) => {
                info!("API key '{}' deleted", service);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => {
                error!("Failed to delete API key '{}': {}", service, e);
                Err(AppError::Security(format!(
                    "Failed to delete API key: {}",
                    e
                )))
            }
        }
    }

    async fn has_api_key(&self, service: &str) -> Result<bool, AppError> {
        Ok(self.get_api_key(service).await?.is_some())
    }

    async fn clear_all_credentials(&self) -> Result<(), AppError> {
        // Hardcoded list — keep in sync when a new credential service is added.
        // The OS keyring API doesn't support enumeration, so we can't discover
        // entries dynamically.
        let known_services = vec!["anthropic_api_key", "openai_api_key"];

        for service in known_services {
            let _ = self.delete_api_key(service).await;
        }

        info!("All credentials cleared");
        Ok(())
    }

    async fn set_custom_endpoint(&self, endpoint: &str) -> Result<(), AppError> {
        let config = EndpointConfig {
            custom_endpoint: Some(endpoint.to_string()),
        };
        self.save_endpoint_config(&config)
    }

    async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError> {
        let config = self.load_endpoint_config()?;
        Ok(config.custom_endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    async fn test_store_and_retrieve_api_key() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let adapter = CredentialsAdapter::with_service("com.lattice.test".to_string(), config_path);

        adapter
            .set_api_key("test_service", "test_key_12345")
            .await
            .unwrap();

        let retrieved = adapter.get_api_key("test_service").await.unwrap();
        assert_eq!(retrieved, Some("test_key_12345".to_string()));

        adapter.delete_api_key("test_service").await.unwrap();

        let after_delete = adapter.get_api_key("test_service").await.unwrap();
        assert_eq!(after_delete, None);
    }

    #[tokio::test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    async fn test_has_api_key() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let adapter =
            CredentialsAdapter::with_service("com.lattice.test2".to_string(), config_path);

        assert!(!adapter.has_api_key("test_key").await.unwrap());

        adapter.set_api_key("test_key", "value").await.unwrap();
        assert!(adapter.has_api_key("test_key").await.unwrap());

        adapter.delete_api_key("test_key").await.unwrap();
    }

    #[tokio::test]
    async fn test_empty_key_rejected() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let adapter = CredentialsAdapter::new(config_path);

        let result = adapter.set_api_key("test", "").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_custom_endpoint() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let adapter = CredentialsAdapter::new(config_path);

        let endpoint = adapter.get_custom_endpoint().await.unwrap();
        assert_eq!(endpoint, None);

        adapter
            .set_custom_endpoint("http://localhost:8080")
            .await
            .unwrap();

        let endpoint = adapter.get_custom_endpoint().await.unwrap();
        assert_eq!(endpoint, Some("http://localhost:8080".to_string()));
    }

    #[tokio::test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    async fn test_clear_all_credentials() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let adapter =
            CredentialsAdapter::with_service("com.lattice.test3".to_string(), config_path);

        adapter
            .set_api_key("anthropic_api_key", "key1")
            .await
            .unwrap();
        adapter.set_api_key("openai_api_key", "key2").await.unwrap();

        adapter.clear_all_credentials().await.unwrap();

        assert!(!adapter.has_api_key("anthropic_api_key").await.unwrap());
        assert!(!adapter.has_api_key("openai_api_key").await.unwrap());
    }
}
