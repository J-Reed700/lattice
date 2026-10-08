//! Architecture support for the exact pinned local engine. This is a format
//! check, not a promise that every quantization fits or runs on every machine.
use super::gguf_metadata::architecture_from_prefix;
use crate::shared::error::{AppError, Result};
use futures::StreamExt;
use std::time::Duration;

const ARCHITECTURES: &str = include_str!("../../../../scripts/llama-architectures.txt");
const HEADER_LIMIT: usize = 4 * 1024 * 1024;

fn check_architecture(architecture: &str) -> Result<()> {
    if ARCHITECTURES
        .lines()
        .skip(2)
        .any(|entry| entry == architecture)
    {
        return Ok(());
    }
    Err(AppError::InvalidInput(format!(
        "This model uses the '{architecture}' architecture, which Lattice's bundled local engine does not support. Update Lattice or choose a compatible model. The model weights were not downloaded."
    )))
}

/// Read a small prefix, including when a server ignores Range. Never buffer a
/// whole GGUF to decide whether downloading it is worthwhile.
pub(crate) async fn check_download(url: &str, token: Option<&str>) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AppError::Network("Could not start the model compatibility check".into()))?;
    let mut request = client
        .get(url)
        .header("Range", format!("bytes=0-{}", HEADER_LIMIT - 1));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|_| {
        AppError::Network(
            "Could not read the model header to check compatibility. Please retry.".into(),
        )
    })?;
    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "The model compatibility check failed (HTTP {}). Check access to the selected model and retry.",
            response.status().as_u16()
        )));
    }
    let mut stream = response.bytes_stream();
    let mut prefix = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            AppError::Network("The model header download was interrupted. Please retry.".into())
        })?;
        prefix.extend(chunk.iter().take(HEADER_LIMIT.saturating_sub(prefix.len())));
        if let Some(architecture) = architecture_from_prefix(&prefix) {
            return check_architecture(&architecture);
        }
        if prefix.len() == HEADER_LIMIT {
            break;
        }
    }
    Err(AppError::InvalidInput(
        "Could not determine this model's GGUF architecture from its header. Its compatibility is unknown, so the model weights were not downloaded.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn gguf(architecture: &str) -> Vec<u8> {
        let mut data = b"GGUF".to_vec();
        data.extend(3u32.to_le_bytes());
        data.extend(0u64.to_le_bytes());
        data.extend(1u64.to_le_bytes());
        let key = "general.architecture";
        data.extend((key.len() as u64).to_le_bytes());
        data.extend(key.as_bytes());
        data.extend(8u32.to_le_bytes());
        data.extend((architecture.len() as u64).to_le_bytes());
        data.extend(architecture.as_bytes());
        data
    }

    #[test]
    fn manifest_matches_the_engine_pin() {
        let lock = include_str!("../../../../scripts/llama-server.lock");
        let tag = lock
            .lines()
            .find_map(|line| line.strip_prefix("llama_cpp_tag "))
            .unwrap();
        assert_eq!(ARCHITECTURES.lines().nth(1), Some(tag));
        assert!(check_architecture("llama").is_ok());
        assert!(check_architecture("future-unknown-family").is_err());
    }

    #[tokio::test]
    async fn inspects_the_exact_file_with_range_and_auth_before_accepting_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/selected.gguf"))
            .and(header("range", "bytes=0-4194303"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(gguf("llama")))
            .expect(1)
            .mount(&server)
            .await;
        check_download(
            &format!("{}/selected.gguf", server.uri()),
            Some("test-token"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rejects_unsupported_or_missing_architecture_even_if_range_is_ignored() {
        let server = MockServer::start().await;
        for (file, body) in [
            ("unknown", gguf("future-unknown-family")),
            ("invalid", b"not GGUF".to_vec()),
        ] {
            Mock::given(path(format!("/{file}")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&server)
                .await;
            let error = check_download(&format!("{}/{file}", server.uri()), None)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("not downloaded"));
        }
    }

    #[test]
    fn truncated_headers_never_claim_compatibility() {
        let header = gguf("llama");
        for length in 0..header.len() {
            assert!(architecture_from_prefix(&header[..length]).is_none());
        }
        assert_eq!(architecture_from_prefix(&header).as_deref(), Some("llama"));
    }
}
