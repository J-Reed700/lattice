//! A raw, bounded GET through the service's guarded client and site pacer,
//! for callers in the web feature that need bytes rather than page text —
//! the chat pictures in `features::web::images`. It adds no second client:
//! the address guard, redirect policy and per-site pacing are the ones every
//! page read goes through.

use std::time::Instant;

use reqwest::header::HeaderMap;

use super::{validate_url_resolved, WebService};

/// One body read by [`WebService::fetch_bytes_bounded`].
/// The `Content-Type` header is deliberately not carried: callers that care
/// what the bytes are sniff them.
#[derive(Debug, Clone)]
pub struct BoundedBody {
    pub bytes: Vec<u8>,
    /// Where the request ended after redirects.
    pub final_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedFetchError {
    /// The URL failed the scheme or address check; nothing was sent.
    Refused(String),
    /// The request or the body read failed.
    Network(String),
    /// The server answered with a non-success status.
    Status(u16),
    /// The body is larger than the caller's bound.
    TooLarge,
    /// The caller's deadline passed first.
    Deadline,
}

impl std::fmt::Display for BoundedFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(reason) => write!(f, "refused: {reason}"),
            Self::Network(reason) => write!(f, "network error: {reason}"),
            Self::Status(status) => write!(f, "HTTP {status}"),
            Self::TooLarge => f.write_str("response larger than allowed"),
            Self::Deadline => f.write_str("ran out of time"),
        }
    }
}

impl WebService {
    /// GET `url` with `headers`, reading at most `max_bytes` of decoded body.
    ///
    /// `deadline` is the caller's own (a chat turn's). When given it bounds
    /// the address check, the site-slot wait, the request and the body read,
    /// and it replaces the client's default request timeout for this request,
    /// so the turn's clock is the only one involved.
    pub(crate) async fn fetch_bytes_bounded(
        &self,
        url: &str,
        headers: HeaderMap,
        max_bytes: usize,
        deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError> {
        let work = async {
            validate_url_resolved(url)
                .await
                .map_err(|error| BoundedFetchError::Refused(error.to_string()))?;
            let _site_slot = self.pacer.acquire(url).await;
            let mut request = self.client.get(url).headers(headers);
            if let Some(deadline) = deadline {
                request = request.timeout(deadline.saturating_duration_since(Instant::now()));
            }
            let mut response = request
                .send()
                .await
                .map_err(|error| BoundedFetchError::Network(error.to_string()))?;
            if !response.status().is_success() {
                return Err(BoundedFetchError::Status(response.status().as_u16()));
            }
            // Content-Length is only an early rejection hint; the loop below
            // also covers chunked and compressed bodies.
            if response
                .content_length()
                .is_some_and(|length| length > max_bytes as u64)
            {
                return Err(BoundedFetchError::TooLarge);
            }
            let final_url = response.url().to_string();
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| BoundedFetchError::Network(error.to_string()))?
            {
                if bytes.len().saturating_add(chunk.len()) > max_bytes {
                    return Err(BoundedFetchError::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(BoundedBody { bytes, final_url })
        };
        match deadline {
            Some(deadline) => {
                if Instant::now() >= deadline {
                    return Err(BoundedFetchError::Deadline);
                }
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), work)
                    .await
                    .map_err(|_| BoundedFetchError::Deadline)?
            }
            None => work.await,
        }
    }
}
