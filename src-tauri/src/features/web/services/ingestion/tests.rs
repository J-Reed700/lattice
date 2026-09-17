//! Tests for the web ingestion service.

use super::extract::{ExtractionStrategy, YtDlpMetadata};
use super::fetch::TempDirectoryGuard;
use super::WebIngestionService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::function_calling::dto::CleanArticle;
use crate::features::indexing::engine::chunker::ContextualizedChunk;
use crate::features::indexing::IndexStorageTrait;
use crate::features::web::{WebArchiveServiceTrait, WebIngestionServiceTrait};
use crate::infrastructure::services::traits::MockArticleExtractorService;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokenizers::Tokenizer;

struct MockWebArchiveService;

#[async_trait]
impl WebArchiveServiceTrait for MockWebArchiveService {
    async fn archive_article(&self, _article: CleanArticle, url: &str) -> Result<PathBuf> {
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

    fn owns(&self, path: &Path) -> bool {
        path.starts_with("/mock/web-archive")
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
        _chunks: Vec<crate::features::indexing::engine::chunker::TextChunk>,
        _embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        Ok("doc-123".to_string())
    }

    async fn document_exists(
        &self,
        _path: &std::path::Path,
    ) -> crate::features::indexing::engine::error::Result<bool> {
        Ok(false)
    }

    async fn get_document_by_path(
        &self,
        _path: &std::path::Path,
    ) -> crate::features::indexing::engine::error::Result<
        Option<crate::features::indexing::engine::storage::DocumentRecord>,
    > {
        Ok(None)
    }

    async fn needs_reindex(
        &self,
        _path: &std::path::Path,
    ) -> crate::features::indexing::engine::error::Result<bool> {
        Ok(false)
    }

    async fn mark_document_status(
        &self,
        _path: &std::path::Path,
        _status: &str,
    ) -> crate::features::indexing::engine::error::Result<()> {
        Ok(())
    }

    async fn remove_document(
        &self,
        _path: &std::path::Path,
    ) -> crate::features::indexing::engine::error::Result<()> {
        Ok(())
    }

    async fn store_file_metadata_only(
        &self,
        _path: &std::path::Path,
        _file_id: &str,
        _mime_type: &str,
    ) -> crate::features::indexing::engine::error::Result<String> {
        Ok("doc-123".to_string())
    }

    async fn get_indexed_count(&self) -> crate::features::indexing::engine::error::Result<i64> {
        Ok(0)
    }

    async fn get_total_chunks(&self) -> crate::features::indexing::engine::error::Result<i64> {
        Ok(0)
    }

    async fn store_document_with_context(
        &self,
        _path: &std::path::Path,
        _mime_type: &str,
        _chunks: Vec<ContextualizedChunk>,
        _embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        Ok("doc-123".to_string())
    }

    async fn store_document_with_context_and_file(
        &self,
        _path: &std::path::Path,
        _file_id: &str,
        _mime_type: &str,
        _chunks: Vec<ContextualizedChunk>,
        _embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        Ok("doc-123".to_string())
    }

    async fn batch_store_documents(
        &self,
        _documents: Vec<(
            std::path::PathBuf,
            String,
            Vec<crate::features::indexing::engine::chunker::TextChunk>,
            Vec<Vec<f32>>,
        )>,
    ) -> crate::features::indexing::engine::error::Result<Vec<String>> {
        Ok(vec!["doc-123".to_string()])
    }
}

#[tokio::test]
#[ignore = "requires tokenizer vocabulary file"]
async fn test_ingest_url_with_mock_article() {
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
#[ignore = "requires tokenizer vocabulary file"]
async fn test_chunk_text() {
    let service = create_test_service();

    let short_text = "This is a short text.";
    let chunks = service.chunk_text(short_text).unwrap();
    assert_eq!(chunks.len(), 1);

    let long_text = "This is a sentence. ".repeat(100);
    let chunks = service.chunk_text(&long_text).unwrap();
    assert!(chunks.len() > 1);

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
            panic!("builder unexpectedly succeeded without required dependencies");
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
            panic!("builder unexpectedly accepted invalid chunk configuration");
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
            panic!("builder unexpectedly accepted empty yt-dlp binary");
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
    let _service = create_test_service();
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
    let _service = create_test_service();
    let temp_dir = TempDirectoryGuard::new("lattice-transcript-test").unwrap();

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

    let transcript = WebIngestionService::load_transcript_from_directory(temp_dir.path()).unwrap();
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
            panic!("builder unexpectedly accepted zero yt-dlp timeout");
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
            panic!("builder unexpectedly accepted empty ASR binary");
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
            panic!("builder unexpectedly accepted zero ASR timeout");
        }
        Err(error) => error,
    };
    assert!(matches!(error, AppError::InvalidConfig(_)));
    assert!(error.to_string().contains("asr_timeout_secs"));
}

#[test]
fn test_find_best_audio_file_prefers_wav() {
    let _service = create_test_service();
    let temp_dir = TempDirectoryGuard::new("lattice-audio-select-test").unwrap();

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

/// Mirrors `embedding_input::late_chunking_falls_back_to_the_identical_per_chunk_inputs`
/// for the web importer: both strategies must hand the model the same text and
/// store the same chunks, so the two paths land in one vector space.
mod late_chunking {
    use super::*;
    use std::sync::Mutex;

    /// Splits at a fixed byte width and records every text handed to the model.
    struct RecordingEmbeddingService {
        late: bool,
        width: usize,
        embedded: Mutex<Vec<String>>,
    }

    impl RecordingEmbeddingService {
        fn new(late: bool, width: usize) -> Self {
            Self {
                late,
                width,
                embedded: Mutex::new(Vec::new()),
            }
        }
        fn embedded(&self) -> Vec<String> {
            self.embedded.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl EmbeddingServiceTrait for RecordingEmbeddingService {
        async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
            let mut out = self.embed_batch(&[text.to_owned()]).await?;
            Ok(out.remove(0))
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            self.embedded
                .lock()
                .expect("lock")
                .extend(texts.iter().cloned());
            Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
        }

        async fn embed_contextualized_chunks(
            &self,
            chunks: &[ContextualizedChunk],
        ) -> Result<Vec<Vec<f32>>> {
            let texts: Vec<String> = chunks
                .iter()
                .map(|c| c.contextualized_content.clone())
                .collect();
            self.embed_batch(&texts).await
        }

        fn split_text(
            &self,
            text: &str,
            _prefix: &str,
        ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
            let mut chunks = Vec::new();
            let mut start = 0;
            while start < text.len() {
                let mut end = (start + self.width).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                chunks.push(
                    crate::application::ports::embedding_port::EmbeddingTextChunk {
                        text: text[start..end].to_owned(),
                        start,
                        end,
                        token_count: 1,
                    },
                );
                start = end;
            }
            Ok(chunks)
        }

        fn uses_late_chunking(&self) -> bool {
            self.late
        }
    }

    /// Records exactly what reached storage, so the two strategies can be
    /// compared on citation evidence and not only on vectors.
    #[derive(Default)]
    struct RecordingStorage {
        stored: Mutex<Vec<ContextualizedChunk>>,
        vectors: Mutex<Vec<Vec<f32>>>,
    }

    #[async_trait]
    impl IndexStorageTrait for RecordingStorage {
        async fn store_document_with_context(
            &self,
            _path: &std::path::Path,
            _mime_type: &str,
            chunks: Vec<ContextualizedChunk>,
            embeddings: Vec<Vec<f32>>,
        ) -> crate::features::indexing::engine::error::Result<String> {
            *self.stored.lock().expect("lock") = chunks;
            *self.vectors.lock().expect("lock") = embeddings;
            Ok("doc-123".to_string())
        }

        async fn store_document(
            &self,
            _path: &std::path::Path,
            _mime_type: &str,
            _chunks: Vec<crate::features::indexing::engine::chunker::TextChunk>,
            _embeddings: Vec<Vec<f32>>,
        ) -> crate::features::indexing::engine::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn document_exists(
            &self,
            _path: &std::path::Path,
        ) -> crate::features::indexing::engine::error::Result<bool> {
            Ok(false)
        }

        async fn get_document_by_path(
            &self,
            _path: &std::path::Path,
        ) -> crate::features::indexing::engine::error::Result<
            Option<crate::features::indexing::engine::storage::DocumentRecord>,
        > {
            Ok(None)
        }

        async fn needs_reindex(
            &self,
            _path: &std::path::Path,
        ) -> crate::features::indexing::engine::error::Result<bool> {
            Ok(false)
        }

        async fn mark_document_status(
            &self,
            _path: &std::path::Path,
            _status: &str,
        ) -> crate::features::indexing::engine::error::Result<()> {
            Ok(())
        }

        async fn remove_document(
            &self,
            _path: &std::path::Path,
        ) -> crate::features::indexing::engine::error::Result<()> {
            Ok(())
        }

        async fn store_file_metadata_only(
            &self,
            _path: &std::path::Path,
            _file_id: &str,
            _mime_type: &str,
        ) -> crate::features::indexing::engine::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn get_indexed_count(&self) -> crate::features::indexing::engine::error::Result<i64> {
            Ok(0)
        }

        async fn get_total_chunks(&self) -> crate::features::indexing::engine::error::Result<i64> {
            Ok(0)
        }

        async fn store_document_with_context_and_file(
            &self,
            _path: &std::path::Path,
            _file_id: &str,
            _mime_type: &str,
            _chunks: Vec<ContextualizedChunk>,
            _embeddings: Vec<Vec<f32>>,
        ) -> crate::features::indexing::engine::error::Result<String> {
            Ok("doc-123".to_string())
        }

        async fn batch_store_documents(
            &self,
            _documents: Vec<(
                std::path::PathBuf,
                String,
                Vec<crate::features::indexing::engine::chunker::TextChunk>,
                Vec<Vec<f32>>,
            )>,
        ) -> crate::features::indexing::engine::error::Result<Vec<String>> {
            Ok(vec!["doc-123".to_string()])
        }
    }

    const TEXT: &str = "First body line here. Second body line here. Third body line here.";

    async fn ingest(late: bool) -> (Vec<String>, Vec<ContextualizedChunk>, Vec<Vec<f32>>) {
        let extractor = Arc::new(MockArticleExtractorService::new());
        extractor.set_response(
            "https://example.com/article",
            CleanArticle {
                title: "Article".to_string(),
                author: None,
                content: String::new(),
                text_content: TEXT.to_string(),
                word_count: 12,
                reading_time_minutes: 1,
                published_date: None,
                excerpt: None,
            },
        );
        let embedding_service = Arc::new(RecordingEmbeddingService::new(late, 12));
        let storage = Arc::new(RecordingStorage::default());
        use tokenizers::models::wordpiece::WordPiece;
        let service = WebIngestionService::new(
            extractor,
            Arc::new(MockWebArchiveService),
            embedding_service.clone(),
            storage.clone(),
            Arc::new(Tokenizer::new(WordPiece::default())),
        );

        service
            .ingest_url("https://example.com/article")
            .await
            .expect("ingest");

        let stored = storage.stored.lock().expect("lock").clone();
        let vectors = storage.vectors.lock().expect("lock").clone();
        (embedding_service.embedded(), stored, vectors)
    }

    fn citations(chunks: &[ContextualizedChunk]) -> Vec<(String, usize, usize)> {
        chunks
            .iter()
            .map(|c| (c.original_content.clone(), c.start_idx, c.end_idx))
            .collect()
    }

    #[tokio::test]
    async fn both_strategies_embed_the_same_text_and_store_the_same_chunks() {
        let (chunk_first_inputs, chunk_first_chunks, chunk_first_vectors) = ingest(false).await;
        let (late_inputs, late_chunks, late_vectors) = ingest(true).await;

        assert!(chunk_first_chunks.len() > 1, "the text should split");
        assert_eq!(chunk_first_inputs, late_inputs);
        assert_eq!(chunk_first_vectors, late_vectors);
        // Citation evidence is untouched by the strategy.
        assert_eq!(citations(&chunk_first_chunks), citations(&late_chunks));
    }

    #[test]
    fn a_span_addresses_every_chunk_exactly_and_carries_the_prefix_once() {
        let chunks = vec![
            ContextualizedChunk {
                contextualized_content: "[Document: Web Article]\n\nFirst ".to_string(),
                original_content: "First ".to_string(),
                context_prefix: "[Document: Web Article]".to_string(),
                chunk_index: 0,
                token_count: 1,
                start_idx: 0,
                end_idx: 6,
            },
            ContextualizedChunk {
                contextualized_content: "[Document: Web Article]\n\nsecond".to_string(),
                original_content: "second".to_string(),
                context_prefix: "[Document: Web Article]".to_string(),
                chunk_index: 1,
                token_count: 1,
                start_idx: 6,
                end_idx: 12,
            },
        ];
        let span = WebIngestionService::span_for_chunks("First second", &chunks).expect("span");
        let prefix_end = span.chunk_ranges.first().expect("a range").start;
        let prefix = span.span_text.get(..prefix_end).expect("prefix");
        for (chunk, range) in chunks.iter().zip(&span.chunk_ranges) {
            let sliced = span.span_text.get(range.clone()).expect("range");
            assert_eq!(sliced, chunk.original_content);
            assert_eq!(
                format!("{prefix}{sliced}"),
                chunk.contextualized_content,
                "the span must reproduce the chunk-first input exactly"
            );
        }
        assert_eq!(span.first_chunk_index, 0);

        // Chunks that do not tile the text cannot be pooled from one pass.
        assert!(WebIngestionService::span_for_chunks("First second!", &chunks).is_none());
        assert!(WebIngestionService::span_for_chunks("", &[]).is_none());
    }
}
