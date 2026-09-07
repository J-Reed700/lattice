//! Indexing feature dependency injection.

use std::sync::{Arc, RwLock};

use sqlx::SqlitePool;

use crate::application::ports::{
    BatchJobRepositoryPort, ChunkRepositoryPort, ContentAddressedStoragePort,
    ContentExtractionPort, DocumentRepository, DocumentRepositoryPort, EmbeddingPort,
    EmbeddingRepositoryPort, FileStoragePort, FileSystemPort, TranscriptionPort, VectorSearchPort,
};
use crate::domain::repositories::UnitOfWorkFactory;
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};
use crate::features::indexing::IndexingServiceTrait;
use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
use crate::infrastructure::file_system::{FileSystemAdapter, SecureFileStorage};
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use crate::infrastructure::persistence::repositories::{
    BatchJobRepository, ChunkRepositoryImpl, DocumentRepositoryImpl, EmbeddingRepository,
};
use crate::infrastructure::storage::content_addressed_storage::ContentAddressedStorage;
use crate::shared::error::Result;

#[derive(Clone)]
pub struct IndexingDi {
    pub indexing_service: Arc<dyn IndexingServiceTrait>,
    pub indexing_state: Arc<crate::infrastructure::indexing::IndexingState>,

    pub index_file_use_case: Arc<IndexFileUseCase>,
    pub index_directory_use_case: Arc<IndexDirectoryUseCase>,
    pub reindex_document_use_case: Arc<ReindexDocumentUseCase>,
    pub delete_document_use_case: Arc<DeleteDocumentUseCase>,
    pub rename_document_use_case: Arc<RenameDocumentUseCase>,

    // Shared state exposed so BatchDi can reuse it
    pub batch_job_repo: Arc<dyn BatchJobRepositoryPort>,
    pub chunk_repo: Arc<dyn ChunkRepositoryPort>,
    pub document_repo: Arc<dyn DocumentRepository>,
    pub file_storage: Arc<dyn FileStoragePort>,
    pub file_system: Arc<dyn FileSystemPort>,
    pub uow_factory: Arc<dyn UnitOfWorkFactory>,

    /// On-device speech-to-text, shared with the content-extraction adapter so
    /// audio ingest and the explicit `transcribe_file` command use one engine.
    pub transcription: Arc<dyn TranscriptionPort>,
}

pub fn build(
    db_pool: SqlitePool,
    embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
    vector_search: Arc<dyn VectorSearchPort>,
) -> Result<IndexingDi> {
    use crate::infrastructure::setup::degraded_mocks;

    let document_repo =
        Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;
    let batch_job_repo =
        Arc::new(BatchJobRepository::new(db_pool.clone())) as Arc<dyn BatchJobRepositoryPort>;
    let chunk_repo =
        Arc::new(ChunkRepositoryImpl::new(db_pool.clone())) as Arc<dyn ChunkRepositoryPort>;
    let embedding_repo =
        Arc::new(EmbeddingRepository::new(db_pool.clone())) as Arc<dyn EmbeddingRepositoryPort>;
    let uow_factory =
        Arc::new(SqliteUnitOfWorkFactory::new(db_pool.clone())) as Arc<dyn UnitOfWorkFactory>;

    let content_storage =
        Arc::new(ContentAddressedStorage::new()?) as Arc<dyn ContentAddressedStoragePort>;
    let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;
    // Build the transcription port once and share the same Arc with the
    // extraction adapter (audio ingest) and IndexingDi (the transcribe command).
    let transcription = crate::features::transcription::di::build(db_pool).port;
    let content_extractor = Arc::new(ContentExtractionAdapter::with_transcription(
        transcription.clone(),
    )) as Arc<dyn ContentExtractionPort>;
    let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;

    let indexing_embedding =
        Arc::new(DynamicEmbedding::new(embedding_cache)) as Arc<dyn EmbeddingPort>;

    // Degraded by default — real implementation is swapped in once models load.
    let indexing_service = degraded_mocks::create_degraded_indexing();
    let indexing_state = Arc::new(crate::infrastructure::indexing::IndexingState::new());

    let index_file_use_case = Arc::new(
        IndexFileUseCase::new(
            content_storage,
            file_storage.clone(),
            content_extractor,
            indexing_embedding.clone(),
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            embedding_repo,
            uow_factory.clone(),
        )
        .with_vector_search(vector_search.clone()),
    );
    let index_directory_use_case = Arc::new(IndexDirectoryUseCase::new(
        index_file_use_case.clone(),
        indexing_state.clone(),
    ));
    let reindex_document_use_case = Arc::new(ReindexDocumentUseCase::new(
        file_storage.clone(),
        indexing_embedding,
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
        uow_factory.clone(),
        vector_search.clone(),
    ));
    let delete_document_use_case = Arc::new(DeleteDocumentUseCase::new(
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
        chunk_repo.clone(),
        vector_search,
        uow_factory.clone(),
        file_storage.clone(),
    ));
    let rename_document_use_case = Arc::new(RenameDocumentUseCase::new(
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>
    ));

    Ok(IndexingDi {
        indexing_service,
        indexing_state,
        index_file_use_case,
        index_directory_use_case,
        reindex_document_use_case,
        delete_document_use_case,
        rename_document_use_case,
        batch_job_repo,
        chunk_repo,
        document_repo,
        file_storage,
        file_system,
        uow_factory,
        transcription,
    })
}
