//! The authentication header of a remote model server. Its name lives in
//! settings; its value lives in the keyring and is read here, at the moment a
//! connection is made. Nothing else holds the value.

use super::CredentialKey;
use crate::application::ports::credentials_port::CredentialsPort;
use crate::application::ports::llm_port::{RemoteHeader, RemoteServer};
use crate::shared::error::{AppError, Result};

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

/// The header for a connection test. A typed value is used as given. A blank
/// value with a name means "the stored value", but only for the saved URL of
/// that server.
pub async fn header_for_test(
    credentials: &dyn CredentialsPort,
    server: RemoteServer,
    tested_url: &str,
    name: &str,
    typed_value: &str,
    saved_url: &str,
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
    if name.is_empty() {
        return Ok(None);
    }
    if !same_server_url(tested_url, saved_url) {
        return Err(AppError::InvalidConfig(
            "Type the header value to test a server other than the saved one".into(),
        ));
    }
    header(credentials, server, name).await
}

fn same_server_url(left: &str, right: &str) -> bool {
    let normalize = |url: &str| url.trim().trim_end_matches('/').to_owned();
    normalize(left) == normalize(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::credentials::memory::InMemoryCredentials;

    const SAVED: &str = "http://saved:11434";

    async fn stored(value: &str) -> InMemoryCredentials {
        let credentials = InMemoryCredentials::default();
        credentials
            .set_api_key(
                CredentialKey::for_server(RemoteServer::Ollama).as_str(),
                value,
            )
            .await
            .unwrap();
        credentials
    }

    #[tokio::test]
    async fn a_name_with_no_value_typed_or_stored_is_refused() {
        let credentials = InMemoryCredentials::default();
        let result = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            SAVED,
            "X-Api-Key",
            "  ",
            SAVED,
        )
        .await;
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
            SAVED,
            " ",
            "secret",
            SAVED,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidConfig(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn no_name_and_no_value_sends_no_header() {
        let credentials = stored("secret").await;
        let result = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            "http://other",
            " ",
            "",
            SAVED,
        )
        .await;
        assert!(matches!(result, Ok(None)), "{result:?}");
    }

    #[tokio::test]
    async fn a_stored_value_is_not_sent_to_a_url_other_than_the_saved_one() {
        let credentials = stored("secret").await;
        let result = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            "http://typo:11434",
            "X-Api-Key",
            "",
            SAVED,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidConfig(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn the_stored_value_is_used_for_the_saved_url_under_a_renamed_header() {
        let credentials = stored("secret").await;
        let header = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            SAVED,
            "X-Other",
            "",
            SAVED,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(header.name, "X-Other");
        assert_eq!(header.value, "secret");
    }

    #[tokio::test]
    async fn the_saved_url_with_a_trailing_slash_or_spaces_is_the_same_server() {
        let credentials = stored("secret").await;
        for tested in ["http://saved:11434/", "  http://saved:11434  "] {
            let header = header_for_test(
                &credentials,
                RemoteServer::Ollama,
                tested,
                "X-Api-Key",
                "",
                SAVED,
            )
            .await
            .unwrap();
            assert!(header.is_some(), "{tested}");
        }
    }

    #[tokio::test]
    async fn the_saved_url_with_a_name_and_nothing_stored_is_refused() {
        let credentials = InMemoryCredentials::default();
        let result = header_for_test(
            &credentials,
            RemoteServer::Ollama,
            SAVED,
            "X-Api-Key",
            "",
            "http://saved:11434/",
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidConfig(_))),
            "{result:?}"
        );
    }
}
