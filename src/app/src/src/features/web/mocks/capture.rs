//! Mock web capture service for testing
//!
//! Provides deterministic, thread-safe mock implementation of WebCaptureServiceTrait.

#[cfg(test)]
use crate::application::dtos::function_calling_dto::UrlPreview;
#[cfg(test)]
use crate::infrastructure::services::traits::WebCaptureServiceTrait;
#[cfg(test)]
use crate::shared::error::{AppError, Result};
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock web capture service for testing
///
/// Simulates URL preview without actual HTTP requests.
/// Allows configuration of responses and tracking of preview requests.
/// All operations are deterministic and thread-safe.
pub struct MockWebCaptureService {
    /// Configured mock previews by URL
    mock_previews: Arc<RwLock<HashMap<String, UrlPreview>>>,

    /// Track URLs that were previewed
    previewed_urls: Arc<RwLock<Vec<String>>>,
}

#[cfg(test)]
impl MockWebCaptureService {
    /// Create new mock service with empty state
    pub fn new() -> Self {
        Self {
            mock_previews: Arc::new(RwLock::new(HashMap::new())),
            previewed_urls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Configure mock to return specific preview for a URL
    ///
    /// When `fetch_url_preview` is called with this URL, the configured preview will be returned.
    ///
    /// # Arguments
    /// * `url` - The URL to match
    /// * `preview` - The UrlPreview to return
    ///
    /// # Example
    /// ```rust
    /// use vault_desktop::infrastructure::services::mocks::MockWebCaptureService;
    /// use vault_desktop::application::dtos::function_calling_dto::UrlPreview;
    ///
    /// let service = MockWebCaptureService::new();
    /// service.set_preview_for_url(
    ///     "https://example.com",
    ///     UrlPreview {
    ///         url: "https://example.com".to_string(),
    ///         title: "Example Article".to_string(),
    ///         description: Some("A test article".to_string()),
    ///         site_name: Some("Example".to_string()),
    ///         image: None,
    ///         author: Some("John Doe".to_string()),
    ///         published_date: None,
    ///         word_count: 500,
    ///         reading_time_minutes: 3,
    ///         language: Some("en".to_string()),
    ///         content_type: Some("article".to_string()),
    ///         keywords: vec!["test".to_string()],
    ///     }
    /// );
    /// ```
    pub fn set_preview_for_url(&self, url: impl Into<String>, preview: UrlPreview) {
        self.mock_previews
            .write()
            .expect("Mock lock poisoned")
            .insert(url.into(), preview);
    }

    /// Get list of URLs that were previewed
    ///
    /// Returns a clone of all URLs that have been previewed via `fetch_url_preview`.
    /// Useful for verifying that expected URLs were processed.
    ///
    /// # Returns
    /// Vector of URLs in the order they were previewed
    pub fn get_previewed_urls(&self) -> Vec<String> {
        self.previewed_urls
            .read()
            .expect("Mock lock poisoned")
            .clone()
    }

    /// Clear all previewed URLs and configured previews
    ///
    /// Resets the mock to initial empty state.
    pub fn clear(&self) {
        self.previewed_urls
            .write()
            .expect("Mock lock poisoned")
            .clear();
        self.mock_previews
            .write()
            .expect("Mock lock poisoned")
            .clear();
    }

    /// Create default preview for testing
    fn default_preview(url: &str) -> UrlPreview {
        UrlPreview {
            url: url.to_string(),
            title: "Mock Article Title".to_string(),
            description: Some("This is a mock article description for testing.".to_string()),
            site_name: Some("Mock Site".to_string()),
            image: Some("https://example.com/mock-image.jpg".to_string()),
            author: Some("Mock Author".to_string()),
            published_date: Some(Utc::now()),
            word_count: 350,
            reading_time_minutes: 2,
            language: Some("en".to_string()),
            content_type: Some("article".to_string()),
            keywords: vec!["mock".to_string(), "test".to_string()],
        }
    }
}

#[cfg(test)]
impl Default for MockWebCaptureService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl WebCaptureServiceTrait for MockWebCaptureService {
    async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview> {
        // Track preview request
        self.previewed_urls
            .write()
            .expect("Mock lock poisoned")
            .push(url.to_string());

        // Mock URL validation - block obvious bad URLs
        if url.contains("localhost") || url.contains("127.0.0.1") {
            return Err(AppError::InvalidUrl("localhost not allowed".to_string()));
        }

        // Return configured preview or default
        let previews = self.mock_previews.read().expect("Mock lock poisoned");
        if let Some(preview) = previews.get(url) {
            Ok(preview.clone())
        } else {
            // Default mock preview
            Ok(Self::default_preview(url))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_returns_configured_preview() {
        let service = MockWebCaptureService::new();

        let custom_preview = UrlPreview {
            url: "https://example.com/article".to_string(),
            title: "Custom Title".to_string(),
            description: Some("Custom description".to_string()),
            site_name: Some("Custom Site".to_string()),
            image: None,
            author: Some("Jane Doe".to_string()),
            published_date: None,
            word_count: 1000,
            reading_time_minutes: 5,
            language: Some("es".to_string()),
            content_type: Some("article".to_string()),
            keywords: vec!["custom".to_string()],
        };

        service.set_preview_for_url("https://example.com/article", custom_preview.clone());

        let result = service
            .fetch_url_preview("https://example.com/article")
            .await
            .unwrap();

        assert_eq!(result.title, "Custom Title");
        assert_eq!(result.author, Some("Jane Doe".to_string()));
        assert_eq!(result.word_count, 1000);
    }

    #[tokio::test]
    async fn test_mock_returns_default_preview() {
        let service = MockWebCaptureService::new();

        let result = service
            .fetch_url_preview("https://unknown.com")
            .await
            .unwrap();

        assert_eq!(result.title, "Mock Article Title");
        assert_eq!(result.url, "https://unknown.com");
    }

    #[tokio::test]
    async fn test_mock_tracks_previewed_urls() {
        let service = MockWebCaptureService::new();

        service
            .fetch_url_preview("https://example1.com")
            .await
            .unwrap();
        service
            .fetch_url_preview("https://example2.com")
            .await
            .unwrap();

        let urls = service.get_previewed_urls();
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example1.com".to_string()));
        assert!(urls.contains(&"https://example2.com".to_string()));
    }

    #[tokio::test]
    async fn test_mock_validates_localhost() {
        let service = MockWebCaptureService::new();

        let result = service.fetch_url_preview("http://localhost:8000").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_clear() {
        let service = MockWebCaptureService::new();

        service
            .fetch_url_preview("https://example.com")
            .await
            .unwrap();
        assert_eq!(service.get_previewed_urls().len(), 1);

        service.clear();
        assert_eq!(service.get_previewed_urls().len(), 0);
    }
}
