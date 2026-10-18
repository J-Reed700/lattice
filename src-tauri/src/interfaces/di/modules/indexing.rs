//! The indexing module: indexing files and web pages, transcription and OCR.

use super::CoreModule;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - Indexing
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};

// Application Use Cases - Web
use crate::features::web::use_cases::{GetUrlPreviewUseCase, IngestWebUrlUseCase};

// Application Ports
use lattice_core::application::ports::{
    ChunkRepositoryPort, DocumentRepository, EmbeddingRepositoryPort, FileStoragePort,
    VectorSearchPort,
};

// Embedding and storage
use crate::features::embedding::repository::EmbeddingRepository;
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::indexing::di::IndexingPorts;
use crate::features::indexing::engine::storage::IndexStorage;
use crate::features::search::engine::sparse_search::SqliteSparseTermStore;

// Service Traits
use crate::features::web::{
    ArticleExtractorServiceTrait, WebArchiveServiceTrait, WebCaptureServiceTrait,
    WebIngestionServiceTrait,
};

/// Indexing module — hollow composition root over the indexing and web
/// feature slices. Shares `vector_search` with [`SearchModule`] so
/// index-time writes reach the same USearch instance queried at search time.
#[derive(Clone)]
pub struct IndexingModule {
    indexing: crate::features::indexing::di::IndexingDi,
    web: crate::features::web::di::WebDi,
    /// Page OCR for scanned PDFs: the bundled `llama-server` with the
    /// catalog OCR model, started only when a scanned page needs it. Until
    /// that model is downloaded it answers `NotConfigured` like `NoopOcr`:
    /// scanned pages stay in `ExtractedContent::needs_ocr` and the rest of
    /// every document is still indexed.
    ocr: Arc<dyn lattice_core::application::ports::OcrPort>,
    /// Stored chunk vectors, the same repository the indexer writes.
    embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
}

impl IndexingModule {
    /// Build IndexingModule by composing the indexing and web features.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        model_provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        shared: crate::features::indexing::di::IndexingShared,
    ) -> lattice_core::shared::error::Result<Self> {
        let model_dir = core.data_dir().join("models");
        // Web first: deletion routes archived articles back through the very
        // archive service that wrote them, so indexing needs that instance.
        let web = crate::features::web::di::build(
            &model_dir,
            Arc::new(DynamicEmbedding::new(Arc::clone(&model_provider))),
            Arc::new(IndexStorage::new(db_pool.clone())),
            Arc::clone(&vector_search),
            Arc::clone(&shared.query_cache),
        )?;
        let ocr = crate::features::ocr::di::build(
            Arc::new(
                crate::features::download::downloaded_model_repository::DownloadedModelRepository::new(
                    db_pool.clone(),
                ),
            ),
            Arc::clone(core.background()),
        );
        let embedding_repo =
            Arc::new(EmbeddingRepository::new(db_pool.clone())) as Arc<dyn EmbeddingRepositoryPort>;
        let ports = IndexingPorts {
            embedding: Arc::new(DynamicEmbedding::new(model_provider)),
            embedding_repo: Arc::clone(&embedding_repo),
            // Learned sparse postings live beside the dense vectors. Wired
            // unconditionally: `IndexFileUseCase` only reads a sparse head when
            // the loaded model actually has one, so this is inert for a
            // dense-only model.
            sparse_term_store: Arc::new(SqliteSparseTermStore::new(db_pool.clone())),
            // Built once: the extraction adapter and the transcribe command
            // share this engine.
            transcription: crate::features::transcription::di::build(
                db_pool.clone(),
                Arc::clone(core.background()),
            )
            .port,
        };
        let indexing = crate::features::indexing::di::build(
            db_pool,
            ports,
            vector_search,
            web.web_archive.clone(),
            Arc::clone(&ocr),
            shared,
        )?;
        Ok(Self {
            indexing,
            web,
            ocr,
            embedding_repo,
        })
    }

    /// Stored chunk vectors, for a feature that reads them back.
    pub fn embedding_repository(&self) -> &Arc<dyn EmbeddingRepositoryPort> {
        &self.embedding_repo
    }

    // Indexing use case getters
    pub fn index_file_use_case(&self) -> &Arc<IndexFileUseCase> {
        &self.indexing.index_file_use_case
    }

    pub fn index_directory_use_case(&self) -> &Arc<IndexDirectoryUseCase> {
        &self.indexing.index_directory_use_case
    }

    pub fn reindex_document_use_case(&self) -> &Arc<ReindexDocumentUseCase> {
        &self.indexing.reindex_document_use_case
    }

    pub fn delete_document_use_case(&self) -> &Arc<DeleteDocumentUseCase> {
        &self.indexing.delete_document_use_case
    }

    pub fn rename_document_use_case(&self) -> &Arc<RenameDocumentUseCase> {
        &self.indexing.rename_document_use_case
    }

    /// On-device speech-to-text, shared with the content-extraction adapter.
    pub fn transcription_port(
        &self,
    ) -> &Arc<dyn lattice_core::application::ports::TranscriptionPort> {
        &self.indexing.transcription
    }

    /// Page OCR for scanned PDFs, the same port the indexer's extractor uses.
    pub fn ocr_port(&self) -> &Arc<dyn lattice_core::application::ports::OcrPort> {
        &self.ocr
    }

    // Web use case getters
    pub fn ingest_web_url_use_case(&self) -> &Arc<IngestWebUrlUseCase> {
        &self.web.ingest_web_url_use_case
    }

    pub fn get_url_preview_use_case(&self) -> &Arc<GetUrlPreviewUseCase> {
        &self.web.get_url_preview_use_case
    }

    // Service getters
    pub fn indexing_state(&self) -> &Arc<crate::features::indexing::engine::IndexingState> {
        &self.indexing.indexing_state
    }

    /// Collector for unreferenced blobs in the imported-file library.
    pub fn library_gc(&self) -> &Arc<crate::features::indexing::LibraryGc> {
        &self.indexing.library_gc
    }

    pub fn web_ingestion_service(&self) -> &Arc<dyn WebIngestionServiceTrait> {
        &self.web.web_ingestion_service
    }

    pub fn web_capture_service(&self) -> &Arc<dyn WebCaptureServiceTrait> {
        &self.web.web_capture_service
    }

    pub fn article_extractor_service(&self) -> &Arc<dyn ArticleExtractorServiceTrait> {
        &self.web.article_extractor_service
    }

    pub fn web_archive(&self) -> &Arc<dyn WebArchiveServiceTrait> {
        &self.web.web_archive
    }

    pub fn file_storage(&self) -> &Arc<dyn FileStoragePort> {
        &self.indexing.file_storage
    }

    pub fn chunk_repository(&self) -> &Arc<dyn ChunkRepositoryPort> {
        &self.indexing.chunk_repo
    }

    /// Document rows: the sink adapter needs the same repository the
    /// use cases above use, so a mirrored note resolves to the document
    /// a user-initiated delete would have found.
    pub fn document_repo(&self) -> &Arc<dyn DocumentRepository> {
        &self.indexing.document_repo
    }

    /// Opens the transactions that commit an imported document whole.
    pub fn uow_factory(&self) -> &Arc<dyn lattice_core::application::ports::UnitOfWorkFactory> {
        &self.indexing.uow_factory
    }
}
