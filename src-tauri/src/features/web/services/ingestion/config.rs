//! Configuration and builder types for web ingestion.
//!
//! Holds [`WebIngestionConfig`], its validation rules, and the
//! [`WebIngestionServiceBuilder`] used by dependency-injection code.

use super::WebIngestionService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::indexing::IndexStorageTrait;
use crate::features::web::WebArchiveServiceTrait;
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Runtime configuration for web ingestion chunking behavior.
#[derive(Debug, Clone)]
pub struct WebIngestionConfig {
    /// Maximum tokens per chunk when splitting article text.
    pub chunk_size: usize,
    /// Token overlap between adjacent chunks.
    pub chunk_overlap: usize,
    /// Whether to prefer sentence boundaries during chunking.
    pub prefer_sentence_boundaries: bool,
    /// Enable video metadata extraction fallback via yt-dlp.
    pub enable_video_fallback: bool,
    /// Executable name/path for yt-dlp.
    pub yt_dlp_binary: String,
    /// Timeout in seconds for yt-dlp extraction.
    pub yt_dlp_timeout_secs: u64,
    /// Enable ASR fallback when subtitle extraction is unavailable.
    pub enable_asr_fallback: bool,
    /// Executable name/path for Whisper CLI.
    pub asr_binary: String,
    /// Whisper model name passed to CLI.
    pub asr_model: String,
    /// Optional language hint passed to ASR CLI (e.g., "en").
    pub asr_language: Option<String>,
    /// Timeout in seconds for ASR transcription.
    pub asr_timeout_secs: u64,
}

impl Default for WebIngestionConfig {
    fn default() -> Self {
        Self {
            chunk_size: 800,
            chunk_overlap: 120,
            prefer_sentence_boundaries: true,
            enable_video_fallback: true,
            yt_dlp_binary: "yt-dlp".to_string(),
            yt_dlp_timeout_secs: 30,
            enable_asr_fallback: false,
            asr_binary: "whisper".to_string(),
            asr_model: "base".to_string(),
            asr_language: Some("en".to_string()),
            asr_timeout_secs: 600,
        }
    }
}

/// Builder for `WebIngestionService`.
///
/// Useful when wiring optional settings and multiple dependencies in DI code.
pub struct WebIngestionServiceBuilder {
    article_extractor: Option<Arc<dyn ArticleExtractorServiceTrait>>,
    web_archive: Option<Arc<dyn WebArchiveServiceTrait>>,
    embedding_service: Option<Arc<dyn EmbeddingServiceTrait>>,
    index_storage: Option<Arc<dyn IndexStorageTrait>>,
    tokenizer: Option<Arc<Tokenizer>>,
    config: WebIngestionConfig,
}

impl WebIngestionServiceBuilder {
    pub fn new() -> Self {
        Self {
            article_extractor: None,
            web_archive: None,
            embedding_service: None,
            index_storage: None,
            tokenizer: None,
            config: WebIngestionConfig::default(),
        }
    }

    pub fn article_extractor(mut self, value: Arc<dyn ArticleExtractorServiceTrait>) -> Self {
        self.article_extractor = Some(value);
        self
    }

    pub fn web_archive(mut self, value: Arc<dyn WebArchiveServiceTrait>) -> Self {
        self.web_archive = Some(value);
        self
    }

    pub fn embedding_service(mut self, value: Arc<dyn EmbeddingServiceTrait>) -> Self {
        self.embedding_service = Some(value);
        self
    }

    pub fn index_storage(mut self, value: Arc<dyn IndexStorageTrait>) -> Self {
        self.index_storage = Some(value);
        self
    }

    pub fn tokenizer(mut self, value: Arc<Tokenizer>) -> Self {
        self.tokenizer = Some(value);
        self
    }

    pub fn config(mut self, value: WebIngestionConfig) -> Self {
        self.config = value;
        self
    }

    pub fn chunk_size(mut self, value: usize) -> Self {
        self.config.chunk_size = value;
        self
    }

    pub fn chunk_overlap(mut self, value: usize) -> Self {
        self.config.chunk_overlap = value;
        self
    }

    pub fn prefer_sentence_boundaries(mut self, value: bool) -> Self {
        self.config.prefer_sentence_boundaries = value;
        self
    }

    pub fn enable_video_fallback(mut self, value: bool) -> Self {
        self.config.enable_video_fallback = value;
        self
    }

    pub fn yt_dlp_binary(mut self, value: impl Into<String>) -> Self {
        self.config.yt_dlp_binary = value.into();
        self
    }

    pub fn yt_dlp_timeout_secs(mut self, value: u64) -> Self {
        self.config.yt_dlp_timeout_secs = value;
        self
    }

    pub fn enable_asr_fallback(mut self, value: bool) -> Self {
        self.config.enable_asr_fallback = value;
        self
    }

    pub fn asr_binary(mut self, value: impl Into<String>) -> Self {
        self.config.asr_binary = value.into();
        self
    }

    pub fn asr_model(mut self, value: impl Into<String>) -> Self {
        self.config.asr_model = value.into();
        self
    }

    pub fn asr_language(mut self, value: Option<String>) -> Self {
        self.config.asr_language = value;
        self
    }

    pub fn asr_timeout_secs(mut self, value: u64) -> Self {
        self.config.asr_timeout_secs = value;
        self
    }

    pub fn build(self) -> Result<WebIngestionService> {
        let article_extractor = self.article_extractor.ok_or_else(|| {
            AppError::InvalidConfig(
                "WebIngestionServiceBuilder missing required dependency: article_extractor"
                    .to_string(),
            )
        })?;
        let web_archive = self.web_archive.ok_or_else(|| {
            AppError::InvalidConfig(
                "WebIngestionServiceBuilder missing required dependency: web_archive".to_string(),
            )
        })?;
        let embedding_service = self.embedding_service.ok_or_else(|| {
            AppError::InvalidConfig(
                "WebIngestionServiceBuilder missing required dependency: embedding_service"
                    .to_string(),
            )
        })?;
        let index_storage = self.index_storage.ok_or_else(|| {
            AppError::InvalidConfig(
                "WebIngestionServiceBuilder missing required dependency: index_storage".to_string(),
            )
        })?;
        let tokenizer = self.tokenizer.ok_or_else(|| {
            AppError::InvalidConfig(
                "WebIngestionServiceBuilder missing required dependency: tokenizer".to_string(),
            )
        })?;

        WebIngestionService::with_config(
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            self.config,
        )
    }
}

impl Default for WebIngestionServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WebIngestionService {
    pub(super) fn validate_config(config: &WebIngestionConfig) -> Result<()> {
        if config.chunk_size == 0 {
            return Err(AppError::InvalidConfig(
                "Web ingestion chunk_size must be greater than 0".to_string(),
            ));
        }

        if config.chunk_overlap >= config.chunk_size {
            return Err(AppError::InvalidConfig(format!(
                "Web ingestion chunk_overlap ({}) must be smaller than chunk_size ({})",
                config.chunk_overlap, config.chunk_size
            )));
        }

        if config.enable_video_fallback && config.yt_dlp_binary.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Web ingestion yt_dlp_binary cannot be empty when video fallback is enabled"
                    .to_string(),
            ));
        }

        if config.enable_video_fallback && config.yt_dlp_timeout_secs == 0 {
            return Err(AppError::InvalidConfig(
                "Web ingestion yt_dlp_timeout_secs must be greater than 0".to_string(),
            ));
        }

        if config.enable_asr_fallback && config.asr_binary.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Web ingestion asr_binary cannot be empty when ASR fallback is enabled".to_string(),
            ));
        }

        if config.enable_asr_fallback && config.asr_model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Web ingestion asr_model cannot be empty when ASR fallback is enabled".to_string(),
            ));
        }

        if config.enable_asr_fallback && config.asr_timeout_secs == 0 {
            return Err(AppError::InvalidConfig(
                "Web ingestion asr_timeout_secs must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}
