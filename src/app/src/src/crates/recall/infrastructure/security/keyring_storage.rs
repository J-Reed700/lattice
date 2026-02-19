//! Secure Credential Storage using OS Keyring
//!
//! Stores sensitive credentials (API keys, passwords) in the operating system's secure keyring:
//! - Windows: Windows Credential Manager
//! - macOS: Keychain
//! - Linux: Secret Service API / libsecret
//!
//! This is a CRITICAL SECURITY FIX - never store API keys in plaintext!

use crate::shared::error::{AppError, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// Service name for keyring entries
const SERVICE_NAME: &str = "com.recall.vault";

/// Secure storage for sensitive credentials
///
/// Uses the operating system's native credential storage:
/// - Windows: Credential Manager
/// - macOS: Keychain
/// - Linux: Secret Service (GNOME Keyring, KWallet)
///
/// # Example
/// ```
/// use vault_desktop::security::SecureStorage;
///
/// let storage = SecureStorage::new();
///
/// // Store an API key
/// storage.store_api_key("anthropic_api_key", "sk-ant-...")?;
///
/// // Retrieve it later
/// if let Some(key) = storage.get_api_key("anthropic_api_key")? {
///     println!("API key: {}", key);
/// }
///
/// // Delete when no longer needed
/// storage.delete_api_key("anthropic_api_key")?;
/// ```
pub struct SecureStorage {
    service: String,
}

fn keyring_guarded<T>(op: impl FnOnce() -> Result<T> + std::panic::UnwindSafe) -> Result<T> {
    match std::panic::catch_unwind(op) {
        Ok(result) => result,
        Err(_) => Err(AppError::KeyringError("Keyring unavailable".to_string())),
    }
}

impl SecureStorage {
    /// Create a new secure storage instance
    ///
    /// Uses the default service name: "com.recall.vault"
    pub fn new() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
        }
    }

    /// Create with a custom service name
    ///
    /// Useful for testing or isolating credentials
    pub fn with_service(service: String) -> Self {
        Self { service }
    }

    /// Store an API key securely in the OS keyring
    ///
    /// # Arguments
    /// * `key_name` - Identifier for the key (e.g., "anthropic_api_key", "openai_api_key")
    /// * `api_key` - The API key value to store
    ///
    /// # Errors
    /// Returns error if:
    /// - Keyring is not available (rare - usually indicates OS issue)
    /// - Permission denied
    /// - Storage is full (very rare)
    ///
    /// # Example
    /// ```
    /// storage.store_api_key("anthropic_api_key", "sk-ant-api-...")?;
    /// storage.store_api_key("openai_api_key", "sk-...")?;
    /// ```
    pub fn store_api_key(&self, key_name: &str, api_key: &str) -> Result<()> {
        keyring_guarded(|| {
            if api_key.is_empty() {
                return Err(AppError::InvalidInput(
                    "API key cannot be empty".to_string(),
                ));
            }

            let entry = Entry::new(&self.service, key_name).map_err(|e| {
                AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
            })?;

            entry.set_password(api_key).map_err(|e| {
                AppError::KeyringError(format!("Failed to store API key '{}': {}", key_name, e))
            })?;

            info!("API key '{}' stored securely", key_name);
            Ok(())
        })
    }

    /// Retrieve an API key from the OS keyring
    ///
    /// # Arguments
    /// * `key_name` - Identifier for the key
    ///
    /// # Returns
    /// - `Ok(Some(key))` - Key found and retrieved
    /// - `Ok(None)` - Key not found (not an error)
    /// - `Err(_)` - System error accessing keyring
    ///
    /// # Example
    /// ```
    /// match storage.get_api_key("anthropic_api_key")? {
    ///     Some(key) => println!("Found key: {}", &key[..10]),
    ///     None => println!("Key not found"),
    /// }
    /// ```
    pub fn get_api_key(&self, key_name: &str) -> Result<Option<String>> {
        keyring_guarded(|| {
            let entry = Entry::new(&self.service, key_name).map_err(|e| {
                AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
            })?;

            match entry.get_password() {
                Ok(password) => {
                    info!("API key '{}' retrieved successfully", key_name);
                    Ok(Some(password))
                }
                Err(keyring::Error::NoEntry) => {
                    // Not an error - key just doesn't exist
                    Ok(None)
                }
                Err(keyring::Error::Ambiguous(_)) => {
                    warn!("Multiple entries found for key '{}'", key_name);
                    Err(AppError::Other("Ambiguous keyring entry".to_string()))
                }
                Err(e) => {
                    error!("Failed to retrieve API key '{}': {}", key_name, e);
                    Err(e.into())
                }
            }
        })
    }

    /// Delete an API key from the OS keyring
    ///
    /// # Arguments
    /// * `key_name` - Identifier for the key to delete
    ///
    /// # Returns
    /// - `Ok(())` - Key deleted (or didn't exist)
    /// - `Err(_)` - System error
    ///
    /// # Example
    /// ```
    /// // Clean up when API key is no longer needed
    /// storage.delete_api_key("old_api_key")?;
    /// ```
    pub fn delete_api_key(&self, key_name: &str) -> Result<()> {
        keyring_guarded(|| {
            let entry = Entry::new(&self.service, key_name).map_err(|e| {
                AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
            })?;

            match entry.delete_credential() {
                Ok(()) => {
                    info!("API key '{}' deleted", key_name);
                    Ok(())
                }
                Err(keyring::Error::NoEntry) => {
                    // Not an error - key already doesn't exist
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to delete API key '{}': {}", key_name, e);
                    Err(e.into())
                }
            }
        })
    }

    /// Check if an API key exists
    ///
    /// Cheaper than get_api_key() if you only need to check existence
    pub fn has_api_key(&self, key_name: &str) -> Result<bool> {
        Ok(self.get_api_key(key_name)?.is_some())
    }

    /// List all stored credential names
    ///
    /// Note: This is platform-dependent and may not work on all systems.
    /// Returns empty vec on unsupported platforms.
    pub fn list_credential_names(&self) -> Vec<String> {
        // keyring crate doesn't provide a list API
        // This would require platform-specific code
        // For now, return known credential types
        vec![
            "anthropic_api_key".to_string(),
            "openai_api_key".to_string(),
            "database_password".to_string(),
        ]
    }
}

impl Default for SecureStorage {
    fn default() -> Self {
        Self::new()
    }
}

// Tauri commands for frontend integration

/// Tauri command: Store a credential securely
///
/// # Example (TypeScript)
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// await invoke('store_credential', {
///   keyName: 'anthropic_api_key',
///   value: 'sk-ant-...'
/// });
/// ```
#[tauri::command]
pub async fn store_credential(key_name: String, value: String) -> Result<(), String> {
    let storage = SecureStorage::new();
    storage
        .store_api_key(&key_name, &value)
        .map_err(|e| e.to_string())
}

/// Tauri command: Retrieve a credential securely
///
/// # Example (TypeScript)
/// ```typescript
/// const apiKey = await invoke<string | null>('get_credential', {
///   keyName: 'anthropic_api_key'
/// });
///
/// if (apiKey) {
///   console.log('API key found');
/// } else {
///   console.log('API key not configured');
/// }
/// ```
#[tauri::command]
pub async fn get_credential(key_name: String) -> Result<Option<String>, String> {
    let storage = SecureStorage::new();
    storage.get_api_key(&key_name).map_err(|e| e.to_string())
}

/// Tauri command: Delete a credential
///
/// # Example (TypeScript)
/// ```typescript
/// await invoke('delete_credential', {
///   keyName: 'anthropic_api_key'
/// });
/// ```
#[tauri::command]
pub async fn delete_credential(key_name: String) -> Result<(), String> {
    let storage = SecureStorage::new();
    storage.delete_api_key(&key_name).map_err(|e| e.to_string())
}

/// Tauri command: Check if credential exists
///
/// # Example (TypeScript)
/// ```typescript
/// const exists = await invoke<boolean>('has_credential', {
///   keyName: 'anthropic_api_key'
/// });
/// ```
#[tauri::command]
pub async fn has_credential(key_name: String) -> Result<bool, String> {
    let storage = SecureStorage::new();
    storage.has_api_key(&key_name).map_err(|e| e.to_string())
}

/// Credential information for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialInfo {
    pub key_name: String,
    pub exists: bool,
    pub last_updated: Option<String>,
}

/// Tauri command: Get status of all known credentials
///
/// Useful for settings UI to show which credentials are configured
///
/// # Example (TypeScript)
/// ```typescript
/// const credentials = await invoke<CredentialInfo[]>('get_credential_status');
///
/// credentials.forEach(cred => {
///   console.log(`${cred.key_name}: ${cred.exists ? '✓ Configured' : '✗ Not set'}`);
/// });
/// ```
#[tauri::command]
pub async fn get_credential_status() -> Result<Vec<CredentialInfo>, String> {
    let storage = SecureStorage::new();
    let known_keys = vec!["anthropic_api_key", "openai_api_key"];

    let mut infos = Vec::new();
    for key in known_keys {
        let exists = storage.has_api_key(key).unwrap_or(false);
        infos.push(CredentialInfo {
            key_name: key.to_string(),
            exists,
            last_updated: None, // Could be enhanced with timestamp tracking
        });
    }

    Ok(infos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires OS keyring access - may not be available in CI
    fn test_store_and_retrieve() {
        let storage = SecureStorage::with_service("com.recall.test".to_string());

        // Store a test key
        storage
            .store_api_key("test_key", "test_value_12345")
            .unwrap();

        // Retrieve it
        let retrieved = storage.get_api_key("test_key").unwrap();
        assert_eq!(retrieved, Some("test_value_12345".to_string()));

        // Clean up
        storage.delete_api_key("test_key").unwrap();

        // Verify deletion
        let after_delete = storage.get_api_key("test_key").unwrap();
        assert_eq!(after_delete, None);
    }

    #[test]
    #[ignore] // Requires OS keyring access - may not be available in CI
    fn test_has_api_key() {
        let storage = SecureStorage::with_service("com.recall.test".to_string());

        // Should not exist initially
        assert!(!storage.has_api_key("test_key2").unwrap());

        // Store it
        storage.store_api_key("test_key2", "value").unwrap();

        // Should exist now
        assert!(storage.has_api_key("test_key2").unwrap());

        // Clean up
        storage.delete_api_key("test_key2").unwrap();
    }

    #[test]
    fn test_empty_key_rejected() {
        let storage = SecureStorage::new();

        let result = storage.store_api_key("test", "");
        assert!(result.is_err());
    }
}
