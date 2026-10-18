//! Contract for capturing a cited web page as one self-contained, inert HTML
//! document: the "frozen copy" the Page view renders.
//!
//! Design: `docs/design/2026-10-13-frozen-web-snapshots.md` (§6.5). The engine
//! behind it lives in `features::web::services::snapshot`; the storage, job
//! and URL scheme that use it live in `features::web::snapshots`.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

/// What to capture. `snapshot_id` is allocated by the store before capture so
/// the sanitizer can route every rewritten link through
/// `/l?u=<url>&s=<snapshot_id>` (§8.3).
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub url: String,
    pub snapshot_id: String,
}

/// A progress report; `done`/`total` are abstract units (resources), not bytes.
#[derive(Debug, Clone)]
pub struct CaptureProgress {
    pub done: u32,
    pub total: u32,
    pub message: String,
}

/// Where a capture's HTML came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMethod {
    /// Fetched over plain HTTP and sanitized.
    Http,
    /// Rendered in the hidden browser window, then sanitized (wave 2).
    Browser,
}

impl CaptureMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Browser => "browser",
        }
    }
}

/// Bytes with their declared type, ready to become a `data:` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataUri {
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl DataUri {
    pub fn to_data_url(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.mime,
            base64::engine::general_purpose::STANDARD.encode(&self.bytes)
        )
    }
}

/// A subresource that could not be inlined, with the reason the design names
/// (`too_large`, `over_budget`, `blocked_host`, `bad_type`, `http_<status>`,
/// `timeout`, `network`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedResource {
    pub url: String,
    pub reason: String,
}

/// What happened to the page's subresources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceReport {
    pub inlined: u32,
    pub dropped: Vec<DroppedResource>,
    pub bytes_inlined: u64,
}

/// The finished, self-contained document and what is known about it.
#[derive(Debug, Clone)]
pub struct CapturedPage {
    pub requested_url: String,
    pub final_url: String,
    pub title: Option<String>,
    /// The complete document, UTF-8, with the envelope of §6.4 rule E.
    pub html: String,
    /// At most 16 KiB; shown in the viewer's address pill, never inlined.
    pub favicon: Option<DataUri>,
    pub method: CaptureMethod,
    pub resources: ResourceReport,
    /// A cap or budget was hit; `partial_reasons` says which.
    pub partial: bool,
    pub partial_reasons: Vec<String>,
    pub captured_at: DateTime<Utc>,
}

/// Why a capture did not produce a page.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// 403, 429 or 503: a JavaScript check or rate limit a browser may pass.
    #[error("the site answered {status}")]
    Blocked { status: u16 },
    #[error("unsupported content type {mime}")]
    UnsupportedContentType { mime: String },
    #[error("the page is larger than the capture limit")]
    TooLarge,
    #[error("the capture ran out of time")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
    /// The address guard, a malformed URL, or a non-2xx status that is not
    /// `Blocked`.
    #[error("refused: {0}")]
    Refused(String),
    #[error("cancelled")]
    Cancelled,
}

impl CaptureError {
    /// Whether a later attempt could succeed without anything changing.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout)
    }

    /// A stable code for the snapshot row and the viewer.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Blocked { .. } => "blocked",
            Self::UnsupportedContentType { .. } => "unsupported_content_type",
            Self::TooLarge => "too_large",
            Self::Timeout => "timeout",
            Self::Network(_) => "network",
            Self::Refused(_) => "refused",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Receives progress; called at most a few times a second.
pub type ProgressSink = Arc<dyn Fn(CaptureProgress) + Send + Sync>;

/// Captures one page. Implementations must apply the address guard to the
/// page and to every subresource, the size and time caps of §6.3, and the
/// sanitization rules of §6.4; they never execute page content.
#[async_trait]
pub trait PageCapturePort: Send + Sync {
    async fn capture(
        &self,
        request: CaptureRequest,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> std::result::Result<CapturedPage, CaptureError>;

    fn attach_browser(
        &self,
        _browser: Arc<crate::features::web::services::browser_reader::BrowserReader>,
    ) {
    }
}
