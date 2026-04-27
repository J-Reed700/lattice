//! # Ingest Web URL Use Case
//!
//! Fetches web content from a URL and indexes it into the document lattice.
//!
//! This use case orchestrates:
//! 1. URL validation (HTTP/HTTPS only)
//! 2. URL safety check (block malicious domains)
//! 3. Content fetching via HTTP
//! 4. Article metadata extraction
//! 5. Content cleaning (readability)
//! 6. Document indexing with embeddings
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::web::ingest_web_url::IngestWebUrlUseCase;
//! use vault_desktop::application::dtos::web_dto::IngestWebUrlRequestDto;
//!
//! # async fn example(use_case: IngestWebUrlUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = IngestWebUrlRequestDto {
//!     url: "https://example.com/article".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Indexed document {} with {} chunks", response.document_id, response.chunks_created);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::web::dto::{IngestWebUrlRequestDto, IngestWebUrlResponseDto};
use crate::features::web::WebIngestionServiceTrait;
use crate::shared::error::{AppError, Result};

/// Ingest web URL use case.
///
/// Coordinates web content ingestion, including:
/// - URL validation and safety checks
/// - Content fetching and extraction
/// - Document indexing with embeddings
///
/// ## Dependencies
///
/// - `WebIngestionServiceTrait`: Handles the complete web ingestion pipeline
pub struct IngestWebUrlUseCase {
    web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
}

impl IngestWebUrlUseCase {
    /// Create a new ingest web URL use case.
    ///
    /// # Arguments
    ///
    /// * `web_ingestion_service` - Service for web content ingestion
    pub fn new(web_ingestion_service: Arc<dyn WebIngestionServiceTrait>) -> Self {
        Self {
            web_ingestion_service,
        }
    }

    /// Execute web URL ingestion.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the URL to ingest
    ///
    /// # Returns
    ///
    /// Response with document ID and ingestion statistics
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - URL is invalid (not HTTP/HTTPS)
    /// - URL is blocked (malicious domain)
    /// - Network request fails
    /// - Content extraction fails
    /// - Indexing fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::web::ingest_web_url::IngestWebUrlUseCase;
    /// # use vault_desktop::application::dtos::web_dto::IngestWebUrlRequestDto;
    /// # async fn example(use_case: IngestWebUrlUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = IngestWebUrlRequestDto {
    ///     url: "https://blog.example.com/article".to_string(),
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(response) => {
    ///         println!("Success!");
    ///         println!("  Document ID: {}", response.document_id);
    ///         println!("  Title: {}", response.title);
    ///         println!("  Word Count: {}", response.word_count);
    ///         println!("  Chunks: {}", response.chunks_created);
    ///     }
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: IngestWebUrlRequestDto,
    ) -> Result<IngestWebUrlResponseDto> {
        // 1. Validate URL (must be HTTP or HTTPS)
        Self::validate_url(&request.url)?;

        // 2. Delegate to web ingestion service
        // (Service handles: fetch, extract, chunk, embed, index)
        let result = self.web_ingestion_service.ingest_url(&request.url).await?;

        // 3. Convert service result to DTO
        Ok(IngestWebUrlResponseDto {
            document_id: result.document_id,
            url: result.url,
            title: result.title,
            word_count: result.word_count,
            chunks_created: result.chunks_created,
            site_name: result.site_name,
            author: result.author,
            reading_time_minutes: result.reading_time_minutes,
        })
    }

    /// Validate URL is HTTP or HTTPS.
    ///
    /// # Arguments
    ///
    /// * `url` - URL to validate
    ///
    /// # Returns
    ///
    /// Ok if URL is valid, Err otherwise
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if URL is not HTTP/HTTPS
    fn validate_url(url: &str) -> Result<()> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::InvalidInput(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        if url.trim().is_empty() {
            return Err(AppError::InvalidInput("URL cannot be empty".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::web::WebIngestionResult;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock web ingestion service for testing
    struct MockWebIngestionService {
        responses: Arc<Mutex<HashMap<String, WebIngestionResult>>>,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockWebIngestionService {
        fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn set_response(&self, url: &str, result: WebIngestionResult) {
            self.responses
                .lock()
                .unwrap()
                .insert(url.to_string(), result);
        }

        fn get_call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl WebIngestionServiceTrait for MockWebIngestionService {
        async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult> {
            *self.call_count.lock().unwrap() += 1;

            if let Some(result) = self.responses.lock().unwrap().get(url).cloned() {
                return Ok(result);
            }

            // Default response
            Ok(WebIngestionResult {
                document_id: "doc-123".to_string(),
                url: url.to_string(),
                title: "Test Article".to_string(),
                word_count: 500,
                chunks_created: 2,
                site_name: Some("Example Site".to_string()),
                author: Some("Test Author".to_string()),
                reading_time_minutes: Some(3),
            })
        }
    }

    #[tokio::test]
    async fn test_ingest_valid_https_url() {
        // Arrange
        let mock_service = Arc::new(MockWebIngestionService::new());
        let expected = WebIngestionResult {
            document_id: "doc-456".to_string(),
            url: "https://example.com/article".to_string(),
            title: "Real Article".to_string(),
            word_count: 1000,
            chunks_created: 4,
            site_name: Some("Example".to_string()),
            author: Some("Jane Doe".to_string()),
            reading_time_minutes: Some(5),
        };
        mock_service.set_response("https://example.com/article", expected.clone());

        let use_case = IngestWebUrlUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "https://example.com/article".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.document_id, "doc-456");
        assert_eq!(response.url, "https://example.com/article");
        assert_eq!(response.title, "Real Article");
        assert_eq!(response.word_count, 1000);
        assert_eq!(response.chunks_created, 4);
        assert_eq!(mock_service.get_call_count(), 1);
    }

    #[tokio::test]
    async fn test_ingest_valid_http_url() {
        // Arrange
        let mock_service = Arc::new(MockWebIngestionService::new());
        let use_case = IngestWebUrlUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "http://example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        assert_eq!(mock_service.get_call_count(), 1);
    }

    #[tokio::test]
    async fn test_ingest_invalid_url_no_protocol() {
        // Arrange
        let mock_service = Arc::new(MockWebIngestionService::new());
        let use_case = IngestWebUrlUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        assert_eq!(mock_service.get_call_count(), 0); // Should not call service
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("http://") || msg.contains("https://"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_ingest_invalid_url_ftp() {
        // Arrange
        let mock_service = Arc::new(MockWebIngestionService::new());
        let use_case = IngestWebUrlUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "ftp://example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        assert_eq!(mock_service.get_call_count(), 0);
    }

    #[tokio::test]
    async fn test_ingest_empty_url() {
        // Arrange
        let mock_service = Arc::new(MockWebIngestionService::new());
        let use_case = IngestWebUrlUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        assert_eq!(mock_service.get_call_count(), 0);
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("empty") || msg.contains("http"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_ingest_network_error_propagates() {
        // Arrange
        struct FailingService;

        #[async_trait]
        impl WebIngestionServiceTrait for FailingService {
            async fn ingest_url(&self, _url: &str) -> Result<WebIngestionResult> {
                Err(AppError::Network("Connection timeout".to_string()))
            }
        }

        let use_case = IngestWebUrlUseCase::new(Arc::new(FailingService));

        // Act
        let result = use_case
            .execute(IngestWebUrlRequestDto {
                url: "https://example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Network(msg) => assert_eq!(msg, "Connection timeout"),
            _ => panic!("Expected Network error"),
        }
    }
}
