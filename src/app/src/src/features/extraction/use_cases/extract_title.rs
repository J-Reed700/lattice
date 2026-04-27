//! Extract Document Title Use Case
//!
//! Extracts the title from markdown document content using multiple strategies.
//!
//! # Strategy Priority
//! 1. H1 header (`# Title`)
//! 2. YAML frontmatter (`title: ...`)
//! 3. First non-empty line
//!
//! # Example
//! ```rust,no_run
//! let use_case = ExtractDocumentTitleUseCase::new();
//! let request = ExtractTitleRequestDto {
//!     content: "# My Document\n\nContent here...".into(),
//! };
//! let result = use_case.execute(request).await?;
//! assert_eq!(result.title, Some("My Document".into()));
//! assert_eq!(result.strategy, TitleExtractionStrategy::H1Header);
//! ```

use crate::features::extraction::dto::{
    ExtractTitleRequestDto, ExtractTitleResponseDto, TitleExtractionStrategy,
};
use crate::infrastructure::extraction::LinkParser;
use crate::shared::error::AppError;

pub struct ExtractDocumentTitleUseCase {
    link_parser: LinkParser,
}

impl ExtractDocumentTitleUseCase {
    pub fn new() -> Self {
        Self {
            link_parser: LinkParser::new(),
        }
    }

    pub async fn execute(
        &self,
        request: ExtractTitleRequestDto,
    ) -> Result<ExtractTitleResponseDto, AppError> {
        tracing::debug!(
            content_length = request.content.len(),
            "Extracting document title"
        );

        let title_option = self.link_parser.extract_title(&request.content);

        let (title, strategy) = if let Some(title) = title_option {
            let strategy = self.determine_strategy(&request.content, &title);
            (Some(title), strategy)
        } else {
            (None, TitleExtractionStrategy::FirstLine)
        };

        tracing::info!(
            title = ?title,
            strategy = ?strategy,
            "Successfully extracted document title"
        );

        Ok(ExtractTitleResponseDto { title, strategy })
    }

    fn determine_strategy(&self, content: &str, title: &str) -> TitleExtractionStrategy {
        if content.starts_with("---") && content.contains("title:") {
            TitleExtractionStrategy::Frontmatter
        } else if content.contains(&format!("# {}", title)) {
            TitleExtractionStrategy::H1Header
        } else {
            TitleExtractionStrategy::FirstLine
        }
    }
}

impl Default for ExtractDocumentTitleUseCase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_title_from_h1() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "# My Note\n\nContent here...".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("My Note".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::H1Header);
    }

    #[tokio::test]
    async fn test_extract_title_from_frontmatter() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "---\ntitle: My Note\n---\n\nContent...".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("My Note".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::Frontmatter);
    }

    #[tokio::test]
    async fn test_extract_title_from_frontmatter_with_quotes() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "---\ntitle: \"My Note\"\n---\n\nContent...".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("My Note".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::Frontmatter);
    }

    #[tokio::test]
    async fn test_extract_title_from_first_line() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "My Note\n\nContent here...".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("My Note".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::FirstLine);
    }

    #[tokio::test]
    async fn test_extract_title_empty_content() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto { content: "".into() };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, None);
    }

    #[tokio::test]
    async fn test_extract_title_h1_priority_over_frontmatter() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "---\ntitle: Frontmatter Title\n---\n\n# H1 Title\n\nContent...".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("Frontmatter Title".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::Frontmatter);
    }

    #[tokio::test]
    async fn test_extract_title_ignores_code_blocks() {
        let use_case = ExtractDocumentTitleUseCase::new();
        let request = ExtractTitleRequestDto {
            content: "```python\ncode\n```\n\n# Real Title".into(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.title, Some("Real Title".to_string()));
        assert_eq!(result.strategy, TitleExtractionStrategy::H1Header);
    }
}
