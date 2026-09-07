//! Article extractor service trait
//!
//! Provides clean article content extraction (reader mode) from web pages.

use crate::features::function_calling::dto::CleanArticle;
use crate::shared::error::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for article extraction operations
///
/// Extracts clean, readable article content from HTML, similar to
/// Firefox Reader Mode or Pocket's article view.
///
/// # Features
///
/// - **Main Content Detection**: Identifies article container via heuristics
/// - **Boilerplate Removal**: Removes ads, navigation, sidebars, footers
/// - **Metadata Extraction**: Extracts author, publication date
/// - **Text Extraction**: Provides both clean HTML and plain text
/// - **Reading Time**: Calculates reading time at 200 WPM
/// - **Excerpt Generation**: Creates summary from first ~200 characters
///
/// # Implementations
/// - `ArticleExtractorService`: Production implementation with scraper
/// - `MockArticleExtractorService`: In-memory mock for testing
#[async_trait]
pub trait ArticleExtractorServiceTrait: Send + Sync {
    /// Extract clean article content from HTML
    ///
    /// Applies readability algorithm to extract main article content:
    /// 1. Finds main content container (semantic tags, class names)
    /// 2. Removes unwanted elements (scripts, ads, navigation)
    /// 3. Cleans up remaining HTML
    /// 4. Extracts plain text for word count
    /// 5. Calculates reading time (word_count / 200 minutes)
    ///
    /// # Arguments
    /// * `html` - The HTML content to extract from
    /// * `url` - The source URL (used for context and metadata extraction)
    ///
    /// # Returns
    /// `CleanArticle` containing:
    /// - `title`: Extracted article title
    /// - `author`: Author name if found
    /// - `content`: Clean HTML content
    /// - `text_content`: Plain text (no HTML)
    /// - `word_count`: Number of words
    /// - `reading_time_minutes`: Estimated minutes to read
    /// - `published_date`: Publication date if found
    /// - `excerpt`: First ~200 characters
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if HTML is empty or malformed
    /// - `AppError::ContentExtraction` if article content cannot be found
    /// - `AppError::Other` if HTML parsing fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::ArticleExtractorServiceTrait;
    ///
    /// # async fn example<S: ArticleExtractorServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let html = r#"<html><body><article><h1>Title</h1><p>Content...</p></article></body></html>"#;
    /// let article = service.extract_article(html, "https://example.com/article").await?;
    /// println!("Title: {}", article.title);
    /// println!("Word count: {}", article.word_count);
    /// println!("Reading time: {} minutes", article.reading_time_minutes);
    /// # Ok(())
    /// # }
    /// ```
    async fn extract_article(&self, html: &str, url: &str) -> Result<CleanArticle>;

    /// Extract article content from a web URL
    ///
    /// Fetches the URL and extracts article content in one operation.
    /// This method handles URL validation (SSRF prevention) and HTTP fetching.
    ///
    /// # Arguments
    /// * `url` - The URL to fetch and extract (must be valid HTTP/HTTPS URL)
    ///
    /// # Returns
    /// `CleanArticle` containing extracted article content
    ///
    /// # Errors
    /// - `AppError::InvalidUrl` if URL validation fails (SSRF prevention)
    /// - `AppError::Network` if HTTP request fails or times out
    /// - `AppError::InvalidInput` if HTML is empty or malformed
    /// - `AppError::ContentExtraction` if article content cannot be found
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::ArticleExtractorServiceTrait;
    ///
    /// # async fn example<S: ArticleExtractorServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let article = service.extract_article_from_url("https://example.com/article").await?;
    /// println!("Title: {}", article.title);
    /// # Ok(())
    /// # }
    /// ```
    async fn extract_article_from_url(&self, url: &str) -> Result<CleanArticle>;
}

/// Mock implementation of ArticleExtractorServiceTrait for testing
///
/// Provides deterministic responses for testing without actual HTML parsing.
/// Configure expected responses using `set_response`.
///
/// # Example
/// ```rust
/// use lattice::infrastructure::services::traits::MockArticleExtractorService;
/// use lattice::application::dtos::function_calling_dto::CleanArticle;
///
/// let mock = MockArticleExtractorService::new();
///
/// // Configure expected response
/// let expected_article = CleanArticle {
///     title: "Test Article".to_string(),
///     author: Some("Test Author".to_string()),
///     content: "<p>Test content</p>".to_string(),
///     text_content: "Test content".to_string(),
///     word_count: 2,
///     reading_time_minutes: 1,
///     published_date: None,
///     excerpt: Some("Test content".to_string()),
/// };
/// mock.set_response("https://example.com", expected_article);
///
/// // Mock will return configured response
/// # tokio_test::block_on(async {
/// let result = mock.extract_article("<html>...</html>", "https://example.com").await.unwrap();
/// assert_eq!(result.title, "Test Article");
/// # });
/// ```
pub struct MockArticleExtractorService {
    /// Configured responses keyed by URL
    responses: Arc<Mutex<HashMap<String, CleanArticle>>>,
}

impl MockArticleExtractorService {
    /// Create a new mock service
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a disabled service that returns errors with a custom message
    ///
    /// Used as a fallback when ArticleExtractorService fails to initialize.
    /// All extraction attempts will return an error with the provided message.
    ///
    /// # Arguments
    /// * `error_message` - The error message to return for all extraction attempts
    ///
    /// # Example
    /// ```rust
    /// use lattice::infrastructure::services::traits::MockArticleExtractorService;
    ///
    /// let disabled = MockArticleExtractorService::new_disabled(
    ///     "Article extraction unavailable: SSL certificate error".to_string()
    /// );
    /// ```
    pub fn new_disabled(error_message: String) -> Self {
        let service = Self::new();
        // Store error message as a special marker
        service.responses.lock().insert(
            "__disabled__".to_string(),
            CleanArticle {
                title: error_message,
                author: None,
                content: String::new(),
                text_content: String::new(),
                word_count: 0,
                reading_time_minutes: 0,
                published_date: None,
                excerpt: None,
            },
        );
        service
    }

    /// Configure a response for a specific URL
    ///
    /// When `extract_article` is called with this URL, the configured
    /// article will be returned.
    ///
    /// # Arguments
    /// * `url` - The URL to match
    /// * `article` - The article to return
    pub fn set_response(&self, url: &str, article: CleanArticle) {
        self.responses.lock().insert(url.to_string(), article);
    }

    /// Clear all configured responses
    pub fn clear_responses(&self) {
        self.responses.lock().clear();
    }

    /// Check if this mock is in disabled mode
    fn is_disabled(&self) -> Option<String> {
        self.responses
            .lock()
            .get("__disabled__")
            .map(|a| a.title.clone())
    }
}

impl Default for MockArticleExtractorService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ArticleExtractorServiceTrait for MockArticleExtractorService {
    async fn extract_article_from_url(&self, url: &str) -> Result<CleanArticle> {
        // For mock, just call extract_article with empty HTML
        self.extract_article("", url).await
    }

    async fn extract_article(&self, _html: &str, url: &str) -> Result<CleanArticle> {
        // Check if service is disabled
        if let Some(error_message) = self.is_disabled() {
            return Err(crate::shared::error::AppError::ServiceNotAvailable(
                error_message,
            ));
        }

        // Return configured response if available
        if let Some(article) = self.responses.lock().get(url).cloned() {
            return Ok(article);
        }

        // Default response for testing
        Ok(CleanArticle {
            title: "Mock Article".to_string(),
            author: Some("Mock Author".to_string()),
            content: "<p>Mock article content</p>".to_string(),
            text_content: "Mock article content".to_string(),
            word_count: 3,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Mock article content".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_service_default_response() {
        let mock = MockArticleExtractorService::new();

        let result = mock
            .extract_article("<html>test</html>", "https://example.com")
            .await
            .unwrap();

        assert_eq!(result.title, "Mock Article");
        assert_eq!(result.author, Some("Mock Author".to_string()));
        assert_eq!(result.word_count, 3);
        assert_eq!(result.reading_time_minutes, 1);
    }

    #[tokio::test]
    async fn test_mock_service_configured_response() {
        let mock = MockArticleExtractorService::new();

        let custom_article = CleanArticle {
            title: "Custom Title".to_string(),
            author: Some("Custom Author".to_string()),
            content: "<p>Custom content goes here</p>".to_string(),
            text_content: "Custom content goes here".to_string(),
            word_count: 4,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Custom content goes here".to_string()),
        };

        mock.set_response("https://custom.com/article", custom_article.clone());

        let result = mock
            .extract_article("<html>ignored</html>", "https://custom.com/article")
            .await
            .unwrap();

        assert_eq!(result.title, "Custom Title");
        assert_eq!(result.author, Some("Custom Author".to_string()));
        assert_eq!(result.word_count, 4);
    }

    #[tokio::test]
    async fn test_mock_service_clear_responses() {
        let mock = MockArticleExtractorService::new();

        let custom_article = CleanArticle {
            title: "Custom".to_string(),
            author: None,
            content: "<p>Test</p>".to_string(),
            text_content: "Test".to_string(),
            word_count: 1,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Test".to_string()),
        };

        mock.set_response("https://test.com", custom_article);
        mock.clear_responses();

        // Should return default response after clear
        let result = mock
            .extract_article("<html>test</html>", "https://test.com")
            .await
            .unwrap();

        assert_eq!(result.title, "Mock Article");
    }

    #[tokio::test]
    async fn test_disabled_service_returns_error() {
        let error_msg = "Article extraction unavailable: SSL certificate error".to_string();
        let disabled_service = MockArticleExtractorService::new_disabled(error_msg.clone());

        let result = disabled_service
            .extract_article("<html>test</html>", "https://example.com")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("Service not available: {}", error_msg)
        );
    }

    #[tokio::test]
    async fn test_disabled_service_extract_from_url_returns_error() {
        let error_msg = "Article extraction unavailable: HTTP client init failed".to_string();
        let disabled_service = MockArticleExtractorService::new_disabled(error_msg.clone());

        let result = disabled_service
            .extract_article_from_url("https://example.com")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("Service not available: {}", error_msg)
        );
    }

    #[tokio::test]
    async fn test_normal_mock_still_works() {
        let normal_mock = MockArticleExtractorService::new();

        let result = normal_mock
            .extract_article("<html>test</html>", "https://example.com")
            .await;

        assert!(result.is_ok());
        let article = result.unwrap();
        assert_eq!(article.title, "Mock Article");
    }
}
