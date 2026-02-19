//! PDF document extraction with page tracking.

use super::types::{ContentMetadata, ExtractedContent};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::path::Path;

/// Type alias for page range (page_number, start_char, end_char).
pub type PageRange = (usize, usize, usize);

/// Type alias for extracted PDF data (text, page_count, page_ranges).
pub type ExtractedPdf = (String, usize, Vec<PageRange>);

/// Extract content from PDF files with page tracking.
pub async fn extract_pdf(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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

    let (text, page_count, page_ranges) = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || extract_pdf_with_pages(&path_clone)),
    )
    .await
    .map_err(|_| IndexingError::Other("PDF extraction timed out".to_string()))?
    .map_err(|e| IndexingError::Other(format!("Task join error: {}", e)))??;

    let metadata = ContentMetadata {
        page_count: Some(page_count),
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/pdf".to_string(),
        metadata,
        page_ranges,
    })
}

/// Extract text from a single PDF page.
fn extract_page_text(doc: &lopdf::Document, page_num: u32) -> Result<String> {
    doc.extract_text(&[page_num])
        .map_err(|e| IndexingError::ContentExtraction {
            path: "pdf".to_string(),
            reason: format!("Failed to extract text from page {}: {}", page_num, e),
        })
}

/// Extract PDF with page range tracking.
fn extract_pdf_with_pages(path: &Path) -> Result<ExtractedPdf> {
    use lopdf::Document;

    let doc = Document::load(path).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to load PDF: {}", e),
    })?;

    let page_count = doc.get_pages().len();
    let mut text = String::new();
    let mut page_ranges = Vec::new();

    for page_num in 1..=page_count as u32 {
        let start_pos = text.len();

        match extract_page_text(&doc, page_num) {
            Ok(page_text) => {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&page_text);
                let end_pos = text.len();
                page_ranges.push((page_num as usize, start_pos, end_pos));
            }
            Err(_) => {
                // Skip pages that fail to extract, but keep track of position
                let end_pos = text.len();
                page_ranges.push((page_num as usize, start_pos, end_pos));
            }
        }
    }

    Ok((text, page_count, page_ranges))
}
