//! HuggingFace Settings Commands
//!
//! Commands for managing HuggingFace authentication tokens.
//! Tokens are stored in the OS keyring through the credentials port and are
//! never returned to the renderer; downloads read them from the same port.

use crate::features::credentials::keys::CredentialKey;
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

fn has_valid_token_format(token: &str) -> bool {
    token.starts_with("hf_") && token.len() >= 35
}

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
pub async fn set_huggingface_token(credentials: &dyn CredentialsPort, token: String) -> Result<()> {
    if !has_valid_token_format(&token) {
        error!("Invalid HuggingFace token format");

        return Err(AppError::InvalidInput(
            "Invalid HuggingFace token format. Must start with 'hf_' and be at least 35 characters"
                .to_string(),
        ));
    }

    credentials
        .set_api_key(CredentialKey::HuggingFaceToken.as_str(), &token)
        .await
        .inspect_err(|e| {
            error!(error = %e, "Failed to store HuggingFace token");
        })?;

    info!("HuggingFace token stored successfully");
    Ok(())
}

/// Get HuggingFace token status (whether it's set)
///
/// Returns status indicating if a token is configured.
/// Does not return the actual token value.
pub async fn get_huggingface_token_status(
    credentials: &dyn CredentialsPort,
) -> Result<HfTokenStatus> {
    let is_set = credentials
        .has_api_key(CredentialKey::HuggingFaceToken.as_str())
        .await?;

    Ok(HfTokenStatus { is_set })
}

/// Delete the stored HuggingFace token
///
/// Removes the token from OS keyring and logs the deletion.
pub async fn delete_huggingface_token(credentials: &dyn CredentialsPort) -> Result<()> {
    credentials
        .delete_api_key(CredentialKey::HuggingFaceToken.as_str())
        .await
        .inspect_err(|e| {
            error!(error = %e, "Failed to delete HuggingFace token");
        })?;

    info!("HuggingFace token deleted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::credentials::memory::InMemoryCredentials;

    #[test]
    fn test_token_validation() {
        // Valid token
        let valid_token = "hf_".to_string() + &"x".repeat(33);
        assert!(has_valid_token_format(&valid_token));

        // Invalid: too short
        let short_token = "hf_short".to_string();
        assert!(!has_valid_token_format(&short_token));

        // Invalid: wrong prefix
        let wrong_prefix = "sk_".to_string() + &"x".repeat(33);
        assert!(!has_valid_token_format(&wrong_prefix));
    }

    #[tokio::test]
    async fn a_token_is_stored_reported_as_set_and_deleted() {
        let keyring = InMemoryCredentials::default();
        let test_token = "hf_".to_string() + &"test".repeat(9);

        assert!(!get_huggingface_token_status(&keyring).await.unwrap().is_set);

        set_huggingface_token(&keyring, test_token.clone())
            .await
            .unwrap();

        assert!(get_huggingface_token_status(&keyring).await.unwrap().is_set);
        assert_eq!(
            keyring.stored(CredentialKey::HuggingFaceToken.as_str()),
            Some(test_token)
        );

        delete_huggingface_token(&keyring).await.unwrap();

        assert!(!get_huggingface_token_status(&keyring).await.unwrap().is_set);
    }
}
