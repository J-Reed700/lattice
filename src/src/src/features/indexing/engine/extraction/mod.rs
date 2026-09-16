//! Content extraction module for various file types.
//!
//! This module provides a unified interface for extracting text content from
//! different file formats including PDF, DOCX, HTML, CSV, and plain text.
//!
//! # Architecture
//!
//! The module is split into focused sub-modules:
//! - `types`: Shared data structures
//! - `mime`: MIME type detection
//! - `text`: Plain text and markdown extraction
//! - `pdf`: PDF document extraction with page tracking
//! - `docx`: Word document extraction
//! - `html`: HTML and code file extraction
//! - `csv`: CSV/TSV file extraction
//! - `streaming`: Streaming text extraction
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use crate::features::indexing::engine::extraction::ContentExtractor;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let extractor = ContentExtractor::new();
//! let content = extractor.extract_from_file(Path::new("document.pdf")).await?;
//! println!("Extracted {} characters", content.metadata.char_count);
//! # Ok(())
//! # }
//! ```

mod csv;
mod docx;
mod html;
mod mime;
mod odt;
mod pdf;
mod pdf_layout;
mod pptx;
mod rtf;
mod streaming;
mod text;
mod types;
mod xlsx;

pub use types::{ContentMetadata, ExtractedContent};

pub use streaming::extract_text_streaming;

use crate::application::ports::OcrPort;
use crate::features::indexing::engine::error::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;

/// Default maximum file size: 50 MB
const DEFAULT_MAX_FILE_SIZE_BYTES: u64 = 50 * 1024 * 1024;

/// Main content extractor with configurable size limits.
///
/// The extractor automatically detects file types and routes to the
/// appropriate extraction module.
pub struct ContentExtractor {
    max_file_size: u64,
    /// Consulted for scanned PDF pages. `None` leaves those pages in
    /// `ExtractedContent::needs_ocr` and indexes the rest of the document.
    ocr: Option<Arc<dyn OcrPort>>,
}

impl ContentExtractor {
    /// Create a new extractor with default settings.
    pub fn new() -> Self {
        Self::with_max_size(DEFAULT_MAX_FILE_SIZE_BYTES)
    }

    /// Create an extractor with a custom maximum file size.
    pub fn with_max_size(max_file_size: u64) -> Self {
        Self {
            max_file_size,
            ocr: None,
        }
    }

    /// Enable OCR for scanned PDF pages.
    ///
    /// Without this the extractor still succeeds on a partly scanned PDF; the
    /// scanned pages are simply reported in `ExtractedContent::needs_ocr`.
    pub fn with_ocr(mut self, ocr: Arc<dyn OcrPort>) -> Self {
        self.ocr = Some(ocr);
        self
    }

    /// Extract content from a file, automatically detecting the type.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to extract
    ///
    /// # Returns
    ///
    /// * `Ok(ExtractedContent)` - Successfully extracted content with metadata
    /// * `Err(IndexingError)` - File not found, unsupported type, or extraction error
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use crate::features::indexing::engine::extraction::ContentExtractor;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let extractor = ContentExtractor::new();
    /// let content = extractor.extract_from_file(Path::new("doc.pdf")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedContent> {
        let mime_type = mime::detect_mime_type(path)?;

        match mime_type.as_str() {
            // Text files
            "text/plain" | "text/markdown" => {
                text::extract_text_file(path, self.max_file_size).await
            }

            // Documents
            "application/pdf" => {
                pdf::extract_pdf(path, self.max_file_size, self.ocr.as_ref()).await
            }
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                docx::extract_docx(path, self.max_file_size).await
            }
            "application/rtf" => rtf::extract_rtf(path, self.max_file_size).await,
            "application/vnd.oasis.opendocument.text" => {
                odt::extract_odt(path, self.max_file_size).await
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
                xlsx::extract_xlsx(path, self.max_file_size).await
            }
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                pptx::extract_pptx(path, self.max_file_size).await
            }

            // Code files (treat as text with proper MIME type)
            mime if mime.starts_with("text/")
                || mime == "application/json"
                || mime == "application/xml"
                || mime == "application/graphql" =>
            {
                html::extract_code_file(path, &mime_type, self.max_file_size).await
            }

            // CSV/TSV
            "text/csv" | "text/tab-separated-values" => {
                csv::extract_csv_file(path, &mime_type, self.max_file_size).await
            }

            _ => Err(
                crate::features::indexing::engine::error::IndexingError::UnsupportedFileType {
                    path: path.display().to_string(),
                    detected_type: mime_type,
                },
            ),
        }
    }

    /// Extract text from a stream.
    ///
    /// Currently only supports plain text and markdown streams.
    pub async fn extract_text_streaming<R: AsyncRead + Unpin>(
        reader: R,
        mime_type: &str,
    ) -> Result<Vec<String>> {
        streaming::extract_text_streaming(reader, mime_type).await
    }

    /// Get a list of all supported file extensions.
    pub fn supported_extensions() -> &'static [&'static str] {
        mime::supported_extensions()
    }

    /// Check if a file path has a supported extension.
    pub fn is_supported(&self, path: &Path) -> bool {
        mime::is_supported(path)
    }
}

impl Default for ContentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_extensions() {
        let extractor = ContentExtractor::new();

        assert!(extractor.is_supported(Path::new("test.txt")));
        assert!(extractor.is_supported(Path::new("test.md")));
        assert!(extractor.is_supported(Path::new("test.pdf")));
        assert!(extractor.is_supported(Path::new("test.docx")));

        assert!(!extractor.is_supported(Path::new("test.exe")));
        assert!(!extractor.is_supported(Path::new("test.jpg")));
    }
}
