//! CSV and TSV file extraction.

use super::types::{ContentMetadata, ExtractedContent};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::path::Path;
use tokio::io::{AsyncReadExt, BufReader};

/// Extract content from CSV or TSV files.
pub async fn extract_csv_file(
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
    let mut csv_content = String::new();
    reader
        .read_to_string(&mut csv_content)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let delimiter = if mime_type == "text/tab-separated-values" {
        '\t'
    } else {
        ','
    };

    let processed_text = format_csv_for_search(&csv_content, delimiter);

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
    })
}

/// Format CSV content for better searchability.
///
/// Converts CSV rows into "header: value" format for better semantic search.
fn format_csv_for_search(csv: &str, delimiter: char) -> String {
    let lines: Vec<&str> = csv.lines().collect();

    if lines.is_empty() {
        return String::new();
    }

    let headers: Vec<&str> = lines
        .first()
        .map(|line| line.split(delimiter).map(|s| s.trim()).collect())
        .unwrap_or_default();

    if headers.is_empty() {
        return csv.to_string();
    }

    let mut result = Vec::new();

    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }

        let values: Vec<&str> = line.split(delimiter).map(|s| s.trim()).collect();

        let mut row_parts = Vec::new();
        for (i, value) in values.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                if !value.is_empty() {
                    row_parts.push(format!("{}: {}", header, value));
                }
            }
        }

        if !row_parts.is_empty() {
            result.push(row_parts.join(", "));
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_csv_extraction() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Name,Age,City").unwrap();
        writeln!(temp_file, "Alice,30,NYC").unwrap();
        writeln!(temp_file, "Bob,25,SF").unwrap();

        let path = temp_file.path().with_extension("csv");
        std::fs::copy(temp_file.path(), &path).unwrap();

        let result = extract_csv_file(&path, "text/csv", 50 * 1024 * 1024).await;

        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.text.contains("Name: Alice"));
        assert!(content.text.contains("Age: 30"));
        assert!(content.text.contains("City: NYC"));
        assert_eq!(content.mime_type, "text/csv");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_csv_formatting() {
        let csv = "Name,Age\nAlice,30\nBob,25";
        let formatted = format_csv_for_search(csv, ',');

        assert!(formatted.contains("Name: Alice"));
        assert!(formatted.contains("Age: 30"));
        assert!(formatted.contains("Name: Bob"));
    }
}
