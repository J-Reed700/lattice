//! Port for optical character recognition of document pages. A scanned page
//! has no content-stream text, so it is reported per page
//! (`ExtractedContent::needs_ocr`) and handed here one page at a time.
//!
//! The port takes `(&Path, page)` rather than image bytes: a scan's image may
//! be CCITTFax, JBIG2 or JPXDecode and a page may mix fragments, so only
//! rendering the page is faithful, and rasterizing belongs to the adapter, not
//! the extraction engine (as with `TranscriptionPort`). An image file is a
//! one-page document. `features::ocr::VisionOcr` starts a dedicated
//! `llama-server` with the OCR model only when a page needs it.
//! Implementations must not load a model before the first `recognize_page`
//! call, and must run inference off the async runtime.

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
    /// OCR cannot run at all right now: the OCR server would not start, or
    /// this platform cannot render the document. Unlike a page failure, every
    /// later page would fail the same way, so callers stop asking.
    Unavailable { reason: String },
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "No OCR model is downloaded. Download PaddleOCR-VL in Settings → AI → Models to index scanned pages."
            ),
            Self::PageRender { page, reason } => {
                write!(f, "Could not render page {page} for OCR: {reason}")
            }
            Self::Recognition { page, reason } => {
                write!(f, "OCR failed on page {page}: {reason}")
            }
            Self::Unavailable { reason } => write!(f, "OCR is unavailable: {reason}"),
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
    /// - [`OcrError::Unavailable`] when no page of this document can be
    ///   recognized right now.
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
