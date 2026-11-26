//! Plain text and markdown file extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use crate::infrastructure::extraction::markup::decode_text_bytes;
use std::path::Path;

/// Extract content from plain text or markdown files.
pub async fn extract_text_file(
    path: &Path,
    max_file_size: Option<u64>,
) -> Result<ExtractedContent> {
    super::check_file_size(path, max_file_size).await?;

    let bytes = tokio::fs::read(path).await.map_err(|e| IndexingError::Io {
        message: e.to_string(),
        kind: format!("{:?}", e.kind()),
    })?;
    let text = decode_text_bytes(bytes, path);

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    let mime_type = if matches!(extension.as_deref(), Some("md" | "markdown")) {
        "text/markdown".to_string()
    } else {
        "text/plain".to_string()
    };

    Ok(ExtractedContent {
        text,
        mime_type,
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
        ocr_pages: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_text_file_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Test content").unwrap();
        writeln!(temp_file, "Second line").unwrap();

        let result = extract_text_file(temp_file.path(), None).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("Test content"));
        assert!(content.text.contains("Second line"));
        assert_eq!(content.metadata.word_count, 4);
    }

    #[tokio::test]
    async fn a_windows_1252_file_is_read_instead_of_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.txt");
        std::fs::write(&path, b"R\xE9sum\xE9 \x96 2019\n").unwrap();
        let content = extract_text_file(&path, None).await.unwrap();
        assert_eq!(content.text, "R\u{e9}sum\u{e9} \u{2013} 2019\n");
    }
}
