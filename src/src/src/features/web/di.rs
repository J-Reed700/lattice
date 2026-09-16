//! Web feature dependency injection.

use std::path::Path;
use std::sync::Arc;

use crate::features::embedding::service::DynamicEmbeddingService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::indexing::engine::storage::IndexStorage;
use crate::features::indexing::IndexStorageTrait;
use crate::features::web::services::ingestion::WebIngestionService;
use crate::features::web::use_cases::{GetUrlPreviewUseCase, IngestWebUrlUseCase};
use crate::features::web::{
    WebArchiveServiceTrait, WebCaptureServiceTrait, WebIngestionServiceTrait,
};
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::infrastructure::services::{
    ArticleExtractorService, WebArchiveService, WebCaptureService,
};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use tokenizers::Tokenizer;

#[derive(Clone)]
pub struct WebDi {
    pub web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
    pub web_capture_service: Arc<dyn WebCaptureServiceTrait>,
    pub article_extractor_service: Arc<dyn ArticleExtractorServiceTrait>,
    pub web_archive: Arc<dyn WebArchiveServiceTrait>,
    pub ingest_web_url_use_case: Arc<IngestWebUrlUseCase>,
    pub get_url_preview_use_case: Arc<GetUrlPreviewUseCase>,
}

pub fn build(
    db_pool: SqlitePool,
    model_dir: &Path,
    model_provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
) -> Result<WebDi> {
    let web_capture_service =
        Arc::new(WebCaptureService::new()?) as Arc<dyn WebCaptureServiceTrait>;
    let article_extractor_service =
        Arc::new(ArticleExtractorService::new()?) as Arc<dyn ArticleExtractorServiceTrait>;
    let web_archive = Arc::new(
        WebArchiveService::new()
            .map_err(|e| AppError::Other(format!("Failed to create web archive service: {}", e)))?,
    ) as Arc<dyn WebArchiveServiceTrait>;

    let tokenizer = match crate::infrastructure::setup::setup_tokenizer(model_dir) {
        Some(tokenizer) => tokenizer,
        None => build_fallback_tokenizer()?,
    };

    let embedding_service =
        Arc::new(DynamicEmbeddingService::new(model_provider)) as Arc<dyn EmbeddingServiceTrait>;
    let index_storage = Arc::new(IndexStorage::new(db_pool)) as Arc<dyn IndexStorageTrait>;

    let web_ingestion_service = Arc::new(
        WebIngestionService::builder()
            .article_extractor(article_extractor_service.clone())
            .web_archive(web_archive.clone())
            .embedding_service(embedding_service)
            .index_storage(index_storage)
            .tokenizer(tokenizer)
            .build()?,
    ) as Arc<dyn WebIngestionServiceTrait>;

    Ok(WebDi {
        ingest_web_url_use_case: Arc::new(IngestWebUrlUseCase::new(web_ingestion_service.clone())),
        get_url_preview_use_case: Arc::new(GetUrlPreviewUseCase::new(web_capture_service.clone())),
        web_ingestion_service,
        web_capture_service,
        article_extractor_service,
        web_archive,
    })
}

/// Minimal BPE tokenizer used when no real tokenizer is available (tests /
/// first-run before model download). Avoids network fetches and provides
/// deterministic tokenization.
fn build_fallback_tokenizer() -> Result<Arc<Tokenizer>> {
    use std::collections::HashMap;
    use tokenizers::models::bpe::BPE;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    let mut vocab = HashMap::new();
    for c in b'a'..=b'z' {
        vocab.insert((c as char).to_string(), c as u32);
    }
    for c in b'A'..=b'Z' {
        vocab.insert((c as char).to_string(), (c + 26) as u32);
    }
    for c in b'0'..=b'9' {
        vocab.insert((c as char).to_string(), (c + 52) as u32);
    }
    vocab.insert(" ".to_string(), 62);
    vocab.insert(".".to_string(), 63);
    vocab.insert(",".to_string(), 64);
    vocab.insert("-".to_string(), 65);
    vocab.insert("[UNK]".to_string(), 66);

    let merges = vec![];
    let bpe = BPE::builder()
        .vocab_and_merges(vocab, merges)
        .unk_token("[UNK]".to_string())
        .build()
        .map_err(|error| AppError::Other(format!("Failed to build fallback tokenizer: {error}")))?;

    let mut tokenizer = Tokenizer::new(bpe);
    tokenizer.with_pre_tokenizer(Whitespace {});
    Ok(Arc::new(tokenizer))
}

/// Web's registrar surface on `Container`.
impl Container {
    pub fn ingest_web_url_use_case(&self) -> Arc<IngestWebUrlUseCase> {
        Arc::clone(self.indexing.ingest_web_url_use_case())
    }

    pub fn get_url_preview_use_case(&self) -> Arc<GetUrlPreviewUseCase> {
        Arc::clone(self.indexing.get_url_preview_use_case())
    }

    /// Get web archive service (from IndexingModule)
    pub fn web_archive(&self) -> Arc<dyn WebArchiveServiceTrait> {
        Arc::clone(self.indexing.web_archive())
    }

    pub fn article_extractor_service(&self) -> Arc<dyn ArticleExtractorServiceTrait> {
        Arc::clone(self.indexing.article_extractor_service())
    }
}
