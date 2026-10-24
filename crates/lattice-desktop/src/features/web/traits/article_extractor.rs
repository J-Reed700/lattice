use async_trait::async_trait;
use lattice_core::application::contracts::web::CleanArticle;
use lattice_core::shared::error::Result;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Extracts clean article content (reader mode) from web pages.
#[async_trait]
pub trait ArticleExtractorServiceTrait: Send + Sync {
    /// Extract clean article content from raw HTML and source URL.
    async fn extract_article(&self, html: &str, url: &str) -> Result<CleanArticle>;

    /// Fetch URL and extract clean article content.
    async fn extract_article_from_url(&self, url: &str) -> Result<CleanArticle>;
}

/// Mock implementation of ArticleExtractorServiceTrait for testing.
pub struct MockArticleExtractorService {
    responses: Arc<Mutex<HashMap<String, CleanArticle>>>,
}

impl MockArticleExtractorService {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a disabled service that returns `ServiceNotAvailable` with `error_message`.
    pub fn new_disabled(error_message: String) -> Self {
        let service = Self::new();
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

    pub fn set_response(&self, url: &str, article: CleanArticle) {
        self.responses.lock().insert(url.to_string(), article);
    }

    pub fn clear_responses(&self) {
        self.responses.lock().clear();
    }

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
        self.extract_article("", url).await
    }

    async fn extract_article(&self, _html: &str, url: &str) -> Result<CleanArticle> {
        if let Some(error_message) = self.is_disabled() {
            return Err(lattice_core::shared::error::AppError::ServiceNotAvailable(
                error_message,
            ));
        }

        if let Some(article) = self.responses.lock().get(url).cloned() {
            return Ok(article);
        }

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
