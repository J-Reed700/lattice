//! OpenDocument Text (ODT) extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from ODT files.
pub async fn extract_odt(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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
    let text = tokio::task::spawn_blocking(move || extract_odt_sync(&path_clone))
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
        mime_type: "application/vnd.oasis.opendocument.text".to_string(),
        metadata,
        page_ranges: vec![],
    })
}

fn extract_odt_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open ODT as ZIP: {}", e),
    })?;

    let mut content_xml =
        archive
            .by_name("content.xml")
            .map_err(|e| IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("Failed to find content.xml in ODT: {}", e),
            })?;

    let mut xml_content = String::new();
    content_xml
        .read_to_string(&mut xml_content)
        .map_err(|e| IndexingError::ContentExtraction {
            path: path.display().to_string(),
            reason: format!("Failed to read content.xml: {}", e),
        })?;

    Ok(extract_text_from_odt_xml(&xml_content))
}

fn extract_text_from_odt_xml(xml: &str) -> String {
    let normalized = xml
        .replace("</text:p>", "\n")
        .replace("</text:h>", "\n")
        .replace("</text:list-item>", "\n")
        .replace("<text:line-break/>", "\n")
        .replace("<text:line-break />", "\n");

    let stripped = strip_xml_tags(&normalized);
    decode_xml_entities(&stripped)
}

fn strip_xml_tags(input: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    result.push(ch);
                }
            }
        }
    }

    result
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
    use super::decode_xml_entities;

    #[test]
    fn test_odt_entity_decoding() {
        let input = "Hello &amp; goodbye &lt;world&gt;";
        let output = decode_xml_entities(input);
        assert_eq!(output, "Hello & goodbye <world>");
    }
}
