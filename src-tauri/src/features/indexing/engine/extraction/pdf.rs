//! PDF document extraction with page tracking.
//!
//! Text comes from [`super::pdf_layout`], which walks the content-stream
//! operators so headings and tables survive as markdown for the chunker. A
//! page that the walk cannot read falls back to lopdf's plain extraction.
//!
//! Pages that carry an image but no text are scans. They are reported per page
//! in [`ExtractedContent::needs_ocr`] instead of failing the whole document,
//! and are handed to an [`OcrPort`] when one is configured.

use super::pdf_layout;
use super::types::{ContentMetadata, ExtractedContent};
use crate::application::ports::{OcrError, OcrPort};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, warn};

/// Type alias for page range (page_number, start_char, end_char).
pub type PageRange = (usize, usize, usize);

/// Below this many alphanumeric characters outside its running header and
/// footer, a page is not carrying text: whatever it says is in the image.
const MIN_PAGE_ALPHANUMERICS: usize = 20;

/// What a page contributes to the document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PageText {
    /// One-based physical page number.
    page: u32,
    text: String,
    /// The page draws an image and yielded almost no text: a scan.
    needs_ocr: bool,
}

/// Extract content from PDF files with page tracking.
///
/// `ocr` is consulted only for pages that came back textless. Without a
/// provider — or with one that has no model — those pages stay in
/// `needs_ocr` and the rest of the document is still indexed.
pub async fn extract_pdf(
    path: &Path,
    max_file_size: u64,
    ocr: Option<&Arc<dyn OcrPort>>,
) -> Result<ExtractedContent> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let file_size = metadata.len();

    if file_size > max_file_size {
        return Err(IndexingError::FileTooLarge {
            path: path.display().to_string(),
            size_bytes: file_size,
            max_size_bytes: max_file_size,
        });
    }

    let path_clone = path.to_path_buf();

    let mut pages = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || extract_pdf_pages(&path_clone)),
    )
    .await
    .map_err(|_| IndexingError::Other("PDF extraction timed out".to_string()))?
    .map_err(|e| IndexingError::Other(format!("Task join error: {}", e)))??;

    if let Some(ocr) = ocr {
        recognize_scanned_pages(path, &mut pages, ocr.as_ref()).await;
    }

    let (text, page_ranges) = assemble(&pages);

    if is_unreadable(&text, &pages) {
        return Err(IndexingError::ContentExtraction {
            path: path.display().to_string(),
            reason: "No selectable text found. This PDF may need OCR before it can be indexed."
                .into(),
        });
    }

    let needs_ocr: Vec<u32> = pages
        .iter()
        .filter(|page| page.needs_ocr)
        .map(|page| page.page)
        .collect();

    if !needs_ocr.is_empty() {
        warn!(
            path = %path.display(),
            pages = ?needs_ocr,
            "PDF pages have no selectable text and were left unindexed pending OCR"
        );
    }

    let metadata = ContentMetadata {
        page_count: Some(page_ranges.len()),
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/pdf".to_string(),
        metadata,
        page_ranges,
        needs_ocr,
    })
}

/// Read every page, structure it, and mark the scans.
fn extract_pdf_pages(path: &Path) -> Result<Vec<PageText>> {
    use lopdf::Document;

    let doc = Document::load(path).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to load PDF: {}", e),
    })?;

    let page_ids = doc.get_pages();
    let layouts = collect_pdf_pages(path, page_ids.keys().copied(), |page| {
        let Some(page_id) = page_ids.get(&page).copied() else {
            return Err(IndexingError::Other(format!("Page {page} has no object")));
        };
        pdf_layout::read_page(&doc, page, page_id, |reason| {
            warn!(
                path = %path.display(),
                page,
                reason,
                "Layout-aware PDF extraction fell back to plain text for this page"
            );
        })
        .map_err(IndexingError::Other)
    })?;

    Ok(pdf_layout::render(&layouts)
        .into_iter()
        .zip(layouts.iter())
        .map(|(text, layout)| PageText {
            page: layout.page,
            needs_ocr: layout.has_image && body_alphanumerics(&text) < MIN_PAGE_ALPHANUMERICS,
            text,
        })
        .collect())
}

/// Alphanumerics in a page's body, ignoring its first and last line.
///
/// A scanned page still carries a running header and a folio number, which are
/// live text on top of the image. Counting them would push a scan over any
/// sensible threshold — a chapter page of a scanned manual can reach fifty
/// characters of header alone — and the page would be silently indexed as if
/// the scan were not there.
fn body_alphanumerics(text: &str) -> usize {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let body = lines.len().saturating_sub(1);
    lines
        .into_iter()
        .take(body)
        .skip(1)
        .flat_map(str::chars)
        .filter(|c| c.is_alphanumeric())
        .count()
}

/// Run each page through `extract`, refusing to return a partial document.
fn collect_pdf_pages<T>(
    path: &Path,
    pages: impl Iterator<Item = u32>,
    mut extract: impl FnMut(u32) -> Result<T>,
) -> Result<Vec<T>> {
    pages
        .map(|page_num| {
            // A missing page must fail the import, not become an empty passage in
            // a document that the rest of the application treats as fully indexed.
            extract(page_num).map_err(|error| IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!(
                    "Could not read page {page_num}: {error}. The PDF was not indexed."
                ),
            })
        })
        .collect()
}

/// Join the pages into one document, recording each page's byte range.
///
/// The ranges are contiguous and cover the text exactly — `embedding_input`
/// rejects an extraction whose pages leave a gap, because a gap means a
/// citation could point at text from the wrong page.
fn assemble(pages: &[PageText]) -> (String, Vec<PageRange>) {
    let mut text = String::new();
    let mut page_ranges = Vec::new();
    for page in pages {
        let start_pos = text.len();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&page.text);
        page_ranges.push((page.page as usize, start_pos, text.len()));
    }
    (text, page_ranges)
}

/// A document is unreadable when it has no text at all, or when every page is
/// a scan nothing recognized. Anything less than that is indexed: one scanned
/// insert in a 400-page manual must not cost the other 399 pages.
fn is_unreadable(text: &str, pages: &[PageText]) -> bool {
    text.trim().is_empty() || (!pages.is_empty() && pages.iter().all(|page| page.needs_ocr))
}

/// Ask the OCR provider for each scanned page, in page order.
///
/// Failures are never fatal: the page keeps its `needs_ocr` flag and the
/// document is indexed without it. A provider with no model answers
/// [`OcrError::NotConfigured`] once and is not asked again.
async fn recognize_scanned_pages(path: &Path, pages: &mut [PageText], ocr: &dyn OcrPort) {
    for page in pages.iter_mut().filter(|page| page.needs_ocr) {
        match ocr.recognize_page(path, page.page).await {
            Ok(recognized) if !recognized.trim().is_empty() => {
                page.text = splice(&page.text, recognized.trim());
                page.needs_ocr = false;
            }
            Ok(_) => debug!(page = page.page, "OCR recognized no text on this page"),
            Err(OcrError::NotConfigured) => {
                debug!(
                    path = %path.display(),
                    "No OCR model configured; scanned pages stay unindexed"
                );
                return;
            }
            Err(error) => warn!(
                path = %path.display(),
                page = page.page,
                error = %error,
                "OCR failed for this page"
            ),
        }
    }
}

/// Keep whatever little text the page did have — a folio number is still
/// evidence — and put the recognized text after it.
fn splice(existing: &str, recognized: &str) -> String {
    if existing.trim().is_empty() {
        return recognized.to_string();
    }
    format!("{existing}\n{recognized}")
}

#[cfg(test)]
#[path = "pdf_tests.rs"]
mod tests;
