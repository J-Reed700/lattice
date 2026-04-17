//! Web ingestion service implementation
//!
//! Orchestrates the complete workflow for importing web articles:
//! 1. Extract article content from URL
//! 2. Archive article as markdown file in ~/.recall/web-archive
//! 3. Chunk text content
//! 4. Generate embeddings
//! 5. Store document + chunks + embeddings

use crate::features::function_calling::dto::CleanArticle;
use crate::infrastructure::indexing::chunker::{
    ChunkerConfig, ContextualizedChunk, SemanticChunker,
};
use crate::infrastructure::services::traits::{
    ArticleExtractorServiceTrait, EmbeddingServiceTrait, IndexStorageTrait, WebArchiveServiceTrait,
    WebIngestionResult, WebIngestionServiceTrait,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokenizers::Tokenizer;
use tokio::fs as async_fs;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractionStrategy {
    Article,
    VideoMetadata,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct YtDlpMetadata {
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    description: Option<String>,
    webpage_url: Option<String>,
    original_url: Option<String>,
    upload_date: Option<String>,
    duration: Option<f64>,
    extractor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Transcript {
    events: Option<Vec<Json3Event>>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Event {
    segs: Option<Vec<Json3Segment>>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Segment {
    utf8: Option<String>,
}

#[derive(Debug)]
struct TempDirectoryGuard {
    path: PathBuf,
}

impl TempDirectoryGuard {
    fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("{}-{}", prefix, Uuid::new_v4()));
        fs::create_dir_all(&path).map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to create temporary directory '{}': {}",
                path.display(),
                e
            ))
        })?;

        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectoryGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            debug!(
                path = %self.path.display(),
                error = %error,
                "Failed to cleanup temporary directory"
            );
        }
    }
}

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

/// Web ingestion service
///
/// Production implementation that orchestrates the complete URL import workflow:
/// 1. Extracts article from URL (via ArticleExtractorService)
/// 2. Archives article as markdown file (via WebArchiveService)
/// 3. Chunks text content using SemanticChunker
/// 4. Generates embeddings (via EmbeddingService)
/// 5. Stores document with chunks and embeddings (via IndexStorage)
///
/// # Dependencies
///
/// - `ArticleExtractorServiceTrait`: Extracts clean article content from URLs
/// - `WebArchiveServiceTrait`: Saves articles as markdown files in ~/.recall/web-archive
/// - `EmbeddingServiceTrait`: Generates vector embeddings for text chunks
/// - `IndexStorageTrait`: Stores documents with chunks and embeddings
///
/// # Example
///
/// ```rust,no_run
/// use vault_desktop::infrastructure::services::{
///     WebIngestionService, ArticleExtractorService, embedding::OnnxEmbeddingService
/// };
/// use vault_desktop::infrastructure::indexing::storage::IndexStorage;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let article_extractor = Arc::new(ArticleExtractorService::new(/* ... */));
/// let web_archive = Arc::new(WebArchiveService::new()?);
/// let embedding_service = Arc::new(OnnxEmbeddingService::new(/* ... */).await?);
/// let index_storage = Arc::new(IndexStorage::new(/* ... */));
/// let tokenizer = Arc::new(Tokenizer::from_pretrained("bert-base-uncased", None)?);
///
/// let service = WebIngestionService::new(
///     article_extractor,
///     web_archive,
///     embedding_service,
///     index_storage,
///     tokenizer,
/// );
///
/// let result = service.ingest_url("https://example.com/article").await?;
/// println!("Ingested: {} ({})", result.title, result.document_id);
/// # Ok(())
/// # }
/// ```
pub struct WebIngestionService {
    article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
    web_archive: Arc<dyn WebArchiveServiceTrait>,
    embedding_service: Arc<dyn EmbeddingServiceTrait>,
    index_storage: Arc<dyn IndexStorageTrait>,
    tokenizer: Arc<Tokenizer>,
    config: WebIngestionConfig,
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
    /// Create a new web ingestion service
    ///
    /// # Arguments
    ///
    /// * `article_extractor` - Service for extracting article content
    /// * `web_archive` - Service for archiving articles as markdown files
    /// * `embedding_service` - Service for generating embeddings
    /// * `index_storage` - Storage for documents and embeddings
    /// * `tokenizer` - Tokenizer for text chunking
    pub fn new(
        article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
        web_archive: Arc<dyn WebArchiveServiceTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        index_storage: Arc<dyn IndexStorageTrait>,
        tokenizer: Arc<Tokenizer>,
    ) -> Self {
        Self {
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            config: WebIngestionConfig::default(),
        }
    }

    /// Create a builder for constructing `WebIngestionService`.
    pub fn builder() -> WebIngestionServiceBuilder {
        WebIngestionServiceBuilder::new()
    }

    /// Create service with explicit runtime configuration.
    pub fn with_config(
        article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
        web_archive: Arc<dyn WebArchiveServiceTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        index_storage: Arc<dyn IndexStorageTrait>,
        tokenizer: Arc<Tokenizer>,
        config: WebIngestionConfig,
    ) -> Result<Self> {
        Self::validate_config(&config)?;

        Ok(Self {
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            config,
        })
    }

    fn validate_config(config: &WebIngestionConfig) -> Result<()> {
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

    fn host_matches_domain(host: &str, base_domain: &str) -> bool {
        host == base_domain || host.ends_with(&format!(".{}", base_domain))
    }

    fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    fn parse_yt_dlp_upload_date(&self, value: &str) -> Option<DateTime<Utc>> {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        let datetime = date.and_hms_opt(0, 0, 0)?;
        Some(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc))
    }

    fn calculate_reading_time_minutes(word_count: usize) -> i64 {
        ((word_count as f64 / 200.0).ceil() as i64).max(1)
    }

    fn generate_excerpt(text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.len() <= 200 {
            return Some(trimmed.to_string());
        }

        let truncated = &trimmed[..200];
        if let Some(last_space) = truncated.rfind(' ') {
            Some(format!("{}...", &trimmed[..last_space]))
        } else {
            Some(format!("{}...", truncated))
        }
    }

    fn strip_html_like_tags(text: &str) -> String {
        let mut output = String::new();
        let mut in_tag = false;

        for ch in text.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => output.push(ch),
                _ => {}
            }
        }

        output
    }

    fn dedupe_consecutive_lines(lines: Vec<String>) -> Vec<String> {
        let mut deduped = Vec::new();

        for line in lines {
            if deduped
                .last()
                .is_some_and(|previous: &String| previous == &line)
            {
                continue;
            }
            deduped.push(line);
        }

        deduped
    }

    fn parse_vtt_transcript_text(contents: &str) -> String {
        let mut lines = Vec::new();
        let mut skip_note_block = false;

        for raw_line in contents.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                skip_note_block = false;
                continue;
            }

            if trimmed.starts_with("NOTE") {
                skip_note_block = true;
                continue;
            }

            if skip_note_block
                || trimmed == "WEBVTT"
                || trimmed.starts_with("Kind:")
                || trimmed.starts_with("Language:")
                || trimmed.contains("-->")
                || trimmed.parse::<usize>().is_ok()
            {
                continue;
            }

            let cleaned = Self::normalize_whitespace(
                &Self::strip_html_like_tags(trimmed).replace("&nbsp;", " "),
            );
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        }

        Self::dedupe_consecutive_lines(lines).join("\n")
    }

    fn parse_srt_transcript_text(contents: &str) -> String {
        let mut lines = Vec::new();

        for raw_line in contents.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.contains("-->") || trimmed.parse::<usize>().is_ok() {
                continue;
            }

            let cleaned = Self::normalize_whitespace(&Self::strip_html_like_tags(trimmed));
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        }

        Self::dedupe_consecutive_lines(lines).join("\n")
    }

    fn parse_json3_transcript_text(contents: &str) -> Option<String> {
        let parsed = serde_json::from_str::<Json3Transcript>(contents).ok()?;
        let mut lines = Vec::new();

        for event in parsed.events.unwrap_or_default() {
            for segment in event.segs.unwrap_or_default() {
                if let Some(text) = segment.utf8 {
                    let cleaned = Self::normalize_whitespace(&Self::strip_html_like_tags(&text));
                    if !cleaned.is_empty() {
                        lines.push(cleaned);
                    }
                }
            }
        }

        let transcript = Self::dedupe_consecutive_lines(lines).join("\n");
        if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        }
    }

    fn extract_transcript_text_from_file(path: &Path) -> Option<String> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)?;

        let contents = fs::read_to_string(path).ok()?;
        let transcript = match extension.as_str() {
            "vtt" => Self::parse_vtt_transcript_text(&contents),
            "srt" => Self::parse_srt_transcript_text(&contents),
            "json3" => Self::parse_json3_transcript_text(&contents)?,
            _ => return None,
        };

        if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        }
    }

    fn subtitle_file_score(path: &Path) -> i32 {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        let mut score = match extension.as_str() {
            "vtt" => 50,
            "srt" => 40,
            "json3" => 30,
            _ => 0,
        };

        if file_name.contains(".en.")
            || file_name.contains(".en-")
            || file_name.ends_with(".en.vtt")
        {
            score += 100;
        } else if file_name.contains("english") {
            score += 80;
        }

        score
    }

    fn load_transcript_from_directory(directory: &Path) -> Option<String> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| path.is_file())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "vtt" | "srt" | "json3"))
                    .unwrap_or(false)
            })
            .collect();

        candidates.sort_by(|a, b| Self::subtitle_file_score(b).cmp(&Self::subtitle_file_score(a)));

        for candidate in candidates {
            if let Some(transcript) = Self::extract_transcript_text_from_file(&candidate) {
                let word_count = transcript.split_whitespace().count();
                if word_count >= 20 {
                    return Some(transcript);
                }
            }
        }

        None
    }

    fn audio_file_score(path: &Path) -> i32 {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        match extension.as_str() {
            "wav" => 100,
            "m4a" => 80,
            "mp3" => 70,
            "opus" => 60,
            "webm" => 50,
            _ => 0,
        }
    }

    fn find_best_audio_file(directory: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| path.is_file())
            .filter(|path| Self::audio_file_score(path) > 0)
            .collect();

        candidates.sort_by(|a, b| Self::audio_file_score(b).cmp(&Self::audio_file_score(a)));
        candidates.into_iter().next()
    }

    fn normalize_transcript_for_storage(transcript: &str) -> String {
        transcript
            .lines()
            .map(Self::normalize_whitespace)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn download_audio_for_asr(&self, url: &str, directory: &Path) -> Result<PathBuf> {
        let command_future = Command::new(&self.config.yt_dlp_binary)
            .arg("-f")
            .arg("bestaudio/best")
            .arg("--extract-audio")
            .arg("--audio-format")
            .arg("wav")
            .arg("--audio-quality")
            .arg("0")
            .arg("--paths")
            .arg(directory)
            .arg("-o")
            .arg("%(id)s.%(ext)s")
            .arg("--no-warnings")
            .arg("--")
            .arg(url)
            .output();

        let output = timeout(
            Duration::from_secs(self.config.yt_dlp_timeout_secs),
            command_future,
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "yt-dlp audio download timed out after {} seconds",
                self.config.yt_dlp_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute yt-dlp for audio download '{}': {}",
                self.config.yt_dlp_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("yt-dlp audio download failed: {}", stderr),
            });
        }

        let best_audio = tokio::task::spawn_blocking({
            let directory = directory.to_path_buf();
            move || Self::find_best_audio_file(&directory)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to inspect downloaded audio files: {}", e),
        })?;

        best_audio.ok_or_else(|| AppError::ContentExtraction {
            path: url.to_string(),
            reason: "yt-dlp completed but no audio file was produced".to_string(),
        })
    }

    fn find_best_asr_output_file(directory: &Path, audio_stem: &str) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("txt"))
                        .unwrap_or(false)
            })
            .collect();

        candidates.sort_by(|a, b| {
            let a_name = a
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let b_name = b
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();

            let a_score = if a_name.contains(&audio_stem.to_ascii_lowercase()) {
                1
            } else {
                0
            };
            let b_score = if b_name.contains(&audio_stem.to_ascii_lowercase()) {
                1
            } else {
                0
            };

            b_score.cmp(&a_score)
        });

        candidates.into_iter().next()
    }

    async fn transcribe_audio_with_asr(&self, url: &str, audio_path: &Path) -> Result<String> {
        let output_directory = audio_path.parent().ok_or_else(|| {
            AppError::FileSystem("Audio path missing parent directory".to_string())
        })?;

        let mut command = Command::new(&self.config.asr_binary);
        command.arg(audio_path);
        command.arg("--model").arg(&self.config.asr_model);
        command.arg("--output_format").arg("txt");
        command.arg("--output_dir").arg(output_directory);
        command.arg("--fp16").arg("False");
        if let Some(language) = &self.config.asr_language {
            if !language.trim().is_empty() {
                command.arg("--language").arg(language);
            }
        }

        let output = timeout(
            Duration::from_secs(self.config.asr_timeout_secs),
            command.output(),
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "ASR transcription timed out after {} seconds",
                self.config.asr_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute ASR binary '{}': {}",
                self.config.asr_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("ASR transcription failed: {}", stderr),
            });
        }

        let audio_stem = audio_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        let transcript_path = tokio::task::spawn_blocking({
            let output_directory = output_directory.to_path_buf();
            let audio_stem = audio_stem.to_string();
            move || Self::find_best_asr_output_file(&output_directory, &audio_stem)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to inspect ASR output files: {}", e),
        })?
        .ok_or_else(|| AppError::ContentExtraction {
            path: url.to_string(),
            reason: "ASR completed but transcript file was not found".to_string(),
        })?;

        let transcript = async_fs::read_to_string(&transcript_path)
            .await
            .map_err(|e| AppError::FileRead {
                path: transcript_path.display().to_string(),
                reason: e.to_string(),
            })?;

        let normalized = Self::normalize_transcript_for_storage(&transcript);
        if normalized.is_empty() {
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: "ASR produced an empty transcript".to_string(),
            });
        }

        Ok(normalized)
    }

    async fn extract_transcript_with_asr(&self, url: &str, directory: &Path) -> Result<String> {
        let audio_path = self.download_audio_for_asr(url, directory).await?;
        self.transcribe_audio_with_asr(url, &audio_path).await
    }

    fn build_video_article_with_transcript(
        &self,
        source_url: &str,
        metadata: YtDlpMetadata,
        transcript: &str,
    ) -> Result<CleanArticle> {
        let mut article = self.build_video_article_from_metadata(source_url, metadata)?;
        let normalized_transcript = Self::normalize_whitespace(transcript);
        if normalized_transcript.is_empty() {
            return Ok(article);
        }

        article.text_content = format!(
            "{}\n\nTranscript:\n{}",
            article.text_content, normalized_transcript
        );
        article.word_count = article.text_content.split_whitespace().count();
        article.reading_time_minutes = Self::calculate_reading_time_minutes(article.word_count);
        article.excerpt = Self::generate_excerpt(&article.text_content);

        let transcript_html = Self::escape_html(&normalized_transcript).replace('\n', "<br/>");
        if let Some(stripped) = article.content.strip_suffix("</article>") {
            article.content = format!(
                "{}<h2>Transcript</h2><p>{}</p></article>",
                stripped, transcript_html
            );
        } else {
            article.content = format!(
                "{}<h2>Transcript</h2><p>{}</p>",
                article.content, transcript_html
            );
        }

        Ok(article)
    }

    fn is_likely_video_url(&self, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };

        let host = parsed
            .host_str()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if host.is_empty() {
            return false;
        }

        let known_video_hosts = [
            "youtube.com",
            "youtu.be",
            "vimeo.com",
            "dailymotion.com",
            "tiktok.com",
            "twitch.tv",
            "twitter.com",
            "x.com",
            "instagram.com",
            "facebook.com",
            "rumble.com",
        ];

        if known_video_hosts
            .iter()
            .any(|site| Self::host_matches_domain(&host, site))
        {
            return true;
        }

        let path = parsed.path().to_ascii_lowercase();
        [".mp4", ".mov", ".m4v", ".webm", ".mkv"]
            .iter()
            .any(|ext| path.ends_with(ext))
    }

    fn extraction_strategies_for_url(&self, url: &str) -> Vec<ExtractionStrategy> {
        if !self.config.enable_video_fallback {
            return vec![ExtractionStrategy::Article];
        }

        if self.is_likely_video_url(url) {
            vec![
                ExtractionStrategy::VideoMetadata,
                ExtractionStrategy::Article,
            ]
        } else {
            vec![ExtractionStrategy::Article]
        }
    }

    fn build_video_article_from_metadata(
        &self,
        source_url: &str,
        metadata: YtDlpMetadata,
    ) -> Result<CleanArticle> {
        let title = metadata
            .title
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Video".to_string());

        let author = metadata
            .uploader
            .or(metadata.channel)
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty());

        let description = metadata
            .description
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty());

        let canonical_url = metadata
            .webpage_url
            .or(metadata.original_url)
            .unwrap_or_else(|| source_url.to_string());

        let published_date = metadata
            .upload_date
            .as_ref()
            .and_then(|value| self.parse_yt_dlp_upload_date(value));

        let duration_seconds = metadata.duration.unwrap_or(0.0).max(0.0).round() as i64;
        let duration_text = if duration_seconds > 0 {
            format!("Duration: {} seconds", duration_seconds)
        } else {
            "Duration: Unknown".to_string()
        };

        let extractor_text = metadata
            .extractor
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty())
            .map(|value| format!("Extractor: {}", value));

        let mut text_sections = vec![format!("Title: {}", title)];
        if let Some(author_value) = &author {
            text_sections.push(format!("Creator: {}", author_value));
        }
        text_sections.push(duration_text.clone());
        if let Some(description_value) = &description {
            text_sections.push(description_value.clone());
        }
        if let Some(extractor_value) = &extractor_text {
            text_sections.push(extractor_value.clone());
        }
        text_sections.push(format!("Source: {}", canonical_url));

        let text_content = text_sections.join("\n\n");
        let word_count = text_content.split_whitespace().count();
        let reading_time_minutes = Self::calculate_reading_time_minutes(word_count);
        let excerpt = Self::generate_excerpt(&text_content);

        if word_count == 0 {
            return Err(AppError::ContentExtraction {
                path: source_url.to_string(),
                reason: "Video metadata extraction produced empty content".to_string(),
            });
        }

        let mut content_parts = vec![format!("<h1>{}</h1>", Self::escape_html(&title))];
        if let Some(author_value) = &author {
            content_parts.push(format!(
                "<p><strong>Creator:</strong> {}</p>",
                Self::escape_html(author_value)
            ));
        }
        content_parts.push(format!("<p>{}</p>", Self::escape_html(&duration_text)));
        if let Some(description_value) = &description {
            content_parts.push(format!("<p>{}</p>", Self::escape_html(description_value)));
        }
        if let Some(extractor_value) = &extractor_text {
            content_parts.push(format!(
                "<p><strong>{}</strong></p>",
                Self::escape_html(extractor_value)
            ));
        }
        content_parts.push(format!(
            "<p><a href=\"{}\">Open source video</a></p>",
            Self::escape_html(&canonical_url)
        ));

        Ok(CleanArticle {
            title,
            author,
            content: format!("<article>{}</article>", content_parts.join("")),
            text_content,
            word_count,
            reading_time_minutes,
            published_date,
            excerpt,
        })
    }

    async fn extract_video_metadata_with_ytdlp(&self, url: &str) -> Result<CleanArticle> {
        let temp_dir = TempDirectoryGuard::new("recall-ytdlp")?;

        let command_future = Command::new(&self.config.yt_dlp_binary)
            .arg("--dump-single-json")
            .arg("--skip-download")
            .arg("--write-sub")
            .arg("--write-auto-sub")
            .arg("--sub-langs")
            .arg("en.*,en,-live_chat")
            .arg("--sub-format")
            .arg("vtt/best")
            .arg("--paths")
            .arg(temp_dir.path())
            .arg("-o")
            .arg("%(id)s.%(ext)s")
            .arg("--no-warnings")
            .arg("--")
            .arg(url)
            .output();

        let output = timeout(
            Duration::from_secs(self.config.yt_dlp_timeout_secs),
            command_future,
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "yt-dlp timed out after {} seconds",
                self.config.yt_dlp_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute yt-dlp '{}': {}",
                self.config.yt_dlp_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            let stderr_preview = if stderr.chars().count() > 280 {
                format!("{}...", stderr.chars().take(280).collect::<String>())
            } else {
                stderr
            };

            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("yt-dlp extraction failed: {}", stderr_preview),
            });
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("yt-dlp returned non-UTF8 output: {}", e),
        })?;

        let metadata: YtDlpMetadata =
            serde_json::from_str(&stdout).map_err(|e| AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("Failed to parse yt-dlp JSON output: {}", e),
            })?;

        let subtitle_transcript = tokio::task::spawn_blocking({
            let subtitles_dir = temp_dir.path().to_path_buf();
            move || Self::load_transcript_from_directory(&subtitles_dir)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to load subtitle transcript files: {}", e),
        })?;

        if let Some(transcript) = subtitle_transcript {
            return self.build_video_article_with_transcript(url, metadata, &transcript);
        }

        if self.config.enable_asr_fallback {
            match self.extract_transcript_with_asr(url, temp_dir.path()).await {
                Ok(transcript) => {
                    return self.build_video_article_with_transcript(url, metadata, &transcript);
                }
                Err(error) => {
                    warn!(url, error = %error, "ASR transcript fallback failed");
                }
            }
        }

        self.build_video_article_from_metadata(url, metadata)
    }

    async fn extract_content_with_strategies(&self, url: &str) -> Result<CleanArticle> {
        let mut article_error: Option<AppError> = None;
        let mut video_error: Option<AppError> = None;

        for strategy in self.extraction_strategies_for_url(url) {
            let result = match strategy {
                ExtractionStrategy::Article => {
                    self.article_extractor.extract_article_from_url(url).await
                }
                ExtractionStrategy::VideoMetadata => {
                    self.extract_video_metadata_with_ytdlp(url).await
                }
            };

            match result {
                Ok(article) => {
                    debug!(?strategy, url, "Web extraction strategy succeeded");
                    return Ok(article);
                }
                Err(error) => {
                    warn!(?strategy, url, error = %error, "Web extraction strategy failed");
                    match strategy {
                        ExtractionStrategy::Article => article_error = Some(error),
                        ExtractionStrategy::VideoMetadata => video_error = Some(error),
                    }
                }
            }
        }

        let preferred_error = if self.is_likely_video_url(url) {
            video_error.or(article_error)
        } else {
            article_error.or(video_error)
        };

        Err(
            preferred_error.unwrap_or_else(|| AppError::ContentExtraction {
                path: url.to_string(),
                reason: "No web extraction strategy produced content".to_string(),
            }),
        )
    }

    /// Chunk text into fixed-size chunks with overlap
    ///
    /// Uses SemanticChunker with:
    /// - Chunk size: 512 tokens
    /// - Overlap: 50 tokens
    /// - Sentence boundary preference enabled
    ///
    /// # Arguments
    ///
    /// * `text` - The text to chunk
    ///
    /// # Returns
    ///
    /// Vector of contextualized chunks with metadata
    ///
    /// # Errors
    ///
    /// - `AppError::TokenizationError` if tokenization fails
    fn chunk_text(&self, text: &str) -> Result<Vec<ContextualizedChunk>> {
        let config = ChunkerConfig {
            max_tokens: self.config.chunk_size,
            overlap_tokens: self.config.chunk_overlap,
            prefer_sentence_boundaries: self.config.prefer_sentence_boundaries,
        };

        let chunker = SemanticChunker::new(Arc::clone(&self.tokenizer), config)?;

        // Create minimal metadata for chunking
        let metadata = crate::infrastructure::indexing::metadata_extractor::DocumentMetadata {
            title: "Web Article".to_string(),
            document_type: "web".to_string(),
            page_number: None,
            section: None,
        };

        chunker.chunk_with_context(text, &metadata)
    }
}

#[async_trait]
impl WebIngestionServiceTrait for WebIngestionService {
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult> {
        // 1. Extract article/video content from URL using strategy pipeline
        let article = self.extract_content_with_strategies(url).await?;

        // 2. Archive article as markdown file in ~/.recall/web-archive
        let file_path = self
            .web_archive
            .archive_article(article.clone(), url)
            .await?;

        // 3. Chunk text content (fixed-size chunks)
        let chunks = self.chunk_text(&article.text_content)?;

        // 4. Generate embeddings
        let chunk_texts: Vec<String> = chunks
            .iter()
            .map(|c| c.contextualized_content.clone())
            .collect();

        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;

        // 5. Store document + chunks + embeddings (using the real file path)
        let stored_doc_id = self
            .index_storage
            .store_document_with_context(&file_path, "text/markdown", chunks.clone(), embeddings)
            .await?;

        // 6. Return result
        Ok(WebIngestionResult {
            document_id: stored_doc_id,
            url: url.to_string(),
            title: article.title.clone(),
            word_count: article.word_count,
            chunks_created: chunks.len(),
            site_name: None, // Not extracted by article extractor
            author: article.author,
            reading_time_minutes: Some(article.reading_time_minutes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::function_calling::dto::CleanArticle;
    use crate::infrastructure::services::traits::MockArticleExtractorService;
    use crate::shared::error::AppError;
    use std::path::{Path, PathBuf};

    struct MockWebArchiveService;

    #[async_trait]
    impl WebArchiveServiceTrait for MockWebArchiveService {
        async fn archive_article(&self, _article: CleanArticle, url: &str) -> Result<PathBuf> {
            // Return a mock path based on URL
            let domain = url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown".to_string());
            Ok(PathBuf::from(format!(
                "/mock/web-archive/{}/article.md",
                domain
            )))
        }

        async fn load_article(&self, _path: &Path) -> Result<CleanArticle> {
            unimplemented!("Not needed for ingestion tests")
        }

        async fn delete_article(&self, _path: &Path) -> Result<()> {
            unimplemented!("Not needed for ingestion tests")
        }

        async fn list_articles(&self) -> Result<Vec<PathBuf>> {
            unimplemented!("Not needed for ingestion tests")
        }
    }

    struct MockEmbeddingService;

    #[async_trait]
    impl EmbeddingServiceTrait for MockEmbeddingService {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1; 384])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1; 384]).collect())
        }

        async fn embed_contextualized_chunks(
            &self,
            chunks: &[ContextualizedChunk],
        ) -> Result<Vec<Vec<f32>>> {
            Ok(chunks.iter().map(|_| vec![0.1; 384]).collect())
        }
    }

    struct MockIndexStorage;

    #[async_trait]
    impl IndexStorageTrait for MockIndexStorage {
        async fn store_document(
            &self,
            _path: &std::path::Path,
            _mime_type: &str,
            _chunks: Vec<crate::infrastructure::indexing::chunker::TextChunk>,
            _embeddings: Vec<Vec<f32>>,
        ) -> crate::infrastructure::indexing::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn document_exists(
            &self,
            _path: &std::path::Path,
        ) -> crate::infrastructure::indexing::error::Result<bool> {
            Ok(false)
        }

        async fn get_document_by_path(
            &self,
            _path: &std::path::Path,
        ) -> crate::infrastructure::indexing::error::Result<
            Option<crate::infrastructure::indexing::storage::DocumentRecord>,
        > {
            Ok(None)
        }

        async fn needs_reindex(
            &self,
            _path: &std::path::Path,
        ) -> crate::infrastructure::indexing::error::Result<bool> {
            Ok(false)
        }

        async fn mark_document_status(
            &self,
            _path: &std::path::Path,
            _status: &str,
        ) -> crate::infrastructure::indexing::error::Result<()> {
            Ok(())
        }

        async fn remove_document(
            &self,
            _path: &std::path::Path,
        ) -> crate::infrastructure::indexing::error::Result<()> {
            Ok(())
        }

        async fn store_file_metadata_only(
            &self,
            _path: &std::path::Path,
            _file_id: &str,
            _mime_type: &str,
        ) -> crate::infrastructure::indexing::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn get_indexed_count(&self) -> crate::infrastructure::indexing::error::Result<i64> {
            Ok(0)
        }

        async fn get_total_chunks(&self) -> crate::infrastructure::indexing::error::Result<i64> {
            Ok(0)
        }

        async fn store_document_with_context(
            &self,
            _path: &std::path::Path,
            _mime_type: &str,
            _chunks: Vec<ContextualizedChunk>,
            _embeddings: Vec<Vec<f32>>,
        ) -> crate::infrastructure::indexing::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn store_document_with_context_and_file(
            &self,
            _path: &std::path::Path,
            _file_id: &str,
            _mime_type: &str,
            _chunks: Vec<ContextualizedChunk>,
            _embeddings: Vec<Vec<f32>>,
        ) -> crate::infrastructure::indexing::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn batch_store_documents(
            &self,
            _documents: Vec<(
                std::path::PathBuf,
                String,
                Vec<crate::infrastructure::indexing::chunker::TextChunk>,
                Vec<Vec<f32>>,
            )>,
        ) -> crate::infrastructure::indexing::error::Result<Vec<String>> {
            Ok(vec!["doc-123".to_string()])
        }
    }

    #[tokio::test]
    #[ignore] // Requires tokenizer vocabulary file
    async fn test_ingest_url_with_mock_article() {
        // Create service with pre-configured mock
        let mock_article_extractor = Arc::new(MockArticleExtractorService::new());
        let custom_article = CleanArticle {
            title: "Test Article Title".to_string(),
            author: Some("Test Author".to_string()),
            content: "<p>Test content</p>".to_string(),
            text_content: "Test content with enough text to create chunks. ".repeat(50),
            word_count: 300,
            reading_time_minutes: 2,
            published_date: None,
            excerpt: Some("Test excerpt".to_string()),
        };

        mock_article_extractor.set_response("https://example.com/test", custom_article);

        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);
        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let service = WebIngestionService::new(
            mock_article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            Arc::new(tokenizer),
        );

        // Ingest the URL
        let result = service
            .ingest_url("https://example.com/test")
            .await
            .unwrap();

        assert_eq!(result.url, "https://example.com/test");
        assert_eq!(result.title, "Test Article Title");
        assert_eq!(result.author, Some("Test Author".to_string()));
        assert_eq!(result.word_count, 300);
        assert!(result.chunks_created > 0);
        assert_eq!(result.reading_time_minutes, Some(2));
    }

    #[tokio::test]
    #[ignore] // Requires tokenizer vocabulary file
    async fn test_chunk_text() {
        let service = create_test_service();

        // Test with short text (should create 1 chunk)
        let short_text = "This is a short text.";
        let chunks = service.chunk_text(short_text).unwrap();
        assert_eq!(chunks.len(), 1);

        // Test with long text (should create multiple chunks)
        let long_text = "This is a sentence. ".repeat(100);
        let chunks = service.chunk_text(&long_text).unwrap();
        assert!(chunks.len() > 1);

        // Verify chunk structure
        for chunk in chunks {
            assert!(!chunk.original_content.is_empty());
            assert!(!chunk.contextualized_content.is_empty());
            assert!(chunk
                .contextualized_content
                .contains(&chunk.original_content));
        }
    }

    #[test]
    fn test_builder_missing_dependencies_returns_error() {
        let result = WebIngestionService::builder().build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(
                    false,
                    "builder unexpectedly succeeded without required dependencies"
                );
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error
            .to_string()
            .contains("missing required dependency: article_extractor"));
    }

    #[test]
    fn test_builder_builds_with_custom_config() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let service = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .chunk_size(256)
            .chunk_overlap(32)
            .prefer_sentence_boundaries(false)
            .build()
            .unwrap();

        assert_eq!(service.config.chunk_size, 256);
        assert_eq!(service.config.chunk_overlap, 32);
        assert!(!service.config.prefer_sentence_boundaries);
    }

    #[test]
    fn test_builder_rejects_invalid_chunk_config() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let result = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .chunk_size(64)
            .chunk_overlap(64)
            .build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(
                    false,
                    "builder unexpectedly accepted invalid chunk configuration"
                );
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error.to_string().contains("chunk_overlap"));
    }

    #[test]
    fn test_builder_rejects_empty_ytdlp_binary_when_enabled() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let result = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_video_fallback(true)
            .yt_dlp_binary("")
            .build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(false, "builder unexpectedly accepted empty yt-dlp binary");
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error.to_string().contains("yt_dlp_binary"));
    }

    #[test]
    fn test_video_url_detection() {
        let service = create_test_service();

        assert!(service.is_likely_video_url("https://www.youtube.com/watch?v=abc123"));
        assert!(service.is_likely_video_url("https://vimeo.com/1234"));
        assert!(service.is_likely_video_url("https://example.com/path/video.mp4"));
        assert!(!service.is_likely_video_url("https://example.com/article"));
    }

    #[test]
    fn test_extraction_strategy_when_video_fallback_disabled() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let service = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_video_fallback(false)
            .build()
            .unwrap();

        let strategies =
            service.extraction_strategies_for_url("https://www.youtube.com/watch?v=abc123");
        assert_eq!(strategies, vec![ExtractionStrategy::Article]);
    }

    #[test]
    fn test_build_video_article_from_metadata() {
        let service = create_test_service();

        let metadata = YtDlpMetadata {
            title: Some("Sample Video".to_string()),
            uploader: Some("Sample Creator".to_string()),
            description: Some(
                "This description contains enough words to produce a realistic content sample for indexing."
                    .to_string(),
            ),
            webpage_url: Some("https://video.example/watch/123".to_string()),
            upload_date: Some("20260201".to_string()),
            duration: Some(120.0),
            extractor: Some("generic".to_string()),
            ..Default::default()
        };

        let article = service
            .build_video_article_from_metadata("https://video.example/watch/123", metadata)
            .unwrap();

        assert_eq!(article.title, "Sample Video");
        assert_eq!(article.author, Some("Sample Creator".to_string()));
        assert!(article.text_content.contains("Duration: 120 seconds"));
        assert!(article.text_content.contains("Extractor: generic"));
        assert!(article.word_count > 10);
        assert!(article.published_date.is_some());
    }

    #[test]
    fn test_parse_vtt_transcript_text() {
        let service = create_test_service();
        let vtt = r#"WEBVTT

00:00:00.000 --> 00:00:01.500
Hello <c.colorE5E5E5>world</c>

00:00:01.500 --> 00:00:03.000
This is a test
"#;

        let transcript = WebIngestionService::parse_vtt_transcript_text(vtt);
        assert!(transcript.contains("Hello world"));
        assert!(transcript.contains("This is a test"));
    }

    #[test]
    fn test_build_video_article_with_transcript() {
        let service = create_test_service();

        let metadata = YtDlpMetadata {
            title: Some("Video With Transcript".to_string()),
            uploader: Some("Creator".to_string()),
            ..Default::default()
        };

        let article = service
            .build_video_article_with_transcript(
                "https://video.example/watch/123",
                metadata,
                "First sentence.\nSecond sentence.",
            )
            .unwrap();

        assert!(article.text_content.contains("Transcript:"));
        assert!(article.text_content.contains("First sentence."));
        assert!(article.content.contains("<h2>Transcript</h2>"));
        assert!(article.word_count > 5);
    }

    #[test]
    fn test_load_transcript_from_directory_prefers_english_track() {
        let service = create_test_service();
        let temp_dir = TempDirectoryGuard::new("recall-transcript-test").unwrap();

        let non_english = temp_dir.path().join("video.es.vtt");
        let english = temp_dir.path().join("video.en.vtt");

        fs::write(
            &non_english,
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHola mundo",
        )
        .unwrap();
        fs::write(
            &english,
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello world from the english transcript with many extra words so the parser keeps this caption track and uses it for robust indexing coverage.",
        )
        .unwrap();

        let transcript =
            WebIngestionService::load_transcript_from_directory(temp_dir.path()).unwrap();
        assert!(transcript.contains("Hello world"));
    }

    #[test]
    fn test_extraction_strategy_order_prefers_video_for_video_urls() {
        let service = create_test_service();

        let video = service.extraction_strategies_for_url("https://www.youtube.com/watch?v=abc123");
        let article = service.extraction_strategies_for_url("https://example.com/blog/post");

        assert_eq!(
            video,
            vec![
                ExtractionStrategy::VideoMetadata,
                ExtractionStrategy::Article
            ]
        );
        assert_eq!(article, vec![ExtractionStrategy::Article]);
    }

    #[test]
    fn test_builder_rejects_zero_ytdlp_timeout_when_enabled() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let result = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_video_fallback(true)
            .yt_dlp_timeout_secs(0)
            .build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(false, "builder unexpectedly accepted zero yt-dlp timeout");
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error.to_string().contains("yt_dlp_timeout_secs"));
    }

    #[test]
    fn test_builder_rejects_empty_asr_binary_when_enabled() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let result = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_asr_fallback(true)
            .asr_binary("")
            .build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(false, "builder unexpectedly accepted empty ASR binary");
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error.to_string().contains("asr_binary"));
    }

    #[test]
    fn test_builder_rejects_zero_asr_timeout_when_enabled() {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let result = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_asr_fallback(true)
            .asr_timeout_secs(0)
            .build();

        assert!(result.is_err());
        let error = match result {
            Ok(_) => {
                assert!(false, "builder unexpectedly accepted zero ASR timeout");
                return;
            }
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidConfig(_)));
        assert!(error.to_string().contains("asr_timeout_secs"));
    }

    #[test]
    fn test_find_best_audio_file_prefers_wav() {
        let service = create_test_service();
        let temp_dir = TempDirectoryGuard::new("recall-audio-select-test").unwrap();

        let mp3 = temp_dir.path().join("audio.mp3");
        let wav = temp_dir.path().join("audio.wav");
        fs::write(&mp3, "dummy").unwrap();
        fs::write(&wav, "dummy").unwrap();

        let selected = WebIngestionService::find_best_audio_file(temp_dir.path()).unwrap();
        assert_eq!(selected.extension().and_then(|v| v.to_str()), Some("wav"));
    }

    fn create_test_service() -> WebIngestionService {
        let article_extractor = Arc::new(MockArticleExtractorService::new());
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn test_non_video_urls_prefer_article_error_when_all_strategies_fail() {
        let article_extractor = Arc::new(MockArticleExtractorService::new_disabled(
            "article extractor unavailable".to_string(),
        ));
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let service = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_video_fallback(true)
            .yt_dlp_binary("yt-dlp-command-that-does-not-exist")
            .build()
            .unwrap();

        let error = service
            .extract_content_with_strategies("https://example.com/article")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("article extractor unavailable"));
    }

    #[tokio::test]
    async fn test_likely_video_urls_prefer_video_error_when_all_strategies_fail() {
        let article_extractor = Arc::new(MockArticleExtractorService::new_disabled(
            "article extractor unavailable".to_string(),
        ));
        let web_archive = Arc::new(MockWebArchiveService);
        let embedding_service = Arc::new(MockEmbeddingService);
        let index_storage = Arc::new(MockIndexStorage);

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Tokenizer::new(wp);

        let service = WebIngestionService::builder()
            .article_extractor(article_extractor)
            .web_archive(web_archive)
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(Arc::new(tokenizer))
            .enable_video_fallback(true)
            .yt_dlp_binary("yt-dlp-command-that-does-not-exist")
            .build()
            .unwrap();

        let error = service
            .extract_content_with_strategies("https://www.youtube.com/watch?v=abc123")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("Failed to execute yt-dlp"));
    }
}
