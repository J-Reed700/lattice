use crate::features::indexing::engine::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadata {
    pub title: String,
    pub page_number: Option<usize>,
    pub section: Option<String>,
    pub document_type: String,
}

impl DocumentMetadata {
    pub fn new(title: String, document_type: String) -> Self {
        Self {
            title,
            page_number: None,
            section: None,
            document_type,
        }
    }

    pub fn with_page_number(mut self, page: usize) -> Self {
        self.page_number = Some(page);
        self
    }

    pub fn with_section(mut self, section: String) -> Self {
        self.section = Some(section);
        self
    }
}

pub fn extract_metadata(
    file_path: &Path,
    content: &str,
    _chunk_index: usize,
) -> Result<DocumentMetadata> {
    let title = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let document_type = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_uppercase();

    let section = extract_section_from_content(content);

    Ok(DocumentMetadata {
        title,
        page_number: None,
        section,
        document_type,
    })
}

pub fn extract_metadata_with_page(
    file_path: &Path,
    content: &str,
    chunk_index: usize,
    page_number: Option<usize>,
) -> Result<DocumentMetadata> {
    let mut metadata = extract_metadata(file_path, content, chunk_index)?;
    metadata.page_number = page_number;
    Ok(metadata)
}

fn extract_section_from_content(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().take(10).collect();

    for line in lines {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('#') {
            let section = trimmed.trim_start_matches('#').trim();
            if !section.is_empty() {
                return Some(section.to_string());
            }
        }

        if trimmed.ends_with(':') && trimmed.len() < 100 {
            let section = trimmed.trim_end_matches(':').trim();
            if !section.is_empty() && !section.contains('\n') {
                return Some(section.to_string());
            }
        }

        if trimmed
            .chars()
            .all(|c| c.is_uppercase() || c.is_whitespace() || c.is_numeric())
            && trimmed.len() > 3
            && trimmed.len() < 100
            && trimmed.split_whitespace().count() <= 10
        {
            return Some(trimmed.to_string());
        }
    }

    None
}

pub fn determine_page_number(
    chunk_start_char: usize,
    page_ranges: &[(usize, usize, usize)],
) -> Option<usize> {
    page_ranges
        .iter()
        .find(|(_, start, end)| chunk_start_char >= *start && chunk_start_char < *end)
        .map(|(page, _, _)| *page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metadata_basic() {
        let path = Path::new("test_document.pdf");
        let content = "This is test content";

        let metadata = extract_metadata(path, content, 0).unwrap();

        assert_eq!(metadata.title, "test_document.pdf");
        assert_eq!(metadata.document_type, "PDF");
        assert_eq!(metadata.page_number, None);
    }

    #[test]
    fn test_extract_metadata_with_page() {
        let path = Path::new("document.pdf");
        let content = "Content";

        let metadata = extract_metadata_with_page(path, content, 0, Some(5)).unwrap();

        assert_eq!(metadata.page_number, Some(5));
    }

    #[test]
    fn test_section_extraction_markdown() {
        let content = "# Introduction\nThis is the intro section.";
        let section = extract_section_from_content(content);

        assert_eq!(section, Some("Introduction".to_string()));
    }

    #[test]
    fn test_section_extraction_markdown_multiple_hashes() {
        let content = "## Chapter 1: Getting Started\nContent here.";
        let section = extract_section_from_content(content);

        assert_eq!(section, Some("Chapter 1: Getting Started".to_string()));
    }

    #[test]
    fn test_section_extraction_colon() {
        let content = "Executive Summary:\nThis document provides...";
        let section = extract_section_from_content(content);

        assert_eq!(section, Some("Executive Summary".to_string()));
    }

    #[test]
    fn test_section_extraction_uppercase() {
        let content = "FINANCIAL RESULTS\nThe company achieved...";
        let section = extract_section_from_content(content);

        assert_eq!(section, Some("FINANCIAL RESULTS".to_string()));
    }

    #[test]
    fn test_section_extraction_no_section() {
        let content = "This is just normal text without any section headers.";
        let section = extract_section_from_content(content);

        assert_eq!(section, None);
    }

    #[test]
    fn test_section_extraction_ignores_long_uppercase() {
        let content = "THIS IS A VERY LONG LINE THAT SHOULD NOT BE CONSIDERED A SECTION HEADER BECAUSE IT IS TOO LONG AND PROBABLY JUST SHOUTING";
        let section = extract_section_from_content(content);

        assert_eq!(section, None);
    }

    #[test]
    fn test_determine_page_number() {
        let page_ranges = vec![(1, 0, 100), (2, 100, 250), (3, 250, 400)];

        assert_eq!(determine_page_number(50, &page_ranges), Some(1));
        assert_eq!(determine_page_number(150, &page_ranges), Some(2));
        assert_eq!(determine_page_number(300, &page_ranges), Some(3));
        assert_eq!(determine_page_number(500, &page_ranges), None);
    }

    #[test]
    fn test_metadata_builder_pattern() {
        let metadata = DocumentMetadata::new("test.pdf".to_string(), "PDF".to_string())
            .with_page_number(3)
            .with_section("Introduction".to_string());

        assert_eq!(metadata.page_number, Some(3));
        assert_eq!(metadata.section, Some("Introduction".to_string()));
    }
}
