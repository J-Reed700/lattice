//! Rich Text Format (RTF) extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::path::Path;
use tokio::io::{AsyncReadExt, BufReader};

/// Extract content from RTF files.
pub async fn extract_rtf(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let raw_text = String::from_utf8_lossy(&bytes);
    let text = strip_rtf(&raw_text);

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/rtf".to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

fn strip_rtf(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' | '}' => {
                // Skip group delimiters
            }
            '\\' => {
                match chars.peek() {
                    Some('\\') | Some('{') | Some('}') => {
                        if let Some(escaped) = chars.next() {
                            output.push(escaped);
                        }
                    }
                    Some('\'') => {
                        chars.next();
                        let hex1 = chars.next();
                        let hex2 = chars.next();
                        if let (Some(h1), Some(h2)) = (hex1, hex2) {
                            if let Ok(byte) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                                output.push(byte as char);
                            }
                        }
                    }
                    Some(_) => {
                        let mut control = String::new();
                        while let Some(&c) = chars.peek() {
                            if c.is_alphabetic() {
                                control.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        // Optional numeric parameter
                        while let Some(&c) = chars.peek() {
                            if c == '-' || c.is_ascii_digit() {
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        if matches!(control.as_str(), "par" | "line") {
                            output.push('\n');
                        }

                        if matches!(chars.peek(), Some(' ')) {
                            chars.next();
                        }
                    }
                    None => {}
                }
            }
            _ => output.push(ch),
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_rtf_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            "{{\\rtf1\\ansi This is \\b bold\\b0 text.\\par Next line.}}"
        )
        .unwrap();

        let result = extract_rtf(temp_file.path(), 50 * 1024 * 1024).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("This is"));
        assert!(content.text.contains("bold"));
        assert!(content.text.contains("Next line"));
    }
}
