//! Content extraction module for various file types.
//!
//! This module provides a unified interface for extracting text content from
//! different file formats including PDF, DOCX, HTML, CSV, and plain text.
//!
//! # Architecture
//!
//! The module is split into focused sub-modules:
//! - `types`: Shared data structures
//! - `text`: Plain text and markdown extraction
//! - `pdf`: PDF document extraction with page tracking
//! - `docx`: Word document extraction
//! - `html`: HTML and code file extraction
//! - `csv`: CSV/TSV file extraction
//! - `streaming`: Streaming text extraction

mod archive_budget;
mod csv;
mod docx;
mod html;
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

use crate::features::indexing::engine::error::{IndexingError, Result};
use lattice_core::application::ports::OcrPort;
use lattice_core::domain::value_objects::{ExtractionRoute, FileFormat};
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::sync::Semaphore;

/// Validate file readability and enforce an optional caller-configured size limit.
pub(super) async fn check_file_size(path: &Path, max_file_size: Option<u64>) -> Result<()> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    if let Some(limit) = max_file_size {
        let file_size = metadata.len();
        if file_size > limit {
            return Err(IndexingError::FileTooLarge {
                path: path.display().to_string(),
                size_bytes: file_size,
                max_size_bytes: limit,
            });
        }
    }

    Ok(())
}

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

/// Main content extractor with optional caller-configured size limits.
///
/// The extractor automatically detects file types and routes to the
/// appropriate extraction module. By default (`ContentExtractor::new()`), local
/// user documents have no arbitrary file-size ceiling; callers that render into
/// bounded UI/IPC surfaces can opt into a ceiling with [`Self::with_max_size`].
pub struct ContentExtractor {
    max_file_size: Option<u64>,
    /// Consulted for scanned PDF pages. `None` leaves those pages in
    /// `ExtractedContent::needs_ocr` and indexes the rest of the document.
    ocr: Option<Arc<dyn OcrPort>>,
    /// Bounds the PDF and office parsers this extractor starts.
    workers: ExtractionWorkers,
}

impl ContentExtractor {
    /// Create a new extractor with no arbitrary file-size cap on local files.
    pub fn new() -> Self {
        Self {
            max_file_size: None,
            ocr: None,
            workers: ExtractionWorkers::new(),
        }
    }

    /// Create an extractor with a custom maximum file size.
    pub fn with_max_size(max_file_size: u64) -> Self {
        Self {
            max_file_size: Some(max_file_size),
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

    /// Extract content from a file, routed by the file-type catalogue.
    pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedContent> {
        let Some(format) = FileFormat::from_path(path) else {
            let detected_type = match path.extension().and_then(|s| s.to_str()) {
                Some(extension) => format!("unknown/{extension}"),
                None => "unknown".to_string(),
            };
            return Err(unsupported(path, detected_type));
        };
        let mime_type = format.mime_type;

        match format.route {
            Some(ExtractionRoute::PlainText) => {
                text::extract_text_file(path, self.max_file_size).await
            }
            Some(ExtractionRoute::Pdf) => {
                pdf::extract_pdf(path, self.max_file_size, self.ocr.as_ref(), &self.workers).await
            }
            Some(ExtractionRoute::Docx) => {
                docx::extract_docx(path, self.max_file_size, &self.workers).await
            }
            Some(ExtractionRoute::Rtf) => rtf::extract_rtf(path, self.max_file_size).await,
            Some(ExtractionRoute::Odt) => {
                odt::extract_odt(path, self.max_file_size, &self.workers).await
            }
            Some(ExtractionRoute::Xlsx) => {
                xlsx::extract_xlsx(path, self.max_file_size, &self.workers).await
            }
            Some(ExtractionRoute::Pptx) => {
                pptx::extract_pptx(path, self.max_file_size, &self.workers).await
            }
            Some(ExtractionRoute::Csv) => {
                csv::extract_csv_file(path, mime_type, self.max_file_size).await
            }
            Some(ExtractionRoute::Html) => {
                html::extract_html_file(path, mime_type, self.max_file_size).await
            }
            Some(ExtractionRoute::Code) => {
                html::extract_code_file(path, mime_type, self.max_file_size).await
            }
            // Audio is transcribed by `ContentExtractionAdapter`, not parsed here.
            Some(ExtractionRoute::Audio) | None => Err(unsupported(path, mime_type.to_string())),
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

    /// Every extension the indexer can read, audio included.
    pub fn supported_extensions() -> Vec<&'static str> {
        FileFormat::all()
            .iter()
            .filter(|format| format.is_indexable())
            .map(|format| format.extension)
            .collect()
    }

    /// Check if a file path has a supported extension.
    pub fn is_supported(&self, path: &Path) -> bool {
        FileFormat::from_path(path).is_some_and(FileFormat::is_indexable)
    }
}

fn unsupported(path: &Path, detected_type: String) -> IndexingError {
    IndexingError::UnsupportedFileType {
        path: path.display().to_string(),
        detected_type,
    }
}

/// Whether a MIME type is source code, config or a shell script rather than
/// prose. In these a `# ` line is a comment, never a section heading.
pub fn is_code_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/x-")
        || mime_type.starts_with("application/x-")
        || matches!(
            mime_type,
            "text/javascript"
                | "text/javascript-jsx"
                | "text/typescript"
                | "text/typescript-jsx"
                | "text/css"
                | "application/json"
                | "application/xml"
                | "application/graphql"
        )
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

    /// Every extension the catalogue calls indexable must reach an extractor;
    /// audio is transcribed by the adapter instead.
    #[tokio::test]
    async fn every_indexable_extension_in_the_catalogue_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let mut unsupported = Vec::new();
        for format in FileFormat::all() {
            if format.route.is_none() || format.route == Some(ExtractionRoute::Audio) {
                continue;
            }
            let path = dir.path().join(format!("sample.{}", format.extension));
            std::fs::write(&path, "hello world\n").unwrap();
            if let Err(IndexingError::UnsupportedFileType { .. }) =
                ContentExtractor::new().extract_from_file(&path).await
            {
                unsupported.push(format.extension);
            }
        }
        assert!(
            unsupported.is_empty(),
            "offered but rejected: {unsupported:?}"
        );
    }

    #[tokio::test]
    async fn a_known_type_without_a_route_is_rejected_with_its_mime_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.lua");
        std::fs::write(&path, "print('hi')").unwrap();
        let error = ContentExtractor::new().extract_from_file(&path).await.err();
        assert!(
            matches!(
                &error,
                Some(IndexingError::UnsupportedFileType { detected_type, .. })
                    if detected_type == "text/x-lua"
            ),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn default_extractor_has_no_fifty_mib_file_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.txt");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(60 * 1024 * 1024).unwrap();

        assert!(
            check_file_size(&path, ContentExtractor::new().max_file_size)
                .await
                .is_ok()
        );
        assert!(matches!(
            check_file_size(
                &path,
                ContentExtractor::with_max_size(50 * 1024 * 1024).max_file_size
            )
            .await,
            Err(IndexingError::FileTooLarge { .. })
        ));
    }
}
