//! An archived article is written before the row that names it, and goes
//! again when that row cannot be written.
use super::WebIngestionService;
use crate::features::web::traits::MockArticleExtractorService;
use crate::features::web::{WebArchiveServiceTrait, WebIngestionServiceTrait};
use async_trait::async_trait;
use lattice_core::application::contracts::web::CleanArticle;
use lattice_core::application::ports::{EmbeddingPort, IngestedDocumentStorePort};
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::domain::value_objects::ContextualizedChunk;
use lattice_core::shared::error::{AppError, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const URL: &str = "https://example.org/essay";

#[derive(Default)]
struct RecordingArchive {
    live: Mutex<Vec<PathBuf>>,
}

#[async_trait]
impl ArticleArchive for RecordingArchive {
    async fn delete_article(&self, path: &Path) -> Result<()> {
        self.live.lock().unwrap().retain(|live| live != path);
        Ok(())
    }

    fn owns(&self, path: &Path) -> bool {
        path.starts_with("/archive")
    }
}

#[async_trait]
impl WebArchiveServiceTrait for RecordingArchive {
    async fn archive_article(&self, _article: CleanArticle, _url: &str) -> Result<PathBuf> {
        let mut live = self.live.lock().unwrap();
        let path = PathBuf::from(format!("/archive/example.org/{}/article.md", live.len()));
        live.push(path.clone());
        Ok(path)
    }

    async fn load_article(&self, _path: &Path) -> Result<CleanArticle> {
        Err(AppError::NotFound("not needed".into()))
    }

    async fn list_articles(&self) -> Result<Vec<PathBuf>> {
        Ok(self.live.lock().unwrap().clone())
    }
}

struct Embedder;

#[async_trait]
impl EmbeddingPort for Embedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1; 4])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1; 4]).collect())
    }

    fn dimension(&self) -> usize {
        4
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct RefusingStore;

#[async_trait]
impl IngestedDocumentStorePort for RefusingStore {
    async fn store_document_with_context_for_model(
        &self,
        _path: &Path,
        _mime_type: &str,
        _chunks: Vec<ContextualizedChunk>,
        _embeddings: Vec<Vec<f32>>,
        _model_identity: &str,
    ) -> Result<String> {
        Err(AppError::Database("injected".into()))
    }

    async fn chunk_ids_in_order(&self, _document_id: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn an_article_whose_document_cannot_be_stored_leaves_no_archived_file() {
    let extractor = Arc::new(MockArticleExtractorService::new());
    extractor.set_response(
        URL,
        CleanArticle {
            title: "Essay".to_string(),
            author: None,
            content: "<p>Body</p>".to_string(),
            text_content: "A paragraph with enough words to make one chunk.".to_string(),
            word_count: 9,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: None,
        },
    );
    let archive = Arc::new(RecordingArchive::default());
    let service = WebIngestionService::new(
        extractor,
        archive.clone(),
        Arc::new(Embedder),
        Arc::new(RefusingStore),
    );

    let result = service.ingest_url(URL).await;

    assert!(result.is_err());
    assert!(archive.list_articles().await.unwrap().is_empty());
}
