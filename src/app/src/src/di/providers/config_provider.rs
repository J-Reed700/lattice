//! ConfigProvider - Application configuration management
//!
//! This provider manages application settings and credential storage using:
//! - File-based settings persistence
//! - OS keyring for secure credential storage
//! - Builder pattern for flexible configuration

use crate::features::settings::dto::SettingsDto;
use crate::infrastructure::security::keyring_storage::SecureStorage;
use crate::shared::error::{AppError, Result};
use parking_lot::RwLock;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Trait for keyring service abstraction
///
/// Allows mocking in tests while using real OS keyring in production
pub trait KeyringServiceTrait: Send + Sync {
    /// Store an API key securely
    fn store_api_key(&self, key_name: &str, api_key: &str) -> Result<()>;

    /// Retrieve an API key
    fn get_api_key(&self, key_name: &str) -> Result<Option<String>>;

    /// Delete an API key
    fn delete_api_key(&self, key_name: &str) -> Result<()>;

    /// Check if an API key exists
    fn has_api_key(&self, key_name: &str) -> Result<bool>;
}

/// Production keyring service using OS secure storage
pub struct ProductionKeyringService {
    storage: SecureStorage,
}

impl ProductionKeyringService {
    pub fn new() -> Self {
        Self {
            storage: SecureStorage::new(),
        }
    }

    pub fn with_service(service_name: String) -> Self {
        Self {
            storage: SecureStorage::with_service(service_name),
        }
    }
}

impl Default for ProductionKeyringService {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringServiceTrait for ProductionKeyringService {
    fn store_api_key(&self, key_name: &str, api_key: &str) -> Result<()> {
        self.storage.store_api_key(key_name, api_key)
    }

    fn get_api_key(&self, key_name: &str) -> Result<Option<String>> {
        self.storage.get_api_key(key_name)
    }

    fn delete_api_key(&self, key_name: &str) -> Result<()> {
        self.storage.delete_api_key(key_name)
    }

    fn has_api_key(&self, key_name: &str) -> Result<bool> {
        self.storage.has_api_key(key_name)
    }
}

/// Mock keyring service for testing
pub struct MockKeyringService {
    credentials: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl MockKeyringService {
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Pre-populate a credential for testing
    pub fn set_credential(&self, key_name: &str, value: &str) {
        self.credentials
            .write()
            .insert(key_name.to_string(), value.to_string());
    }
}

impl Default for MockKeyringService {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringServiceTrait for MockKeyringService {
    fn store_api_key(&self, key_name: &str, api_key: &str) -> Result<()> {
        if api_key.is_empty() {
            return Err(AppError::InvalidInput("API key cannot be empty".to_string()));
        }

        self.credentials
            .write()
            .insert(key_name.to_string(), api_key.to_string());
        Ok(())
    }

    fn get_api_key(&self, key_name: &str) -> Result<Option<String>> {
        Ok(self.credentials
            .read()
            .get(key_name)
            .cloned())
    }

    fn delete_api_key(&self, key_name: &str) -> Result<()> {
        self.credentials.write().remove(key_name);
        Ok(())
    }

    fn has_api_key(&self, key_name: &str) -> Result<bool> {
        Ok(self.credentials.read().contains_key(key_name))
    }
}

/// ConfigProvider manages application configuration and settings
///
/// Provides:
/// - Settings loading from file or defaults
/// - Settings persistence to disk
/// - Mutable settings updates
/// - Secure credential storage via OS keyring
///
/// # Configuration File Format
///
/// Settings are stored in JSON format:
/// ```json
/// {
///   "indexing": {
///     "chunkSize": 512,
///     "chunkOverlap": 50,
///     ...
///   },
///   "search": {
///     "maxResults": 10,
///     ...
///   },
///   ...
/// }
/// ```
///
/// # Configuration File Location
///
/// Default location: `~/.config/recall/settings.json` (Linux/macOS)
/// or `%APPDATA%\recall\settings.json` (Windows)
///
/// # Example
///
/// ```rust,ignore
/// let provider = ConfigProviderBuilder::new()
///     .with_config_path(PathBuf::from("./config.json"))
///     .build()?;
///
/// let settings = provider.settings();
/// assert_eq!(settings.search.max_results, 10);
/// ```
pub struct ConfigProvider {
    settings: Arc<RwLock<SettingsDto>>,
    config_path: PathBuf,
    keyring_service: Arc<dyn KeyringServiceTrait>,
}

impl ConfigProvider {
    /// Get current settings (immutable reference)
    pub fn settings(&self) -> SettingsDto {
        self.settings.read().clone()
    }

    /// Update settings and persist to disk
    ///
    /// # Arguments
    ///
    /// * `new_settings` - Updated settings to save
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be written to disk
    pub fn update_settings(&self, new_settings: SettingsDto) -> Result<()> {
        // Update in-memory settings
        *self.settings.write() = new_settings.clone();

        // Persist to disk
        self.save_settings(&new_settings)?;

        Ok(())
    }

    /// Get keyring service for credential management
    pub fn keyring_service(&self) -> Arc<dyn KeyringServiceTrait> {
        Arc::clone(&self.keyring_service)
    }

    /// Get configuration file path
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Save settings to disk
    fn save_settings(&self, settings: &SettingsDto) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::InvalidInput(format!("Failed to create config directory: {}", e))
            })?;
        }

        // Serialize to JSON
        let json = serde_json::to_string_pretty(settings).map_err(|e| {
            AppError::InvalidInput(format!("Failed to serialize settings: {}", e))
        })?;

        // Write to file
        fs::write(&self.config_path, json).map_err(|e| {
            AppError::InvalidInput(format!("Failed to write settings file: {}", e))
        })?;

        Ok(())
    }

    /// Load settings from file
    fn load_settings(config_path: &Path) -> Result<SettingsDto> {
        if !config_path.exists() {
            // Return default settings if file doesn't exist
            return Ok(SettingsDto::default());
        }

        // Read file contents
        let contents = fs::read_to_string(config_path).map_err(|e| {
            AppError::InvalidInput(format!("Failed to read settings file: {}", e))
        })?;

        // Parse JSON
        let settings: SettingsDto = serde_json::from_str(&contents).map_err(|e| {
            AppError::InvalidInput(format!("Failed to parse settings JSON: {}", e))
        })?;

        Ok(settings)
    }
}

/// Builder for ConfigProvider
///
/// Supports:
/// - Custom config path or platform default
/// - Real keyring or mock for testing
///
/// # Example
///
/// ```rust,ignore
/// // Production usage with defaults
/// let provider = ConfigProviderBuilder::new().build()?;
///
/// // Custom config path
/// let provider = ConfigProviderBuilder::new()
///     .with_config_path(PathBuf::from("./my_config.json"))
///     .build()?;
///
/// // Testing with mock keyring
/// let provider = ConfigProviderBuilder::new()
///     .with_keyring_enabled(false)
///     .build()?;
/// ```
pub struct ConfigProviderBuilder {
    config_path: Option<PathBuf>,
    keyring_service: Option<Arc<dyn KeyringServiceTrait>>,
}

impl ConfigProviderBuilder {
    /// Create a new builder with defaults
    pub fn new() -> Self {
        Self {
            config_path: None,
            keyring_service: None,
        }
    }

    /// Set custom configuration file path
    ///
    /// If not set, uses platform default:
    /// - Linux/macOS: `~/.config/recall/settings.json`
    /// - Windows: `%APPDATA%\recall\settings.json`
    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Control keyring service usage
    ///
    /// # Arguments
    ///
    /// * `enabled` - If true, use OS keyring; if false, use mock
    ///
    /// Useful for testing without requiring OS keyring access
    pub fn with_keyring_enabled(mut self, enabled: bool) -> Self {
        if enabled {
            self.keyring_service = Some(Arc::new(ProductionKeyringService::new()));
        } else {
            self.keyring_service = Some(Arc::new(MockKeyringService::new()));
        }
        self
    }

    /// Set custom keyring service (advanced usage)
    ///
    /// Allows injecting custom keyring implementations for testing
    pub fn with_keyring_service(mut self, service: Arc<dyn KeyringServiceTrait>) -> Self {
        self.keyring_service = Some(service);
        self
    }

    /// Build the ConfigProvider
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Config file exists but cannot be read
    /// - Config file contains invalid JSON
    pub fn build(self) -> Result<ConfigProvider> {
        // Determine config path
        let config_path = self.config_path.unwrap_or_else(|| {
            Self::default_config_path()
        });

        // Determine keyring service
        let keyring_service = self.keyring_service.unwrap_or_else(|| {
            Arc::new(ProductionKeyringService::new())
        });

        // Load settings
        let settings = ConfigProvider::load_settings(&config_path)?;

        Ok(ConfigProvider {
            settings: Arc::new(RwLock::new(settings)),
            config_path,
            keyring_service,
        })
    }

    /// Get default configuration path for the platform
    fn default_config_path() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var("APPDATA")
                .unwrap_or_else(|_| String::from("."));
            PathBuf::from(appdata).join("recall").join("settings.json")
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME")
                .unwrap_or_else(|_| String::from("."));
            PathBuf::from(home)
                .join(".config")
                .join("recall")
                .join("settings.json")
        }
    }
}

impl Default for ConfigProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_provider_default_settings() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path)
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        let settings = provider.settings();

        // Verify default settings
        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
        assert_eq!(settings.llm.model, "llama3.2:latest");
        assert_eq!(settings.ui.theme, "system");
    }

    #[test]
    fn test_config_provider_keyring_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path)
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        let keyring = provider.keyring_service();

        // Should use mock keyring service
        keyring.store_api_key("test_key", "test_value").unwrap();
        let value = keyring.get_api_key("test_key").unwrap();
        assert_eq!(value, Some("test_value".to_string()));
    }

    #[test]
    fn test_config_provider_update_settings() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path.clone())
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        // Get current settings
        let mut new_settings = provider.settings();

        // Modify settings
        new_settings.search.max_results = 20;
        new_settings.llm.temperature = 0.9;

        // Update and persist
        provider.update_settings(new_settings.clone()).unwrap();

        // Verify in-memory settings updated
        let current = provider.settings();
        assert_eq!(current.search.max_results, 20);
        assert_eq!(current.llm.temperature, 0.9);

        // Verify settings persisted to disk
        assert!(config_path.exists());

        // Load a new provider from the same file
        let provider2 = ConfigProviderBuilder::new()
            .with_config_path(config_path)
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        let loaded = provider2.settings();
        assert_eq!(loaded.search.max_results, 20);
        assert_eq!(loaded.llm.temperature, 0.9);
    }

    #[test]
    fn test_config_provider_load_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        // Create a config file with custom settings
        let custom_settings = SettingsDto {
            search: crate::features::settings::dto::SearchSettingsDto {
                max_results: 50,
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&custom_settings).unwrap();
        fs::write(&config_path, json).unwrap();

        // Load provider
        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path)
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        let settings = provider.settings();
        assert_eq!(settings.search.max_results, 50);
    }

    #[test]
    fn test_mock_keyring_service() {
        let service = MockKeyringService::new();

        // Store credential
        service.store_api_key("test_key", "secret_value").unwrap();

        // Retrieve credential
        let value = service.get_api_key("test_key").unwrap();
        assert_eq!(value, Some("secret_value".to_string()));

        // Check existence
        assert!(service.has_api_key("test_key").unwrap());

        // Delete credential
        service.delete_api_key("test_key").unwrap();

        // Verify deletion
        let deleted = service.get_api_key("test_key").unwrap();
        assert_eq!(deleted, None);
        assert!(!service.has_api_key("test_key").unwrap());
    }

    #[test]
    fn test_mock_keyring_rejects_empty_key() {
        let service = MockKeyringService::new();

        let result = service.store_api_key("test_key", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_path_accessor() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path.clone())
            .with_keyring_enabled(false)
            .build()
            .unwrap();

        assert_eq!(provider.config_path(), config_path.as_path());
    }

    #[test]
    fn test_custom_keyring_service() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("settings.json");

        let mock_service = Arc::new(MockKeyringService::new()) as Arc<dyn KeyringServiceTrait>;
        mock_service.store_api_key("pre_set_key", "pre_set_value").unwrap();

        let provider = ConfigProviderBuilder::new()
            .with_config_path(config_path)
            .with_keyring_service(mock_service)
            .build()
            .unwrap();

        let keyring = provider.keyring_service();
        let value = keyring.get_api_key("pre_set_key").unwrap();
        assert_eq!(value, Some("pre_set_value".to_string()));
    }
}
