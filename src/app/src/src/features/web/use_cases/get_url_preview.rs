//! # Get URL Preview Use Case
//!
//! Fetches URL metadata without indexing the content.
//!
//! This use case orchestrates:
//! 1. URL validation (HTTP/HTTPS only)
//! 2. HTML fetching (HEAD + partial GET for efficiency)
//! 3. Metadata extraction (Open Graph, Twitter Card, Schema.org)
//! 4. Preview return (NO indexing)
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::web::get_url_preview::GetUrlPreviewUseCase;
//! use vault_desktop::application::dtos::web_dto::GetUrlPreviewRequestDto;
//!
//! # async fn example(use_case: GetUrlPreviewUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GetUrlPreviewRequestDto {
//!     url: "https://example.com/article".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Title: {}", response.title);
//! println!("Description: {:?}", response.description);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::function_calling::dto::UrlPreview;
use crate::features::web::dto::{GetUrlPreviewRequestDto, UrlPreviewDto};
use crate::features::web::WebCaptureServiceTrait;
use crate::shared::error::{AppError, Result};

/// Get URL preview use case.
///
/// Fetches URL metadata without indexing, including:
/// - Page title, description, image
/// - Open Graph / Twitter Card metadata
/// - Site information
///
/// ## Dependencies
///
/// - `WebCaptureServiceTrait`: Handles URL fetching and metadata extraction
pub struct GetUrlPreviewUseCase {
    web_capture_service: Arc<dyn WebCaptureServiceTrait>,
}

impl GetUrlPreviewUseCase {
    /// Create a new get URL preview use case.
    ///
    /// # Arguments
    ///
    /// * `web_capture_service` - Service for URL preview and metadata extraction
    pub fn new(web_capture_service: Arc<dyn WebCaptureServiceTrait>) -> Self {
        Self {
            web_capture_service,
        }
    }

    /// Execute URL preview.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the URL to preview
    ///
    /// # Returns
    ///
    /// Preview with URL metadata (title, description, image, site name)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - URL is invalid (not HTTP/HTTPS)
    /// - Network request fails
    /// - HTML parsing fails
    /// - Metadata extraction fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::web::get_url_preview::GetUrlPreviewUseCase;
    /// # use vault_desktop::application::dtos::web_dto::GetUrlPreviewRequestDto;
    /// # async fn example(use_case: GetUrlPreviewUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = GetUrlPreviewRequestDto {
    ///     url: "https://blog.example.com/article".to_string(),
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(preview) => {
    ///         println!("Title: {}", preview.title);
    ///         if let Some(desc) = preview.description {
    ///             println!("Description: {}", desc);
    ///         }
    ///         if let Some(img) = preview.image {
    ///             println!("Image: {}", img);
    ///         }
    ///     }
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, request: GetUrlPreviewRequestDto) -> Result<UrlPreviewDto> {
        // 1. Validate URL (must be HTTP or HTTPS)
        Self::validate_url(&request.url)?;

        // 2. Delegate to web capture service
        // (Service handles: URL validation, fetch, metadata extraction)
        let preview = self
            .web_capture_service
            .fetch_url_preview(&request.url)
            .await?;

        // 3. Convert service result to DTO
        Ok(Self::convert_preview(preview))
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

    /// Convert UrlPreview from function_calling_dto to web_dto format.
    ///
    /// # Arguments
    ///
    /// * `preview` - UrlPreview from service layer
    ///
    /// # Returns
    ///
    /// UrlPreviewDto for application layer
    fn convert_preview(preview: UrlPreview) -> UrlPreviewDto {
        UrlPreviewDto {
            url: preview.url,
            title: preview.title,
            description: preview.description,
            image: preview.image,
            site_name: preview.site_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock web capture service for testing
    struct MockWebCaptureService {
        responses: Arc<Mutex<HashMap<String, UrlPreview>>>,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockWebCaptureService {
        fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn set_response(&self, url: &str, preview: UrlPreview) {
            self.responses
                .lock()
                .unwrap()
                .insert(url.to_string(), preview);
        }

        fn get_call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl WebCaptureServiceTrait for MockWebCaptureService {
        async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview> {
            *self.call_count.lock().unwrap() += 1;

            if let Some(preview) = self.responses.lock().unwrap().get(url).cloned() {
                return Ok(preview);
            }

            // Default response
            Ok(UrlPreview {
                url: url.to_string(),
                title: "Default Title".to_string(),
                description: Some("Default description".to_string()),
                image: Some("https://example.com/image.jpg".to_string()),
                site_name: Some("Example Site".to_string()),
                author: None,
                published_date: None,
                word_count: 0,
                reading_time_minutes: 0,
                language: None,
                content_type: None,
                keywords: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_preview_url_with_open_graph() {
        // Arrange
        let mock_service = Arc::new(MockWebCaptureService::new());
        let expected = UrlPreview {
            url: "https://example.com/article".to_string(),
            title: "Amazing Article".to_string(),
            description: Some("This is an amazing article about...".to_string()),
            image: Some("https://example.com/og-image.jpg".to_string()),
            site_name: Some("Example Blog".to_string()),
            author: Some("Jane Doe".to_string()),
            published_date: None,
            word_count: 1000,
            reading_time_minutes: 5,
            language: Some("en".to_string()),
            content_type: None,
            keywords: vec![],
        };
        mock_service.set_response("https://example.com/article", expected.clone());

        let use_case = GetUrlPreviewUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
                url: "https://example.com/article".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let preview = result.unwrap();
        assert_eq!(preview.url, "https://example.com/article");
        assert_eq!(preview.title, "Amazing Article");
        assert_eq!(
            preview.description,
            Some("This is an amazing article about...".to_string())
        );
        assert_eq!(preview.site_name, Some("Example Blog".to_string()));
        assert_eq!(mock_service.get_call_count(), 1);
    }

    #[tokio::test]
    async fn test_preview_url_without_metadata() {
        // Arrange
        let mock_service = Arc::new(MockWebCaptureService::new());
        let minimal = UrlPreview {
            url: "https://minimal.com".to_string(),
            title: "Minimal Page".to_string(),
            description: None,
            image: None,
            site_name: None,
            author: None,
            published_date: None,
            word_count: 0,
            reading_time_minutes: 0,
            language: None,
            content_type: None,
            keywords: vec![],
        };
        mock_service.set_response("https://minimal.com", minimal);

        let use_case = GetUrlPreviewUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
                url: "https://minimal.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let preview = result.unwrap();
        assert_eq!(preview.title, "Minimal Page");
        assert_eq!(preview.description, None);
        assert_eq!(preview.image, None);
        assert_eq!(preview.site_name, None);
    }

    #[tokio::test]
    async fn test_preview_invalid_url_no_protocol() {
        // Arrange
        let mock_service = Arc::new(MockWebCaptureService::new());
        let use_case = GetUrlPreviewUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
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
    async fn test_preview_invalid_url_wrong_protocol() {
        // Arrange
        let mock_service = Arc::new(MockWebCaptureService::new());
        let use_case = GetUrlPreviewUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
                url: "ftp://example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        assert_eq!(mock_service.get_call_count(), 0);
    }

    #[tokio::test]
    async fn test_preview_empty_url() {
        // Arrange
        let mock_service = Arc::new(MockWebCaptureService::new());
        let use_case = GetUrlPreviewUseCase::new(mock_service.clone());

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
                url: "".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        assert_eq!(mock_service.get_call_count(), 0);
    }

    #[tokio::test]
    async fn test_preview_network_error_propagates() {
        // Arrange
        struct FailingService;

        #[async_trait]
        impl WebCaptureServiceTrait for FailingService {
            async fn fetch_url_preview(&self, _url: &str) -> Result<UrlPreview> {
                Err(AppError::Network("Connection refused".to_string()))
            }
        }

        let use_case = GetUrlPreviewUseCase::new(Arc::new(FailingService));

        // Act
        let result = use_case
            .execute(GetUrlPreviewRequestDto {
                url: "https://example.com".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Network(msg) => assert_eq!(msg, "Connection refused"),
            _ => panic!("Expected Network error"),
        }
    }
}
