//! Web feature dependency injection.

use std::path::Path;
use std::sync::Arc;

use crate::features::embedding::service::DynamicEmbeddingService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::indexing::engine::storage::IndexStorage;
use crate::features::indexing::IndexStorageTrait;
use crate::features::web::services::archive::WebArchiveService;
use crate::features::web::services::article_extractor::ArticleExtractorService;
use crate::features::web::services::capture::WebCaptureService;
use crate::features::web::services::ingestion::WebIngestionService;
use crate::features::web::traits::ArticleExtractorServiceTrait;
use crate::features::web::use_cases::{GetUrlPreviewUseCase, IngestWebUrlUseCase};
use crate::features::web::{
    WebArchiveServiceTrait, WebCaptureServiceTrait, WebIngestionServiceTrait,
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
    vector_search: Arc<dyn crate::application::ports::VectorSearchPort>,
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
            .vector_search(vector_search)
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
    for (id, c) in (b'a'..=b'z')
        .chain(b'A'..=b'Z')
        .chain(b'0'..=b'9')
        .enumerate()
    {
        vocab.insert((c as char).to_string(), id as u32);
    }
    vocab.insert(" ".to_string(), 62);
    vocab.insert(".".to_string(), 63);
    vocab.insert(",".to_string(), 64);
    vocab.insert("-".to_string(), 65);
    vocab.insert("[UNK]".to_string(), 66);

    let merges = vec![];
    let bpe = BPE::builder()
        .vocab_and_merges(vocab, merges)
        // This character fallback has no merges to memoize. The default
        // 10,000-entry BPE cache reserves ~800 KiB before its first use.
        .cache_capacity(0)
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

    /// The web service the model's tools use, so the reader reads a cited page
    /// through exactly the same path — and out of the same cache — as the turn
    /// that cited it.
    pub fn web_service(&self) -> Arc<crate::features::web::services::web::WebService> {
        Arc::clone(&self.web_service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_vocabulary_has_unique_ids_and_preserves_character_tokens() {
        let tokenizer = build_fallback_tokenizer().unwrap();
        let vocab = tokenizer.get_vocab(false);
        let ids: std::collections::HashSet<_> = vocab.values().collect();
        assert_eq!(
            ids.len(),
            vocab.len(),
            "Reverse token lookup must be unambiguous"
        );
        let encoded = tokenizer.encode("aAGgZz09", false).unwrap();
        assert_eq!(
            encoded.get_tokens(),
            &["a", "A", "G", "g", "Z", "z", "0", "9"]
        );
    }
}
