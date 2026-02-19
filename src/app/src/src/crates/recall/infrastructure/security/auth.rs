use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};

/// SECURITY: Basic authentication for single-user desktop app
/// While this is a single-user app, we implement basic security measures
pub struct AuthManager {
    app_token: Option<String>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self { app_token: None }
    }

    /// Generate a session token for the app
    pub fn generate_session_token() -> String {
        use uuid::Uuid;
        let token = format!("vault_{}", Uuid::new_v4());

        // Hash the token for storage
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();

        hex::encode(result)
    }

    /// Validate operations based on context
    pub fn validate_operation(&self, operation: &str) -> Result<()> {
        // Define sensitive operations
        const SENSITIVE_OPS: &[&str] = &[
            "delete_all_documents",
            "reset_database",
            "export_all_data",
            "modify_system_config",
        ];

        if SENSITIVE_OPS.contains(&operation) {
            // For sensitive operations, could require additional confirmation
            tracing::warn!("Sensitive operation requested: {}", operation);
        }

        Ok(())
    }

    /// Check if file access is allowed
    pub fn check_file_access(&self, file_path: &str) -> Result<()> {
        // Prevent access to system files
        let restricted_paths = [
            "/etc/passwd",
            "/etc/shadow",
            r"C:\Windows\System32",
            r"C:\Windows\system.ini",
            ".ssh",
            ".gnupg",
        ];

        for restricted in &restricted_paths {
            if file_path.contains(restricted) {
                return Err(AppError::PermissionDenied(
                    "Access to system files is restricted".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Permission levels for operations
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Read,
    Write,
    Delete,
    Admin,
}

impl Permission {
    pub fn check(&self, required: &Permission) -> bool {
        match (self, required) {
            (Permission::Admin, _) => true,
            (Permission::Delete, Permission::Delete) => true,
            (Permission::Delete, Permission::Write) => true,
            (Permission::Delete, Permission::Read) => true,
            (Permission::Write, Permission::Write) => true,
            (Permission::Write, Permission::Read) => true,
            (Permission::Read, Permission::Read) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_token() {
        let token1 = AuthManager::generate_session_token();
        let token2 = AuthManager::generate_session_token();

        // Tokens should be unique
        assert_ne!(token1, token2);

        // Tokens should be properly formatted
        assert_eq!(token1.len(), 64); // SHA256 hex length
    }

    #[test]
    fn test_file_access() {
        let auth = AuthManager::new();

        // Should allow normal files
        assert!(auth
            .check_file_access("/home/user/documents/file.txt")
            .is_ok());

        // Should block system files
        assert!(auth.check_file_access("/etc/passwd").is_err());
        assert!(auth
            .check_file_access(r"C:\Windows\System32\config")
            .is_err());
    }

    #[test]
    fn test_permissions() {
        assert!(Permission::Admin.check(&Permission::Delete));
        assert!(Permission::Admin.check(&Permission::Write));
        assert!(Permission::Admin.check(&Permission::Read));

        assert!(Permission::Write.check(&Permission::Read));
        assert!(!Permission::Read.check(&Permission::Write));
        assert!(!Permission::Write.check(&Permission::Delete));
    }
}
