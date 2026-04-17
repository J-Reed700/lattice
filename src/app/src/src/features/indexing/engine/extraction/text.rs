//! Plain text and markdown file extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::path::Path;
use tokio::io::{AsyncReadExt, BufReader};

/// Extract content from plain text or markdown files.
pub async fn extract_text_file(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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

    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let mut reader = BufReader::new(file);
    let mut text = String::new();
    reader
        .read_to_string(&mut text)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    let mime_type = if path.extension().and_then(|s| s.to_str()) == Some("md") {
        "text/markdown".to_string()
    } else {
        "text/plain".to_string()
    };

    Ok(ExtractedContent {
        text,
        mime_type,
        metadata,
        page_ranges: vec![],
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

        let result = extract_text_file(temp_file.path(), 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("Test content"));
        assert!(content.text.contains("Second line"));
        assert_eq!(content.metadata.word_count, 4);
    }
}
