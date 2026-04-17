//! # Clean Article Content Use Case
//!
//! Extracts clean article content from raw HTML.
//!
//! This use case orchestrates:
//! 1. HTML parsing
//! 2. Readability algorithm application
//! 3. Boilerplate removal (ads, navigation, sidebars, footers)
//! 4. Main content extraction
//! 5. Metadata extraction (title, author)
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::web::clean_article_content::CleanArticleContentUseCase;
//! use vault_desktop::application::dtos::web_dto::CleanArticleRequestDto;
//!
//! # async fn example(use_case: CleanArticleContentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = CleanArticleRequestDto {
//!     html: "<html><body><article>...</article></body></html>".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Title: {:?}", response.title);
//! println!("Word count: {}", response.word_count);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::web::dto::{CleanArticleRequestDto, CleanArticleResponseDto};
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::shared::error::{AppError, Result};

/// Clean article content use case.
///
/// Applies readability algorithm to extract clean article content:
/// - Removes ads, navigation, boilerplate
/// - Extracts main content
/// - Provides plain text + metadata
///
/// ## Dependencies
///
/// - `ArticleExtractorServiceTrait`: Handles HTML parsing and article extraction
pub struct CleanArticleContentUseCase {
    article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
}

impl CleanArticleContentUseCase {
    /// Create a new clean article content use case.
    ///
    /// # Arguments
    ///
    /// * `article_extractor` - Service for article content extraction
    pub fn new(article_extractor: Arc<dyn ArticleExtractorServiceTrait>) -> Self {
        Self { article_extractor }
    }

    /// Execute article cleaning.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the raw HTML to clean
    ///
    /// # Returns
    ///
    /// Cleaned article with content, title, author, and word count
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - HTML is empty or malformed
    /// - Article content cannot be extracted
    /// - HTML parsing fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::web::clean_article_content::CleanArticleContentUseCase;
    /// # use vault_desktop::application::dtos::web_dto::CleanArticleRequestDto;
    /// # async fn example(use_case: CleanArticleContentUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let html = r#"
    /// <html>
    ///   <body>
    ///     <nav>Navigation...</nav>
    ///     <article>
    ///       <h1>Article Title</h1>
    ///       <p>This is the main article content...</p>
    ///     </article>
    ///     <aside>Sidebar ads...</aside>
    ///   </body>
    /// </html>
    /// "#;
    ///
    /// let request = CleanArticleRequestDto {
    ///     html: html.to_string(),
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(cleaned) => {
    ///         println!("Title: {:?}", cleaned.title);
    ///         println!("Author: {:?}", cleaned.author);
    ///         println!("Word count: {}", cleaned.word_count);
    ///         println!("Content preview: {}...", &cleaned.content[..100.min(cleaned.content.len())]);
    ///     }
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: CleanArticleRequestDto,
    ) -> Result<CleanArticleResponseDto> {
        // 1. Validate HTML is not empty
        if request.html.trim().is_empty() {
            return Err(AppError::InvalidInput("HTML cannot be empty".to_string()));
        }

        // 2. Delegate to article extractor service
        // (Service handles: parsing, readability, boilerplate removal)
        // Note: We use a placeholder URL since we're working with raw HTML
        let article = self
            .article_extractor
            .extract_article(&request.html, "about:blank")
            .await?;

        // 3. Convert service result to DTO
        Ok(CleanArticleResponseDto {
            content: article.content,
            title: Some(article.title),
            author: article.author,
            word_count: article.word_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::function_calling::dto::CleanArticle;
    use crate::infrastructure::services::traits::MockArticleExtractorService;

    #[tokio::test]
    async fn test_clean_article_with_full_metadata() {
        // Arrange
        let mock_service = Arc::new(MockArticleExtractorService::new());
        let expected = CleanArticle {
            title: "Test Article Title".to_string(),
            author: Some("John Doe".to_string()),
            content: "<p>Clean article content goes here.</p>".to_string(),
            text_content: "Clean article content goes here.".to_string(),
            word_count: 5,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Clean article content goes here.".to_string()),
        };
        mock_service.set_response("about:blank", expected.clone());

        let use_case = CleanArticleContentUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: "<html><body><article><h1>Test</h1><p>Content</p></article></body></html>"
                    .to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let cleaned = result.unwrap();
        assert_eq!(cleaned.title, Some("Test Article Title".to_string()));
        assert_eq!(cleaned.author, Some("John Doe".to_string()));
        assert_eq!(cleaned.content, "<p>Clean article content goes here.</p>");
        assert_eq!(cleaned.word_count, 5);
    }

    #[tokio::test]
    async fn test_clean_article_without_author() {
        // Arrange
        let mock_service = Arc::new(MockArticleExtractorService::new());
        let expected = CleanArticle {
            title: "Article Without Author".to_string(),
            author: None,
            content: "<p>Content here.</p>".to_string(),
            text_content: "Content here.".to_string(),
            word_count: 2,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Content here.".to_string()),
        };
        mock_service.set_response("about:blank", expected);

        let use_case = CleanArticleContentUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: "<html><body><p>Content here.</p></body></html>".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let cleaned = result.unwrap();
        assert_eq!(cleaned.title, Some("Article Without Author".to_string()));
        assert_eq!(cleaned.author, None);
    }

    #[tokio::test]
    async fn test_clean_empty_html_returns_error() {
        // Arrange
        let mock_service = Arc::new(MockArticleExtractorService::new());
        let use_case = CleanArticleContentUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: "".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_clean_whitespace_only_html_returns_error() {
        // Arrange
        let mock_service = Arc::new(MockArticleExtractorService::new());
        let use_case = CleanArticleContentUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: "   \n\t   ".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_clean_complex_html_with_boilerplate() {
        // Arrange
        let mock_service = Arc::new(MockArticleExtractorService::new());
        let expected = CleanArticle {
            title: "Complex Article".to_string(),
            author: Some("Jane Smith".to_string()),
            content: "<h1>Complex Article</h1><p>Main content without ads or navigation.</p>"
                .to_string(),
            text_content: "Complex Article Main content without ads or navigation.".to_string(),
            word_count: 7,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Complex Article Main content without...".to_string()),
        };
        mock_service.set_response("about:blank", expected);

        let use_case = CleanArticleContentUseCase::new(mock_service);

        let html = r#"
        <html>
          <head><title>Complex Article</title></head>
          <body>
            <nav>
              <a href="/">Home</a>
              <a href="/about">About</a>
            </nav>
            <aside class="ads">
              <div>Advertisement</div>
            </aside>
            <article>
              <h1>Complex Article</h1>
              <p>Main content without ads or navigation.</p>
            </article>
            <footer>
              <p>Copyright 2024</p>
            </footer>
          </body>
        </html>
        "#;

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: html.to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let cleaned = result.unwrap();
        assert_eq!(cleaned.title, Some("Complex Article".to_string()));
        assert_eq!(cleaned.word_count, 7);
        assert!(!cleaned.content.contains("Advertisement"));
        assert!(!cleaned.content.contains("Copyright 2024"));
    }

    #[tokio::test]
    async fn test_clean_extraction_error_propagates() {
        // Arrange
        use async_trait::async_trait;

        struct FailingExtractor;

        #[async_trait]
        impl ArticleExtractorServiceTrait for FailingExtractor {
            async fn extract_article(&self, _html: &str, _url: &str) -> Result<CleanArticle> {
                Err(AppError::Other("Article extraction failed".to_string()))
            }

            async fn extract_article_from_url(&self, _url: &str) -> Result<CleanArticle> {
                Err(AppError::Other("Not implemented".to_string()))
            }
        }

        let use_case = CleanArticleContentUseCase::new(Arc::new(FailingExtractor));

        // Act
        let result = use_case
            .execute(CleanArticleRequestDto {
                html: "<html><body><p>Test</p></body></html>".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Other(msg) => assert_eq!(msg, "Article extraction failed"),
            _ => panic!("Expected Other error"),
        }
    }
}
