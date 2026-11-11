//! The names credentials are stored under in the keyring, and the cloud key
//! lookup every caller shares.

use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::shared::error::Result;

/// A credential other than a cloud provider's API key, which is stored under
/// the provider's id (`openai`, `anthropic`, `google`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKey {
    HuggingFaceToken,
    OllamaAuthHeader,
    LlamaCppAuthHeader,
}

impl CredentialKey {
    pub const ALL: [CredentialKey; 3] = [
        CredentialKey::HuggingFaceToken,
        CredentialKey::OllamaAuthHeader,
        CredentialKey::LlamaCppAuthHeader,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CredentialKey::HuggingFaceToken => "huggingface_token",
            CredentialKey::OllamaAuthHeader => "ollama_auth_header_value",
            CredentialKey::LlamaCppAuthHeader => "llama_cpp_auth_header_value",
        }
    }
}

/// A cloud provider's API key: the `<PROVIDER>_API_KEY` environment variable
/// (`GOOGLE_API_KEY`, then `GEMINI_API_KEY`, for `google`) wins over the
/// keyring entry named for the provider. A blank key is no key.
pub async fn cloud_api_key(
    credentials: &dyn CredentialsPort,
    provider: &str,
) -> Result<Option<String>> {
    let from_env = if provider == "google" {
        std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .ok()
    } else {
        std::env::var(format!("{}_API_KEY", provider.to_uppercase())).ok()
    };
    let key = match from_env {
        Some(key) => Some(key),
        None => credentials.get_api_key(provider).await?,
    };
    Ok(key.filter(|key| !key.trim().is_empty()))
}
