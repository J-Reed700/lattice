//! Parse Wikilinks Use Case
//!
//! Extracts wikilinks from markdown content using the LinkParser infrastructure.
//!
//! # Dependencies
//! - Uses `LinkParser` from infrastructure layer
//!
//! # Example
//! ```rust,no_run
//! let use_case = ParseWikilinksUseCase::new();
//! let request = ParseWikilinksRequestDto {
//!     text: "See [[note]] and [[doc#section|display]]".into(),
//!     source_path: Some("index.md".into()),
//! };
//! let result = use_case.execute(request).await?;
//! println!("Found {} links", result.links.len());
//! ```

use crate::application::dtos::{ParseWikilinksRequestDto, ParseWikilinksResponseDto, WikiLinkDto};
use crate::infrastructure::extraction::LinkParser;
use crate::shared::error::AppError;

pub struct ParseWikilinksUseCase {
    link_parser: LinkParser,
}

impl ParseWikilinksUseCase {
    pub fn new() -> Self {
        Self {
            link_parser: LinkParser::new(),
        }
    }

    pub async fn execute(
        &self,
        request: ParseWikilinksRequestDto,
    ) -> Result<ParseWikilinksResponseDto, AppError> {
        tracing::debug!(
            text_length = request.text.len(),
            source_path = ?request.source_path,
            "Parsing wikilinks from content"
        );

        let source_path = request.source_path.as_deref().unwrap_or("");
        let wikilinks = self.link_parser.parse_document(&request.text, source_path);

        let links: Vec<WikiLinkDto> = wikilinks
            .into_iter()
            .map(|link| WikiLinkDto {
                target: link.target,
                display_text: link.display_text,
                header: link.header,
                line_number: link.line_number,
            })
            .collect();

        tracing::info!(links_found = links.len(), "Successfully parsed wikilinks");

        Ok(ParseWikilinksResponseDto { links })
    }
}

impl Default for ParseWikilinksUseCase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_simple_link() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "See [[todo]] for tasks.".into(),
            source_path: Some("index.md".into()),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].target, "todo");
        assert_eq!(result.links[0].display_text, None);
        assert_eq!(result.links[0].header, None);
        assert_eq!(result.links[0].line_number, 1);
    }

    #[tokio::test]
    async fn test_parse_link_with_display() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "Check [[projects/ml|Machine Learning]]".into(),
            source_path: None,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].target, "projects/ml");
        assert_eq!(
            result.links[0].display_text,
            Some("Machine Learning".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_link_with_header() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "See [[doc#section]]".into(),
            source_path: Some("test.md".into()),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].target, "doc");
        assert_eq!(result.links[0].header, Some("section".to_string()));
    }

    #[tokio::test]
    async fn test_parse_link_with_header_and_display() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "[[projects/ml#architecture|ML Architecture]]".into(),
            source_path: None,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 1);
        assert_eq!(result.links[0].target, "projects/ml");
        assert_eq!(result.links[0].header, Some("architecture".to_string()));
        assert_eq!(
            result.links[0].display_text,
            Some("ML Architecture".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_multiple_links() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "See [[todo]] and [[projects/ml|ML]]".into(),
            source_path: Some("index.md".into()),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 2);
        assert_eq!(result.links[0].target, "todo");
        assert_eq!(result.links[1].target, "projects/ml");
    }

    #[tokio::test]
    async fn test_parse_no_links() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "No links in this text".into(),
            source_path: None,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 0);
    }

    #[tokio::test]
    async fn test_parse_line_numbers() {
        let use_case = ParseWikilinksUseCase::new();
        let request = ParseWikilinksRequestDto {
            text: "Line 1\nLine 2 [[link1]]\nLine 3\nLine 4 [[link2]]".into(),
            source_path: Some("test.md".into()),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.links.len(), 2);
        assert_eq!(result.links[0].line_number, 2);
        assert_eq!(result.links[1].line_number, 4);
    }
}
