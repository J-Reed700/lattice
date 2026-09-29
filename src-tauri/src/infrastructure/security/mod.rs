pub mod auth;
pub mod file_access_config;
pub mod input_validator;
pub mod json_validator;
pub mod keyring_storage;
pub mod migration;
pub mod rate_limiter;
pub mod validated_file;

pub use file_access_config::FileAccessConfig;
pub use input_validator::InputValidator;
pub use migration::{migrate_credentials, CredentialMigration, MigrationReport};
pub use rate_limiter::{RateLimiter, RateLimiters};
pub use validated_file::{ValidatedFile, ValidatedFileError, ValidationError};

use crate::shared::error::Result;

/// Security context for request validation and authorization
///
/// Provides centralized access to security-related services:
/// - Rate limiting for resource-intensive operations
/// - Input validation for user input
#[derive(Debug, Clone)]
pub struct SecurityContext {
    user_id: Option<String>,
    rate_limiters: RateLimiters,
    input_validator: InputValidator,
}

impl SecurityContext {
    pub fn new() -> Self {
        Self {
            user_id: None,
            rate_limiters: RateLimiters::default(),
            input_validator: InputValidator::new(),
        }
    }

    pub fn with_user(user_id: String) -> Self {
        Self {
            user_id: Some(user_id),
            rate_limiters: RateLimiters::default(),
            input_validator: InputValidator::new(),
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    pub fn rate_limiters(&self) -> &RateLimiters {
        &self.rate_limiters
    }

    pub fn input_validator(&self) -> &InputValidator {
        &self.input_validator
    }
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self::new()
    }
}

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io;

const SERVICE_NAME: &str = "lattice-lattice";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub ollama_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub custom_api_endpoint: Option<String>,
}

pub struct SecureStorage;

fn keyring_guarded<T>(
    op: impl FnOnce() -> Result<T, Box<dyn Error>> + std::panic::UnwindSafe,
) -> Result<T, Box<dyn Error>> {
    match std::panic::catch_unwind(op) {
        Ok(result) => result,
        Err(_) => Err(Box::new(io::Error::other("Keyring unavailable"))),
    }
}

type DeleteResult = Result<(), Box<dyn Error>>;

/// `Ok` when every deletion succeeded or found nothing to delete; otherwise an
/// error naming each credential that is still there.
fn all_deleted(results: Vec<(&str, DeleteResult)>) -> DeleteResult {
    let failures: Vec<String> = results
        .into_iter()
        .filter_map(|(name, result)| match result {
            Ok(()) => None,
            Err(e)
                if matches!(
                    e.downcast_ref::<keyring::Error>(),
                    Some(keyring::Error::NoEntry)
                ) =>
            {
                None
            }
            Err(e) => Some(format!("{name}: {e}")),
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Box::new(io::Error::other(format!(
            "could not delete {}",
            failures.join("; ")
        ))))
    }
}

impl SecureStorage {
    pub fn set_ollama_key(key: &str) -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "ollama_api_key")?;
            entry.set_password(key)?;
            Ok(())
        })
    }

    pub fn get_ollama_key() -> Result<Option<String>, Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "ollama_api_key")?;
            match entry.get_password() {
                Ok(key) => Ok(Some(key)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(Box::new(e)),
            }
        })
    }

    pub fn delete_ollama_key() -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "ollama_api_key")?;
            entry.delete_credential()?;
            Ok(())
        })
    }

    pub fn set_openai_key(key: &str) -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "openai_api_key")?;
            entry.set_password(key)?;
            Ok(())
        })
    }

    pub fn get_openai_key() -> Result<Option<String>, Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "openai_api_key")?;
            match entry.get_password() {
                Ok(key) => Ok(Some(key)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(Box::new(e)),
            }
        })
    }

    pub fn delete_openai_key() -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "openai_api_key")?;
            entry.delete_credential()?;
            Ok(())
        })
    }

    pub fn set_custom_endpoint(endpoint: &str) -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "custom_api_endpoint")?;
            entry.set_password(endpoint)?;
            Ok(())
        })
    }

    pub fn get_custom_endpoint() -> Result<Option<String>, Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "custom_api_endpoint")?;
            match entry.get_password() {
                Ok(endpoint) => Ok(Some(endpoint)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(Box::new(e)),
            }
        })
    }

    pub fn delete_custom_endpoint() -> Result<(), Box<dyn Error>> {
        keyring_guarded(|| {
            let entry = Entry::new(SERVICE_NAME, "custom_api_endpoint")?;
            entry.delete_credential()?;
            Ok(())
        })
    }

    /// Delete every stored credential. Each is attempted even when an earlier
    /// one fails; a credential that was never stored is not a failure, but
    /// any other is reported, since "cleared" must mean cleared.
    pub fn clear_all() -> Result<(), Box<dyn Error>> {
        all_deleted(vec![
            ("ollama_api_key", Self::delete_ollama_key()),
            ("openai_api_key", Self::delete_openai_key()),
            ("custom_api_endpoint", Self::delete_custom_endpoint()),
        ])
    }

    pub fn has_credentials() -> bool {
        Self::get_ollama_key().ok().flatten().is_some()
            || Self::get_openai_key().ok().flatten().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_reports_failed_deletions_but_not_missing_ones() {
        let missing: Result<(), Box<dyn Error>> = Err(Box::new(keyring::Error::NoEntry));
        let failed: Result<(), Box<dyn Error>> = Err(Box::new(io::Error::other("locked")));

        let error = all_deleted(vec![("a", Ok(())), ("b", missing), ("c", failed)]).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("c: locked"), "{message}");
        assert!(!message.contains("b:"), "{message}");

        let missing: Result<(), Box<dyn Error>> = Err(Box::new(keyring::Error::NoEntry));
        assert!(all_deleted(vec![("a", Ok(())), ("b", missing)]).is_ok());
    }

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_store_and_retrieve_ollama_key() {
        let test_key = "test-ollama-api-key-123";

        SecureStorage::set_ollama_key(test_key).unwrap();

        let retrieved = SecureStorage::get_ollama_key().unwrap();
        assert_eq!(retrieved, Some(test_key.to_string()));

        SecureStorage::delete_ollama_key().unwrap();

        let after_delete = SecureStorage::get_ollama_key().unwrap();
        assert_eq!(after_delete, None);
    }

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_store_and_retrieve_openai_key() {
        let test_key = "test-openai-api-key-456";

        SecureStorage::set_openai_key(test_key).unwrap();

        let retrieved = SecureStorage::get_openai_key().unwrap();
        assert_eq!(retrieved, Some(test_key.to_string()));

        SecureStorage::delete_openai_key().unwrap();
    }

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_custom_endpoint() {
        let test_endpoint = "https://custom-api.example.com";

        SecureStorage::set_custom_endpoint(test_endpoint).unwrap();

        let retrieved = SecureStorage::get_custom_endpoint().unwrap();
        assert_eq!(retrieved, Some(test_endpoint.to_string()));

        SecureStorage::delete_custom_endpoint().unwrap();
    }

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_has_credentials() {
        let _ = SecureStorage::clear_all();

        assert!(!SecureStorage::has_credentials());

        SecureStorage::set_ollama_key("test").unwrap();
        assert!(SecureStorage::has_credentials());

        SecureStorage::clear_all().unwrap();
        assert!(!SecureStorage::has_credentials());
    }
}
