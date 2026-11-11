//! The authentication header of a remote model server. Its name lives in
//! settings; its value lives in the keyring and is read here, at the moment a
//! connection is made. Nothing else holds the value.

use super::keys::CredentialKey;
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::application::ports::llm_port::{RemoteHeader, RemoteServer};
use lattice_core::shared::error::{AppError, Result};

fn pairing_error() -> AppError {
    AppError::InvalidConfig("Set both authentication header fields, or clear both".into())
}

async fn stored_value(
    credentials: &dyn CredentialsPort,
    server: RemoteServer,
) -> Result<Option<String>> {
    Ok(credentials
        .get_api_key(CredentialKey::for_server(server).as_str())
        .await?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

/// The header to send to `server`, whose configured header name is `name`.
/// No name means no header; a name with no stored value is a configuration
/// the user has to finish.
pub async fn header(
    credentials: &dyn CredentialsPort,
    server: RemoteServer,
    name: &str,
) -> Result<Option<RemoteHeader>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    match stored_value(credentials, server).await? {
        Some(value) => Ok(Some(RemoteHeader {
            name: name.to_owned(),
            value,
        })),
        None => Err(pairing_error()),
    }
}

/// Whether `server` has a header value stored.
pub async fn is_set(credentials: &dyn CredentialsPort, server: RemoteServer) -> Result<bool> {
    Ok(stored_value(credentials, server).await?.is_some())
}

/// The header for a connection test. A typed value is used as given; a blank
/// one means "the stored value" when `name` is the saved header name.
pub async fn header_for_test(
    credentials: &dyn CredentialsPort,
    server: RemoteServer,
    name: &str,
    typed_value: &str,
    saved_name: &str,
) -> Result<Option<RemoteHeader>> {
    let name = name.trim();
    let typed = typed_value.trim();
    if !typed.is_empty() {
        if name.is_empty() {
            return Err(pairing_error());
        }
        return Ok(Some(RemoteHeader {
            name: name.to_owned(),
            value: typed.to_owned(),
        }));
    }
    if !name.is_empty() && name != saved_name.trim() {
        return Err(pairing_error());
    }
    header(credentials, server, name).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::credentials::memory::InMemoryCredentials;

    #[tokio::test]
    async fn a_name_with_no_value_typed_or_stored_is_refused() {
        let credentials = InMemoryCredentials::default();
        let result =
            header_for_test(&credentials, RemoteServer::Ollama, "X-Api-Key", "  ", "").await;
        assert!(
            matches!(result, Err(AppError::InvalidConfig(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_typed_value_with_no_name_is_refused() {
        let credentials = InMemoryCredentials::default();
        let result = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            " ",
            "secret",
            "X-Api-Key",
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidConfig(_))),
            "{result:?}"
        );
    }
}
