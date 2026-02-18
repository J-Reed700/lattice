pub mod config;
pub mod error;
pub mod extractor;
pub mod fetcher;
pub mod metadata;
pub mod types;

pub use config::{WebIngestionConfig, WebIngestionConfigBuilder};
pub use error::{Result, WebIngestionError};
pub use types::{WebContent, WebDocument, WebMetadata};

use extractor::ContentExtractor;
use fetcher::WebFetcher;
use metadata::MetadataExtractor;
use scraper::Html;
use tracing::{debug, info};

pub struct WebIngestionService {
    fetcher: WebFetcher,
    extractor: ContentExtractor,
    config: WebIngestionConfig,
}

impl WebIngestionService {
    pub fn new(config: WebIngestionConfig) -> Result<Self> {
        let fetcher = WebFetcher::new(config.clone())?;
        let extractor = ContentExtractor::new(config.clone());

        Ok(Self {
            fetcher,
            extractor,
            config,
        })
    }

    pub fn with_default_config() -> Result<Self> {
        Self::new(WebIngestionConfig::default())
    }

    pub async fn ingest(&self, url: &str) -> Result<WebDocument> {
        info!("Starting web ingestion for: {}", url);

        let html = self.fetcher.fetch(url).await?;
        debug!("Fetched {} bytes of HTML", html.len());

        let content = self.extractor.extract(&html, url)?;
        debug!(
            "Extracted content: title='{}', {} words",
            content.title, content.word_count
        );

        let html_doc = Html::parse_document(&html);
        let metadata = MetadataExtractor::extract(&html_doc, url);
        debug!("Extracted metadata: {:?}", metadata);

        let document = WebDocument::new(url.to_string(), content, metadata);

        info!(
            "Successfully ingested web document: {} ({} words)",
            document.content.title, document.content.word_count
        );

        Ok(document)
    }

    pub async fn ingest_batch(&self, urls: Vec<String>) -> Vec<Result<WebDocument>> {
        let mut results = Vec::with_capacity(urls.len());

        for url in urls {
            let result = self.ingest(&url).await;
            results.push(result);
        }

        results
    }

    pub fn get_config(&self) -> &WebIngestionConfig {
        &self.config
    }
}

impl Default for WebIngestionService {
    fn default() -> Self {
        // Safe unwrap: with_default_config() only fails if we can't load the tokenizer,
        // which should never happen with the built-in default config.
        // Using unwrap() instead of expect() to comply with clippy::expect_used lint.
        Self::with_default_config().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = WebIngestionService::with_default_config();
        assert!(service.is_ok());
    }

    #[test]
    fn test_service_with_custom_config() {
        let config = WebIngestionConfig::builder()
            .min_text_length(50)
            .build();

        let service = WebIngestionService::new(config);
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_invalid_url() {
        let service = WebIngestionService::with_default_config().unwrap();
        let result = service.ingest("not-a-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ingest_blocked_url() {
        let service = WebIngestionService::with_default_config().unwrap();
        let result = service.ingest("http://localhost:8080").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WebIngestionError::BlockedUrl { .. }
        ));
    }
}
