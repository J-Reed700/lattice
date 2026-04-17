//! Mock web archive service for testing
//!
//! Provides in-memory mock implementation of WebArchiveServiceTrait.

use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::dtos::function_calling_dto::CleanArticle;
use crate::infrastructure::services::traits::WebArchiveServiceTrait;
use crate::shared::error::{AppError, Result};

/// Mock implementation of WebArchiveServiceTrait for testing
///
/// Stores articles in memory using a HashMap keyed by file path.
/// Useful for testing without actual filesystem operations.
///
/// # Example
/// ```rust
/// use vault_desktop::infrastructure::services::mocks::MockWebArchiveService;
/// use vault_desktop::application::dtos::function_calling_dto::CleanArticle;
///
/// let mock = MockWebArchiveService::new();
///
/// # tokio_test::block_on(async {
/// let article = CleanArticle {
///     title: "Test Article".to_string(),
///     author: Some("Author".to_string()),
///     content: "<p>Content</p>".to_string(),
///     text_content: "Content".to_string(),
///     word_count: 1,
///     reading_time_minutes: 1,
///     published_date: None,
///     excerpt: None,
/// };
///
/// let path = mock.archive_article(article.clone(), "https://example.com").await.unwrap();
/// let loaded = mock.load_article(&path).await.unwrap();
/// assert_eq!(loaded.title, "Test Article");
/// # });
/// ```
pub struct MockWebArchiveService {
    /// In-memory storage keyed by file path
    articles: Arc<Mutex<HashMap<PathBuf, CleanArticle>>>,
    /// Counter for generating unique paths
    counter: Arc<Mutex<usize>>,
}

impl MockWebArchiveService {
    /// Create a new mock service
    pub fn new() -> Self {
        Self {
            articles: Arc::new(Mutex::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Configure a stored article at a specific path
    ///
    /// Useful for setting up test fixtures.
    pub fn set_article(&self, path: PathBuf, article: CleanArticle) {
        self.articles.lock().insert(path, article);
    }

    /// Get the number of articles stored
    pub fn article_count(&self) -> usize {
        self.articles.lock().len()
    }

    /// Clear all stored articles
    pub fn clear(&self) {
        self.articles.lock().clear();
        *self.counter.lock() = 0;
    }

    /// Check if an article exists at the given path
    pub fn contains(&self, path: &PathBuf) -> bool {
        self.articles.lock().contains_key(path)
    }

    /// Get all stored article paths
    pub fn get_paths(&self) -> Vec<PathBuf> {
        self.articles.lock().keys().cloned().collect()
    }
}

impl Default for MockWebArchiveService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebArchiveServiceTrait for MockWebArchiveService {
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf> {
        // Extract domain from URL
        let domain = url::Url::parse(url)
            .map_err(|e| AppError::InvalidInput(format!("Invalid URL: {}", e)))?
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("URL has no host".to_string()))?
            .to_string();

        // Generate unique path
        let id = {
            let mut counter = self.counter.lock();
            *counter += 1;
            *counter
        };

        let path = PathBuf::from(format!(
            "/mock/web-archive/{}/{}-{}.md",
            domain,
            article.title.replace(' ', "-").to_lowercase(),
            id
        ));

        // Store article
        self.articles.lock().insert(path.clone(), article);

        Ok(path)
    }

    async fn load_article(&self, path: &Path) -> Result<CleanArticle> {
        self.articles
            .lock()
            .get(path)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Article not found: {}", path.display())))
    }

    async fn delete_article(&self, path: &Path) -> Result<()> {
        // Idempotent - ok if doesn't exist
        self.articles.lock().remove(path);
        Ok(())
    }

    async fn list_articles(&self) -> Result<Vec<PathBuf>> {
        Ok(self.articles.lock().keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_article() -> CleanArticle {
        CleanArticle {
            title: "Test Article".to_string(),
            author: Some("Test Author".to_string()),
            content: "<p>Test content</p>".to_string(),
            text_content: "Test content".to_string(),
            word_count: 2,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Test excerpt".to_string()),
        }
    }

    #[tokio::test]
    async fn test_archive_and_load() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        let path = mock
            .archive_article(article.clone(), "https://example.com/article")
            .await
            .unwrap();

        let loaded = mock.load_article(&path).await.unwrap();

        assert_eq!(loaded.title, article.title);
        assert_eq!(loaded.author, article.author);
        assert_eq!(loaded.word_count, article.word_count);
    }

    #[tokio::test]
    async fn test_delete_article() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        let path = mock
            .archive_article(article, "https://example.com")
            .await
            .unwrap();

        assert!(mock.contains(&path));
        assert_eq!(mock.article_count(), 1);

        mock.delete_article(&path).await.unwrap();

        assert!(!mock.contains(&path));
        assert_eq!(mock.article_count(), 0);
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let mock = MockWebArchiveService::new();
        let path = PathBuf::from("/nonexistent/path.md");

        // Should not error on deleting nonexistent article
        let result = mock.delete_article(&path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_articles() {
        let mock = MockWebArchiveService::new();

        let article1 = create_test_article();
        let article2 = CleanArticle {
            title: "Another Article".to_string(),
            ..create_test_article()
        };

        mock.archive_article(article1, "https://example.com/1")
            .await
            .unwrap();
        mock.archive_article(article2, "https://example.com/2")
            .await
            .unwrap();

        let articles = mock.list_articles().await.unwrap();
        assert_eq!(articles.len(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        mock.archive_article(article, "https://example.com")
            .await
            .unwrap();

        assert_eq!(mock.article_count(), 1);

        mock.clear();

        assert_eq!(mock.article_count(), 0);
    }

    #[tokio::test]
    async fn test_set_article() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();
        let path = PathBuf::from("/custom/path.md");

        mock.set_article(path.clone(), article.clone());

        let loaded = mock.load_article(&path).await.unwrap();
        assert_eq!(loaded.title, article.title);
    }

    #[tokio::test]
    async fn test_unique_paths() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        let path1 = mock
            .archive_article(article.clone(), "https://example.com")
            .await
            .unwrap();
        let path2 = mock
            .archive_article(article.clone(), "https://example.com")
            .await
            .unwrap();

        // Paths should be unique even for same article/URL
        assert_ne!(path1, path2);
    }

    #[tokio::test]
    async fn test_domain_extraction() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        let path = mock
            .archive_article(article, "https://github.com/user/repo")
            .await
            .unwrap();

        // Path should contain domain
        assert!(path.to_string_lossy().contains("github.com"));
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let mock = MockWebArchiveService::new();
        let article = create_test_article();

        let result = mock.archive_article(article, "not-a-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let mock = MockWebArchiveService::new();
        let path = PathBuf::from("/nonexistent/path.md");

        let result = mock.load_article(&path).await;
        assert!(result.is_err());

        match result {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }
}
