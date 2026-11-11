//! HTML and code file extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use crate::infrastructure::extraction::html_text::html_to_text;
use crate::infrastructure::extraction::markup::decode_text_bytes;
use std::path::Path;

/// Extract content from source, config and other text files as written.
pub async fn extract_code_file(
    path: &Path,
    mime_type: &str,
    max_file_size: u64,
) -> Result<ExtractedContent> {
    let text = read_text(path, max_file_size).await?;
    Ok(content_of(text, mime_type))
}

/// Extract the visible text of an HTML file.
pub async fn extract_html_file(
    path: &Path,
    mime_type: &str,
    max_file_size: u64,
) -> Result<ExtractedContent> {
    let text = read_text(path, max_file_size).await?;
    Ok(content_of(html_to_text(&text), mime_type))
}

async fn read_text(path: &Path, max_file_size: u64) -> Result<String> {
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

    let bytes = tokio::fs::read(path).await.map_err(|e| IndexingError::Io {
        message: e.to_string(),
        kind: format!("{:?}", e.kind()),
    })?;
    Ok(decode_text_bytes(bytes, path))
}

fn content_of(text: String, mime_type: &str) -> ExtractedContent {
    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    ExtractedContent {
        text,
        mime_type: mime_type.to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
        ocr_pages: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_code_file_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "fn main() {{").unwrap();
        writeln!(temp_file, "    println!(\"Hello, world!\");").unwrap();
        writeln!(temp_file, "}}").unwrap();

        let path = temp_file.path().with_extension("rs");
        std::fs::copy(temp_file.path(), &path).unwrap();

        let result = extract_code_file(&path, "text/x-rust", 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("fn main"));
        assert!(content.text.contains("println"));
        assert_eq!(content.mime_type, "text/x-rust");

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_html_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "<html><head><title>Test</title></head>").unwrap();
        writeln!(temp_file, "<body><h1>Hello World</h1>").unwrap();
        writeln!(temp_file, "<p>This is a test.</p>").unwrap();
        writeln!(temp_file, "<script>console.log('ignore');</script>").unwrap();
        writeln!(temp_file, "</body></html>").unwrap();

        let path = temp_file.path().with_extension("html");
        std::fs::copy(temp_file.path(), &path).unwrap();

        let result = extract_html_file(&path, "text/html", 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("Test"));
        assert!(content.text.contains("Hello World"));
        assert!(content.text.contains("This is a test"));
        assert!(!content.text.contains("<html>"));
        assert!(!content.text.contains("console.log"));
        assert_eq!(content.mime_type, "text/html");

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn a_windows_1252_file_is_read_instead_of_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.html");
        std::fs::write(&path, b"<p>caf\xE9</p>").unwrap();
        let content = extract_html_file(&path, "text/html", 1 << 20)
            .await
            .unwrap();
        assert_eq!(content.text, "caf\u{e9}");
    }
}
