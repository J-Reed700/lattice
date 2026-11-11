//! The authentication header of a remote model server. Its name lives in
//! settings; its value lives in the keyring and is read here, at the moment a
//! connection is made.

use super::keys::CredentialKey;
use lattice_core::application::contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto};
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::shared::error::Result;

/// The value of the header stored under `key`. A value still carried by the
/// settings (a legacy file whose move to the keyring has not yet succeeded)
/// is used until the keyring has it.
async fn header_value(
    credentials: &dyn CredentialsPort,
    key: CredentialKey,
    in_settings: &str,
) -> Result<String> {
    if !in_settings.trim().is_empty() {
        return Ok(in_settings.trim().to_owned());
    }
    Ok(credentials
        .get_api_key(key.as_str())
        .await?
        .unwrap_or_default()
        .trim()
        .to_owned())
}

/// The Ollama header as `(name, value)`, when both are set.
pub async fn ollama_auth_header(
    credentials: &dyn CredentialsPort,
    settings: &LLMSettingsDto,
) -> Result<Option<(String, String)>> {
    let name = settings.ollama_auth_header_name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let value = header_value(
        credentials,
        CredentialKey::OllamaAuthHeader,
        &settings.ollama_auth_header_value,
    )
    .await?;
    Ok((!value.is_empty()).then(|| (name.to_owned(), value)))
}

/// `connection` with its header value filled in from the keyring.
pub async fn resolved_llama_cpp(
    credentials: &dyn CredentialsPort,
    connection: &LlamaCppSettingsDto,
) -> Result<LlamaCppSettingsDto> {
    let mut resolved = connection.clone();
    resolved.auth_header_value = if connection.auth_header_name.trim().is_empty() {
        String::new()
    } else {
        header_value(
            credentials,
            CredentialKey::LlamaCppAuthHeader,
            &connection.auth_header_value,
        )
        .await?
    };
    Ok(resolved)
}

/// Whether the Ollama header has a value, wherever it is held.
pub async fn ollama_value_is_set(
    credentials: &dyn CredentialsPort,
    settings: &LLMSettingsDto,
) -> Result<bool> {
    Ok(!header_value(
        credentials,
        CredentialKey::OllamaAuthHeader,
        &settings.ollama_auth_header_value,
    )
    .await?
    .is_empty())
}

/// Whether the llama.cpp header has a value, wherever it is held.
pub async fn llama_cpp_value_is_set(
    credentials: &dyn CredentialsPort,
    connection: &LlamaCppSettingsDto,
) -> Result<bool> {
    Ok(!header_value(
        credentials,
        CredentialKey::LlamaCppAuthHeader,
        &connection.auth_header_value,
    )
    .await?
    .is_empty())
}

/// The stored Ollama value for a connection test that names the saved header
/// and leaves the value blank, so a test does not need the secret retyped.
pub async fn saved_ollama_value_for(
    credentials: &dyn CredentialsPort,
    saved: &LLMSettingsDto,
    name: &str,
) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name != saved.ollama_auth_header_name.trim() {
        return Ok(String::new());
    }
    header_value(
        credentials,
        CredentialKey::OllamaAuthHeader,
        &saved.ollama_auth_header_value,
    )
    .await
}

/// The llama.cpp counterpart of `saved_ollama_value_for`.
pub async fn saved_llama_cpp_value_for(
    credentials: &dyn CredentialsPort,
    saved: &LlamaCppSettingsDto,
    name: &str,
) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name != saved.auth_header_name.trim() {
        return Ok(String::new());
    }
    header_value(
        credentials,
        CredentialKey::LlamaCppAuthHeader,
        &saved.auth_header_value,
    )
    .await
}
