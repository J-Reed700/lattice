//! Secure Credential Storage using OS Keyring
//!
//! Stores sensitive credentials (API keys, passwords) in the operating system's secure keyring:
//! - Windows: Windows Credential Manager
//! - macOS: Keychain
//! - Linux: Secret Service API / libsecret
//!
//! This is a CRITICAL SECURITY FIX - never store API keys in plaintext!

use keyring::Entry;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use tracing::{error, info, warn};
use zeroize::Zeroizing;

/// Service name for keyring entries
const SERVICE_NAME: &str = "com.lattice.lattice";

fn should_isolate_for(
    use_os_keyring_env: bool,
    cfg_test: bool,
    test_runner_env: bool,
    exe_path: Option<&Path>,
) -> bool {
    if use_os_keyring_env {
        return false;
    }
    if cfg_test || test_runner_env {
        return true;
    }
    exe_path
        .and_then(|exe| exe.parent())
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps")
}

/// Returns `true` when running inside a Cargo test binary so tests never
/// touch or prompt the user's real OS keyring unless `LATTICE_USE_OS_KEYRING`
/// is explicitly set.
pub fn should_isolate_os_keyring() -> bool {
    let exe_path = std::env::current_exe().ok();
    should_isolate_for(
        std::env::var_os("LATTICE_USE_OS_KEYRING").is_some(),
        cfg!(test),
        std::env::var_os("RUST_TEST_THREADS").is_some() || std::env::var_os("NEXTEST").is_some(),
        exe_path.as_deref(),
    )
}

enum Backend {
    InMemory(Mutex<HashMap<String, Zeroizing<String>>>),
    OsKeyring {
        service: String,
        cache: Mutex<HashMap<String, Option<Zeroizing<String>>>>,
    },
}

/// Secure storage for sensitive credentials
///
/// Uses the operating system's native credential storage in app binaries
/// (with a zeroizing per-session cache) and an isolated in-memory store in tests:
/// - Windows: Credential Manager
/// - macOS: Keychain
/// - Linux: Secret Service (GNOME Keyring, KWallet)
pub struct SecureStorage {
    backend: Backend,
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn keyring_guarded<T>(op: impl FnOnce() -> Result<T> + std::panic::UnwindSafe) -> Result<T> {
    match std::panic::catch_unwind(op) {
        Ok(result) => result,
        Err(_) => Err(AppError::KeyringError("Keyring unavailable".to_string())),
    }
}

fn get_cached_with(
    cache: &Mutex<HashMap<String, Option<Zeroizing<String>>>>,
    key_name: &str,
    loader: impl FnOnce(&str) -> Result<Option<String>>,
) -> Result<Option<String>> {
    {
        let guard = lock_unpoisoned(cache);
        if let Some(cached) = guard.get(key_name) {
            return Ok(cached.as_ref().map(|secret| (**secret).clone()));
        }
    }

    let loaded = loader(key_name)?;
    let mut guard = lock_unpoisoned(cache);
    guard.insert(
        key_name.to_string(),
        loaded.as_ref().map(|secret| Zeroizing::new(secret.clone())),
    );
    Ok(loaded)
}

fn store_cached_with(
    cache: &Mutex<HashMap<String, Option<Zeroizing<String>>>>,
    key_name: &str,
    api_key: &str,
    writer: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<()> {
    if api_key.is_empty() {
        return Err(AppError::InvalidInput(
            "API key cannot be empty".to_string(),
        ));
    }

    writer(key_name, api_key)?;
    let mut guard = lock_unpoisoned(cache);
    guard.insert(
        key_name.to_string(),
        Some(Zeroizing::new(api_key.to_string())),
    );
    Ok(())
}

fn delete_cached_with(
    cache: &Mutex<HashMap<String, Option<Zeroizing<String>>>>,
    key_name: &str,
    deleter: impl FnOnce(&str) -> Result<()>,
) -> Result<()> {
    deleter(key_name)?;
    let mut guard = lock_unpoisoned(cache);
    guard.insert(key_name.to_string(), None);
    Ok(())
}

impl SecureStorage {
    /// Create a new secure storage instance.
    ///
    /// Uses an isolated in-memory store in test binaries (`should_isolate_os_keyring()`),
    /// or the OS keyring with default service `"com.lattice.lattice"` in app binaries.
    pub fn new() -> Self {
        if should_isolate_os_keyring() {
            Self::in_memory()
        } else {
            Self::os_keyring()
        }
    }

    /// Create an isolated in-memory credential store (for tests).
    pub fn in_memory() -> Self {
        Self {
            backend: Backend::InMemory(Mutex::new(HashMap::new())),
        }
    }

    /// Create a storage instance backed by the real OS keyring (`"com.lattice.lattice"`).
    pub fn os_keyring() -> Self {
        Self::with_service(SERVICE_NAME.to_string())
    }

    /// Create with a custom service name backed by the OS keyring.
    ///
    /// Useful for testing or isolating credentials
    pub fn with_service(service: String) -> Self {
        Self {
            backend: Backend::OsKeyring {
                service,
                cache: Mutex::new(HashMap::new()),
            },
        }
    }

    /// Store an API key securely in the active backend.
    ///
    /// # Errors
    /// Returns error if:
    /// - `api_key` is empty
    /// - Keyring is not available (rare - usually indicates OS issue)
    /// - Permission denied
    /// - Storage is full (very rare)
    pub fn store_api_key(&self, key_name: &str, api_key: &str) -> Result<()> {
        if api_key.is_empty() {
            return Err(AppError::InvalidInput(
                "API key cannot be empty".to_string(),
            ));
        }

        match &self.backend {
            Backend::InMemory(store) => {
                lock_unpoisoned(store)
                    .insert(key_name.to_string(), Zeroizing::new(api_key.to_string()));
                info!("API key '{}' stored securely", key_name);
                Ok(())
            }
            Backend::OsKeyring { service, cache } => {
                store_cached_with(cache, key_name, api_key, |k, v| {
                    keyring_guarded(|| {
                        let entry = Entry::new(service, k).map_err(|e| {
                            AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
                        })?;

                        entry.set_password(v).map_err(|e| {
                            AppError::KeyringError(format!(
                                "Failed to store API key '{}': {}",
                                k, e
                            ))
                        })?;

                        info!("API key '{}' stored securely", k);
                        Ok(())
                    })
                })
            }
        }
    }

    /// Retrieve an API key from the active backend (cached per session on OS keyring).
    pub fn get_api_key(&self, key_name: &str) -> Result<Option<String>> {
        match &self.backend {
            Backend::InMemory(store) => Ok(lock_unpoisoned(store)
                .get(key_name)
                .map(|secret| (**secret).clone())),
            Backend::OsKeyring { service, cache } => get_cached_with(cache, key_name, |k| {
                keyring_guarded(|| {
                    let entry = Entry::new(service, k).map_err(|e| {
                        AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
                    })?;

                    match entry.get_password() {
                        Ok(password) => {
                            info!("API key '{}' retrieved successfully", k);
                            Ok(Some(password))
                        }
                        Err(keyring::Error::NoEntry) => {
                            // Not an error - key just doesn't exist
                            Ok(None)
                        }
                        Err(keyring::Error::Ambiguous(_)) => {
                            warn!("Multiple entries found for key '{}'", k);
                            Err(AppError::Other("Ambiguous keyring entry".to_string()))
                        }
                        Err(e) => {
                            error!("Failed to retrieve API key '{}': {}", k, e);
                            Err(AppError::KeyringError(e.to_string()))
                        }
                    }
                })
            }),
        }
    }

    /// Delete an API key from the active backend.
    pub fn delete_api_key(&self, key_name: &str) -> Result<()> {
        match &self.backend {
            Backend::InMemory(store) => {
                lock_unpoisoned(store).remove(key_name);
                info!("API key '{}' deleted", key_name);
                Ok(())
            }
            Backend::OsKeyring { service, cache } => delete_cached_with(cache, key_name, |k| {
                keyring_guarded(|| {
                    let entry = Entry::new(service, k).map_err(|e| {
                        AppError::KeyringError(format!("Failed to create keyring entry: {}", e))
                    })?;

                    match entry.delete_credential() {
                        Ok(()) => {
                            info!("API key '{}' deleted", k);
                            Ok(())
                        }
                        Err(keyring::Error::NoEntry) => {
                            // Not an error - key already doesn't exist
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to delete API key '{}': {}", k, e);
                            Err(AppError::KeyringError(e.to_string()))
                        }
                    }
                })
            }),
        }
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
            "google_api_key".to_string(),
            "database_password".to_string(),
        ]
    }
}

impl Default for SecureStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_store_and_retrieve() {
        let storage = SecureStorage::with_service("com.lattice.test".to_string());

        storage
            .store_api_key("test_key", "test_value_12345")
            .unwrap();

        // Retrieve it
        let retrieved = storage.get_api_key("test_key").unwrap();
        assert_eq!(retrieved, Some("test_value_12345".to_string()));

        storage.delete_api_key("test_key").unwrap();

        let after_delete = storage.get_api_key("test_key").unwrap();
        assert_eq!(after_delete, None);
    }

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_has_api_key() {
        let storage = SecureStorage::with_service("com.lattice.test".to_string());

        assert!(!storage.has_api_key("test_key2").unwrap());

        storage.store_api_key("test_key2", "value").unwrap();

        assert!(storage.has_api_key("test_key2").unwrap());

        storage.delete_api_key("test_key2").unwrap();
    }

    #[test]
    fn test_empty_key_rejected() {
        let storage = SecureStorage::new();

        let result = storage.store_api_key("test", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_should_isolate_os_keyring_in_cargo_test() {
        assert!(should_isolate_os_keyring());
        assert!(should_isolate_for(
            false,
            false,
            false,
            Some(Path::new("/repo/target/debug/deps/lattice-1234"))
        ));
        assert!(!should_isolate_for(
            false,
            false,
            false,
            Some(Path::new(
                "/Applications/Lattice.app/Contents/MacOS/lattice"
            ))
        ));
        assert!(should_isolate_for(false, false, true, None));
        assert!(!should_isolate_for(
            true,
            true,
            true,
            Some(Path::new("/repo/target/debug/deps/lattice-1234"))
        ));
    }

    #[test]
    fn test_in_memory_isolation_crud_without_os_keyring() {
        let storage = SecureStorage::new();
        assert!(matches!(storage.backend, Backend::InMemory(_)));

        assert_eq!(storage.get_api_key("openai").unwrap(), None);
        assert!(!storage.has_api_key("openai").unwrap());

        storage.store_api_key("openai", "sk-test-123").unwrap();
        assert!(storage.has_api_key("openai").unwrap());
        assert_eq!(
            storage.get_api_key("openai").unwrap(),
            Some("sk-test-123".to_string())
        );

        assert!(storage.store_api_key("openai", "").is_err());
        assert_eq!(
            storage.get_api_key("openai").unwrap(),
            Some("sk-test-123".to_string())
        );

        storage.delete_api_key("openai").unwrap();
        assert!(!storage.has_api_key("openai").unwrap());
        assert_eq!(storage.get_api_key("openai").unwrap(), None);
    }

    #[test]
    fn test_os_keyring_session_cache_behavior() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Mutex::new(HashMap::new());
        let loads = AtomicUsize::new(0);

        // Cache miss returning None caches absence and avoids a second loader call.
        let first = get_cached_with(&cache, "missing", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .unwrap();
        assert_eq!(first, None);
        let second = get_cached_with(&cache, "missing", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Some("unexpected".to_string()))
        })
        .unwrap();
        assert_eq!(second, None);
        assert_eq!(loads.load(Ordering::SeqCst), 1);

        // Transient error is NOT cached, so a subsequent retry invokes the loader again.
        let err = get_cached_with(&cache, "transient", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Err(AppError::KeyringError("locked".to_string()))
        });
        assert!(err.is_err());
        let recovered = get_cached_with(&cache, "transient", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Some("recovered-secret".to_string()))
        })
        .unwrap();
        assert_eq!(recovered, Some("recovered-secret".to_string()));
        assert_eq!(loads.load(Ordering::SeqCst), 3);

        // Cached Some is served without invoking the loader again.
        let cached_hit = get_cached_with(&cache, "transient", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .unwrap();
        assert_eq!(cached_hit, Some("recovered-secret".to_string()));
        assert_eq!(loads.load(Ordering::SeqCst), 3);

        // store_cached_with updates the cache immediately (and rejects empty keys).
        assert!(store_cached_with(&cache, "missing", "", |_, _| Ok(())).is_err());
        store_cached_with(&cache, "missing", "new-secret", |_, _| Ok(())).unwrap();
        let after_store = get_cached_with(&cache, "missing", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .unwrap();
        assert_eq!(after_store, Some("new-secret".to_string()));
        assert_eq!(loads.load(Ordering::SeqCst), 3);

        // delete_cached_with updates the cache to None immediately.
        delete_cached_with(&cache, "missing", |_| Ok(())).unwrap();
        let after_delete = get_cached_with(&cache, "missing", |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Some("stale".to_string()))
        })
        .unwrap();
        assert_eq!(after_delete, None);
        assert_eq!(loads.load(Ordering::SeqCst), 3);
    }
}
