//! Content extraction module for various file types.
//!
//! This module provides a unified interface for extracting text content from
//! different file formats including PDF, DOCX, HTML, CSV, and plain text.

mod archive_budget;
mod csv;
mod docx;
mod html;
mod markup;
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

pub use mime::is_code_mime;
pub use types::{ContentMetadata, ExtractedContent};

pub use streaming::extract_text_streaming;

use crate::features::indexing::engine::error::Result;
use lattice_core::application::ports::OcrPort;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::sync::Semaphore;

/// Default maximum file size: 50 MB
const DEFAULT_MAX_FILE_SIZE_BYTES: u64 = 50 * 1024 * 1024;

/// Admission for the blocking parsers that can hold a whole document in
/// memory: PDFs and office archives each run at most two at a time.
///
/// A parser that times out still owns its memory and CPU until it returns, so
/// its permit lives inside the blocking closure and a retry cannot get past
/// the bound. The bound is only worth anything if everything that parses
/// shares it: the composition root builds one set and hands it to every
/// extractor in the app, and an extractor built without one has its own.
#[derive(Debug, Clone)]
pub struct ExtractionWorkers {
    pdf: Arc<Semaphore>,
    archives: Arc<Semaphore>,
}

impl ExtractionWorkers {
    pub fn new() -> Self {
        Self {
            pdf: Arc::new(Semaphore::new(pdf::MAX_PDF_WORKERS)),
            archives: Arc::new(Semaphore::new(archive_budget::MAX_ARCHIVE_WORKERS)),
        }
    }
}

impl Default for ExtractionWorkers {
    fn default() -> Self {
        Self::new()
    }
}

/// Main content extractor with configurable size limits.
///
/// The extractor automatically detects file types and routes to the
/// appropriate extraction module.
pub struct ContentExtractor {
    max_file_size: u64,
    /// Consulted for scanned PDF pages. `None` leaves those pages in
    /// `ExtractedContent::needs_ocr` and indexes the rest of the document.
    ocr: Option<Arc<dyn OcrPort>>,
    /// Bounds the PDF and office parsers this extractor starts.
    workers: ExtractionWorkers,
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
            workers: ExtractionWorkers::new(),
        }
    }

    /// Share `workers` with the other extractors in the app, so their parsers
    /// count against one bound.
    pub fn with_workers(mut self, workers: ExtractionWorkers) -> Self {
        self.workers = workers;
        self
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
    pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedContent> {
        let mime_type = mime::detect_mime_type(path)?;

        match mime_type.as_str() {
            // Text files
            "text/plain" | "text/markdown" => {
                text::extract_text_file(path, self.max_file_size).await
            }

            // Documents
            "application/pdf" => {
                pdf::extract_pdf(path, self.max_file_size, self.ocr.as_ref(), &self.workers).await
            }
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                docx::extract_docx(path, self.max_file_size, &self.workers).await
            }
            "application/rtf" => rtf::extract_rtf(path, self.max_file_size).await,
            "application/vnd.oasis.opendocument.text" => {
                odt::extract_odt(path, self.max_file_size, &self.workers).await
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
                xlsx::extract_xlsx(path, self.max_file_size, &self.workers).await
            }
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                pptx::extract_pptx(path, self.max_file_size, &self.workers).await
            }

            // CSV/TSV. Ahead of the `text/` catch-all below, which would
            // otherwise swallow both of these and index a spreadsheet as code.
            "text/csv" | "text/tab-separated-values" => {
                csv::extract_csv_file(path, &mime_type, self.max_file_size).await
            }

            // Code files (treat as text with proper MIME type)
            mime if mime.starts_with("text/")
                || mime == "application/json"
                || mime == "application/xml"
                || mime == "application/graphql" =>
            {
                html::extract_code_file(path, &mime_type, self.max_file_size).await
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

    /// `text/csv` matches the `text/` catch-all as readily as the arm meant
    /// for it, so the arms' order is what decides whether a spreadsheet is
    /// indexed as rows or as a wall of code.
    #[tokio::test]
    async fn a_csv_file_reaches_the_csv_extractor() {
        let dir = tempfile::tempdir().unwrap();
        for (name, separator, mime) in [
            ("table.csv", ",", "text/csv"),
            ("table.tsv", "\t", "text/tab-separated-values"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(
                &path,
                format!("city{separator}population\nKyoto{separator}1463723\n"),
            )
            .unwrap();
            let content = ContentExtractor::new()
                .extract_from_file(&path)
                .await
                .unwrap();
            assert_eq!(content.mime_type, mime);
            // The "header: value" shape only the CSV extractor produces.
            assert!(
                content.text.contains("population: 1463723"),
                "{name} was not read as a table: {}",
                content.text
            );
        }
    }
}
