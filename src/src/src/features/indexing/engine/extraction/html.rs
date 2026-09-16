//! HTML and code file extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::path::Path;
use tokio::io::{AsyncReadExt, BufReader};

/// Extract content from code files (including HTML).
pub async fn extract_code_file(
    path: &Path,
    mime_type: &str,
    max_file_size: u64,
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

    let processed_text = if mime_type == "text/html" {
        strip_html_tags(&text)
    } else {
        text
    };

    let metadata = ContentMetadata {
        page_count: None,
        word_count: processed_text.split_whitespace().count(),
        char_count: processed_text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text: processed_text,
        mime_type: mime_type.to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

/// Strip HTML tags from text, preserving content.
pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script_or_style = false;

    let lowercase = html.to_lowercase();

    for (i, ch) in html.chars().enumerate() {
        match ch {
            '<' => {
                in_tag = true;

                if lowercase[i..].starts_with("<script") || lowercase[i..].starts_with("<style") {
                    in_script_or_style = true;
                }
            }
            '>' => {
                in_tag = false;

                if in_script_or_style
                    && (lowercase[..i].ends_with("</script") || lowercase[..i].ends_with("</style"))
                {
                    in_script_or_style = false;
                }
            }
            _ => {
                if !in_tag && !in_script_or_style {
                    result.push(ch);
                }
            }
        }
    }

    result
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
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

        let result = extract_code_file(&path, "text/html", 50 * 1024 * 1024).await;

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

    #[test]
    fn test_html_tag_stripping() {
        let html = "<html><body><h1>Title</h1><p>Content</p><script>ignore</script></body></html>";
        let text = strip_html_tags(html);

        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
        assert!(!text.contains("<h1>"));
        assert!(!text.contains("ignore"));
    }
}
