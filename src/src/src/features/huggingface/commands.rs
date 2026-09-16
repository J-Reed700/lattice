//! HuggingFace Settings Commands
//!
//! Commands for managing HuggingFace authentication tokens.
//! Tokens are stored securely in the OS keyring via the SecureStorage service.

use crate::audit::AuditAction;
use crate::infrastructure::security::keyring_storage::SecureStorage;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

const HF_TOKEN_KEY: &str = "huggingface_token";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
// The wire said `is_set`; every consumer (and `types/api/credentials.ts`) reads
// `isSet`, so the status always deserialised as "not set".
#[serde(rename_all = "camelCase")]
pub struct HfTokenStatus {
    pub is_set: bool,
}

/// Store a HuggingFace authentication token securely
///
/// Validates token format and stores in OS keyring.
/// Token must start with "hf_" and be at least 35 characters.
pub async fn set_huggingface_token(token: String) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    // Validate token format (hf_xxx...)
    if !token.starts_with("hf_") || token.len() < 35 {
        error!("Invalid HuggingFace token format");

        crate::audit_failure!(
            logger,
            AuditAction::CredentialStored,
            "credential",
            "Invalid token format".to_string(),
            "key" => HF_TOKEN_KEY
        )
        .await
        .ok();

        return Err(
            "Invalid HuggingFace token format. Must start with 'hf_' and be at least 35 characters"
                .to_string(),
        );
    }

    // Store in keyring
    let storage = SecureStorage::new();
    storage.store_api_key(HF_TOKEN_KEY, &token).map_err(|e| {
        error!(error = %e, "Failed to store HuggingFace token");
        e.to_string()
    })?;

    // Audit log
    crate::audit_success!(
        logger,
        AuditAction::CredentialStored,
        "credential",
        "key" => HF_TOKEN_KEY
    )
    .await
    .ok();

    info!("HuggingFace token stored successfully");
    Ok(())
}

/// Get HuggingFace token status (whether it's set)
///
/// Returns status indicating if a token is configured.
/// Does not return the actual token value.
pub async fn get_huggingface_token_status() -> Result<HfTokenStatus, String> {
    let storage = SecureStorage::new();
    let is_set = storage
        .has_api_key(HF_TOKEN_KEY)
        .map_err(|e| e.to_string())?;

    Ok(HfTokenStatus { is_set })
}

/// Delete the stored HuggingFace token
///
/// Removes the token from OS keyring and logs the deletion.
pub async fn delete_huggingface_token() -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    let storage = SecureStorage::new();
    storage.delete_api_key(HF_TOKEN_KEY).map_err(|e| {
        error!(error = %e, "Failed to delete HuggingFace token");
        e.to_string()
    })?;

    // Audit log
    crate::audit_success!(
        logger,
        AuditAction::CredentialDeleted,
        "credential",
        "key" => HF_TOKEN_KEY
    )
    .await
    .ok();

    info!("HuggingFace token deleted");
    Ok(())
}

/// Get the actual HuggingFace token value
///
/// Returns the token if set, None if not configured.
/// This should only be used internally for download authentication.
pub async fn get_huggingface_token() -> Result<Option<String>, String> {
    let logger = crate::audit::get_audit_logger();

    let storage = SecureStorage::new();
    let token = storage
        .get_api_key(HF_TOKEN_KEY)
        .map_err(|e| e.to_string())?;

    if token.is_some() {
        crate::audit_success!(
            logger,
            AuditAction::CredentialAccessed,
            "credential",
            "key" => HF_TOKEN_KEY
        )
        .await
        .ok();
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_validation() {
        // Valid token
        let valid_token = "hf_".to_string() + &"x".repeat(33);
        assert!(set_huggingface_token(valid_token).await.is_ok());

        // Invalid: too short
        let short_token = "hf_short".to_string();
        assert!(set_huggingface_token(short_token).await.is_err());

        // Invalid: wrong prefix
        let wrong_prefix = "sk_".to_string() + &"x".repeat(33);
        assert!(set_huggingface_token(wrong_prefix).await.is_err());

        // Clean up
        delete_huggingface_token().await.ok();
    }

    #[tokio::test]
    #[ignore] // Requires OS keyring access, run manually with --ignored
    async fn test_token_lifecycle() {
        let test_token = "hf_".to_string() + &"test".repeat(9);

        // Initially should not be set (or clean up from previous run)
        let _ = delete_huggingface_token().await;

        let status = get_huggingface_token_status().await.unwrap();
        assert!(!status.is_set);

        // Set token
        set_huggingface_token(test_token.clone()).await.unwrap();

        // Should be set now
        let status = get_huggingface_token_status().await.unwrap();
        assert!(status.is_set);

        // Get token should return it
        let retrieved = get_huggingface_token().await.unwrap();
        assert_eq!(retrieved, Some(test_token));

        // Delete token
        delete_huggingface_token().await.unwrap();

        // Should not be set anymore
        let status = get_huggingface_token_status().await.unwrap();
        assert!(!status.is_set);
    }
}
