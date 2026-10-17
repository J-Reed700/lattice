pub mod auth;
pub mod file_access_config;
pub mod input_validator;
pub mod json_validator;
pub mod keyring_storage;
pub mod validated_file;

pub use file_access_config::FileAccessConfig;
pub use input_validator::InputValidator;
pub use validated_file::{ValidatedFile, ValidatedFileError, ValidationError};

/// Security context for request validation and authorization
///
/// Provides centralized access to security-related services:
/// - Input validation for user input
#[derive(Debug, Clone)]
pub struct SecurityContext {
    user_id: Option<String>,
    input_validator: InputValidator,
}

impl SecurityContext {
    pub fn new() -> Self {
        Self {
            user_id: None,
            input_validator: InputValidator::new(),
        }
    }

    pub fn with_user(user_id: String) -> Self {
        Self {
            user_id: Some(user_id),
            input_validator: InputValidator::new(),
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
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
