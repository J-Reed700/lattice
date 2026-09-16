//! PowerPoint (PPTX) extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from PPTX files.
pub async fn extract_pptx(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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
    let text = tokio::task::spawn_blocking(move || extract_pptx_sync(&path_clone))
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
        mime_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            .to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

fn extract_pptx_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open PPTX as ZIP: {}", e),
    })?;

    let mut text_parts = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("Failed to read PPTX entry: {}", e),
            })?;
        let name = file.name().to_string();
        if !name.starts_with("ppt/slides/") || !name.ends_with(".xml") {
            continue;
        }

        let mut xml_content = String::new();
        file.read_to_string(&mut xml_content)
            .map_err(|e| IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("Failed to read slide XML: {}", e),
            })?;

        text_parts.extend(extract_text_from_xml_tag(&xml_content, "a:t"));
    }

    Ok(text_parts.join("\n"))
}

fn extract_text_from_xml_tag(xml: &str, tag: &str) -> Vec<String> {
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);
    let mut results = Vec::new();
    let mut cursor = 0;

    while let Some(start_idx) = xml[cursor..].find(&open_tag) {
        let tag_start = cursor + start_idx;
        let after_start = &xml[tag_start..];
        let tag_end_offset = match after_start.find('>') {
            Some(offset) => offset,
            None => break,
        };
        let content_start = tag_start + tag_end_offset + 1;
        let content_slice = &xml[content_start..];
        let end_offset = match content_slice.find(&close_tag) {
            Some(offset) => offset,
            None => break,
        };

        let content = &content_slice[..end_offset];
        if !content.trim().is_empty() {
            results.push(decode_xml_entities(content));
        }

        cursor = content_start + end_offset + close_tag.len();
    }

    results
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::extract_text_from_xml_tag;

    #[test]
    fn test_extract_text_from_xml_tag() {
        let xml = r#"<root><a:t>Hello</a:t><a:t>World</a:t></root>"#;
        let result = extract_text_from_xml_tag(xml, "a:t");
        assert_eq!(result, vec!["Hello".to_string(), "World".to_string()]);
    }
}
