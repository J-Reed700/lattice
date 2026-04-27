//! Word document (DOCX) extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from DOCX files.
pub async fn extract_docx(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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

    let text = tokio::task::spawn_blocking(move || extract_docx_sync(&path_clone))
        .await
        .map_err(|e| IndexingError::Other(format!("Task join error: {}", e)))??;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            .to_string(),
        metadata,
        page_ranges: vec![],
    })
}

/// Extract DOCX content synchronously (used in blocking task).
fn extract_docx_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open DOCX as ZIP: {}", e),
    })?;

    let mut document_xml =
        archive
            .by_name("word/document.xml")
            .map_err(|e| IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("Failed to find document.xml in DOCX: {}", e),
            })?;

    let mut xml_content = String::new();
    document_xml.read_to_string(&mut xml_content).map_err(|e| {
        IndexingError::ContentExtraction {
            path: path.display().to_string(),
            reason: format!("Failed to read document.xml: {}", e),
        }
    })?;

    let text = extract_text_from_docx_xml(&xml_content);

    Ok(text)
}

/// Extract text from DOCX XML content.
fn extract_text_from_docx_xml(xml: &str) -> String {
    let mut text_parts = Vec::new();
    let mut in_text_tag = false;
    let mut current_text = String::new();

    for line in xml.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<w:t") {
            in_text_tag = true;
            if let Some(start) = trimmed.find(">") {
                let after_tag = &trimmed[start + 1..];
                if let Some(end) = after_tag.find("</w:t>") {
                    current_text.push_str(&after_tag[..end]);
                    in_text_tag = false;
                } else {
                    current_text.push_str(after_tag);
                }
            }
        } else if in_text_tag {
            if let Some(end) = trimmed.find("</w:t>") {
                current_text.push_str(&trimmed[..end]);
                in_text_tag = false;
            } else {
                current_text.push_str(trimmed);
            }
        }

        if !in_text_tag && !current_text.is_empty() {
            text_parts.push(decode_xml_entities(&current_text));
            current_text.clear();
        }

        if trimmed.contains("</w:p>") && !text_parts.is_empty() {
            text_parts.push("\n".to_string());
        }
    }

    text_parts.join("")
}

/// Decode XML entities in text.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
