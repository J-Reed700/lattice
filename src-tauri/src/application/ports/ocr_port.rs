//! Port for optical character recognition of document pages.
//!
//! PDF extraction reconstructs text from a page's content stream. A scanned
//! page has no content stream text — only an image — so those pages are
//! reported per page (`ExtractedContent::needs_ocr`) and, when a provider is
//! configured, handed to this port one page at a time.
//!
//! # Why `(&Path, page)` and not image bytes
//!
//! The obvious alternative is for the caller to hand over pixels. It was
//! rejected because the caller cannot produce them honestly:
//!
//! - A scanned page's image XObject can be CCITTFax, JBIG2 or JPXDecode.
//!   Neither `llama-server` nor a native vision path accepts those, so
//!   something would still have to rasterize — and rasterizing is exactly the
//!   infrastructure dependency (pdfium / MuPDF / CoreGraphics) that the
//!   extraction engine must not grow.
//! - A page is not always one image. Rendering the page is the only faithful
//!   input for a page that mixes several image fragments, or a scan with a
//!   vector overlay.
//!
//! So the port receives the document path plus a one-based physical page
//! number, and the adapter owns rasterization plus inference. That is the same
//! shape as [`crate::application::ports::TranscriptionPort`], which also takes
//! a path and lets the adapter own decoding.
//!
//! # What a llama.cpp (Qwen2.5-VL) adapter would do with it
//!
//! 1. Render page `page` of `document` to a PNG at ~150 DPI, longest side
//!    capped (Qwen2.5-VL's window makes >1568px wasteful).
//! 2. Spawn/reuse a `llama-server` sidecar started with both the text weights
//!    (`-m qwen2.5-vl-7b-instruct-q4_k_m.gguf`) and the vision projector
//!    (`--mmproj mmproj-qwen2.5-vl-7b-f16.gguf`). The projector is a second
//!    file that today's download catalog entry does not list.
//! 3. POST `/v1/chat/completions` with an OpenAI-shaped multimodal message:
//!    a `text` part with the transcription instruction and an `image_url`
//!    part carrying `data:image/png;base64,...`.
//! 4. Return the assistant message as the page's text.
//!
//! Implementations must be `Send + Sync`, must not load a model until the
//! first `recognize_page` call (nothing may block app startup), and must run
//! inference off the async runtime.

use async_trait::async_trait;
use std::fmt;
use std::path::Path;

/// Why a page could not be recognized.
///
/// This is a typed error rather than a string so callers can tell "the user
/// has no OCR model" (not a failure — the document is simply indexed without
/// the scanned pages) from "OCR ran and broke" (worth surfacing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// No vision model is downloaded or selected. The only error
    /// [`NoopOcr`] ever returns.
    NotConfigured,
    /// The page could not be turned into an image for the model.
    PageRender { page: u32, reason: String },
    /// The model was reachable but did not produce text for the page.
    Recognition { page: u32, reason: String },
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "No OCR model is configured. Download a vision model in Settings → AI → Models to index scanned pages."
            ),
            Self::PageRender { page, reason } => {
                write!(f, "Could not render page {page} for OCR: {reason}")
            }
            Self::Recognition { page, reason } => {
                write!(f, "OCR failed on page {page}: {reason}")
            }
        }
    }
}

impl std::error::Error for OcrError {}

/// Recognizes the text of a single document page with a vision model.
#[async_trait]
pub trait OcrPort: Send + Sync {
    /// Recognize the text of one page of `document`.
    ///
    /// `page` is the one-based physical page number, matching the page numbers
    /// in `ExtractedContent::page_ranges` and `needs_ocr`.
    ///
    /// Returns the recognized text in reading order. An empty string is a
    /// valid answer for a blank scan and is not an error.
    ///
    /// # Errors
    ///
    /// - [`OcrError::NotConfigured`] when no vision model is available.
    /// - [`OcrError::PageRender`] when the page cannot be rasterized.
    /// - [`OcrError::Recognition`] when inference fails.
    async fn recognize_page(&self, document: &Path, page: u32) -> Result<String, OcrError>;
}

/// The default provider: no vision model, so nothing is recognized.
///
/// Wired in by default so the rest of the pipeline always has a port to call
/// and the "install a model" message comes from one place. Callers treat
/// [`OcrError::NotConfigured`] as "leave these pages in `needs_ocr`", never as
/// a failed import.
pub struct NoopOcr;

#[async_trait]
impl OcrPort for NoopOcr {
    async fn recognize_page(&self, _document: &Path, _page: u32) -> Result<String, OcrError> {
        Err(OcrError::NotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_reports_a_missing_model_rather_than_empty_text() {
        let error = NoopOcr
            .recognize_page(Path::new("scan.pdf"), 1)
            .await
            .unwrap_err();

        assert_eq!(error, OcrError::NotConfigured);
        assert!(error.to_string().contains("Settings"));
    }

    #[test]
    fn page_failures_name_the_page() {
        let error = OcrError::Recognition {
            page: 7,
            reason: "sidecar exited".into(),
        };

        assert!(error.to_string().contains("page 7"));
        assert!(error.to_string().contains("sidecar exited"));
    }
}
