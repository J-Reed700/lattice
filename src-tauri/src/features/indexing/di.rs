//! Indexing feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::UnitOfWorkFactory;
use crate::application::ports::{
    BatchJobRepositoryPort, ChunkRepositoryPort, ContentAddressedStoragePort,
    ContentExtractionPort, DocumentRepository, DocumentRepositoryPort, EmbeddingPort,
    EmbeddingRepositoryPort, FileStoragePort, FileSystemPort, TranscriptionPort, VectorSearchPort,
};
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};
use crate::features::indexing::LibraryGc;
use crate::features::web::WebArchiveServiceTrait;
use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
use crate::infrastructure::file_system::{FileSystemAdapter, SecureFileStorage};
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use crate::infrastructure::persistence::repositories::{
    BatchJobRepository, ChunkRepositoryImpl, DocumentRepositoryImpl, EmbeddingRepository,
};
use crate::infrastructure::storage::content_addressed_storage::ContentAddressedStorage;
use crate::interfaces::di::Container;
use crate::shared::error::Result;

#[derive(Clone)]
pub struct IndexingDi {
    pub indexing_state: Arc<crate::features::indexing::engine::IndexingState>,

    /// The one owner of blob deletion in the content-addressed library.
    /// Shared with the startup sweep.
    pub library_gc: Arc<LibraryGc>,

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
    model_provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    web_archive: Arc<dyn WebArchiveServiceTrait>,
) -> Result<IndexingDi> {
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
    // Learned sparse postings live beside the dense vectors. Wired
    // unconditionally: `IndexFileUseCase` only reads a sparse head when the
    // loaded model actually has one, so this is inert for a dense-only model.
    let sparse_term_store = Arc::new(
        crate::features::search::engine::sparse_search::SqliteSparseTermStore::new(db_pool.clone()),
    ) as Arc<dyn crate::application::ports::SparseTermStorePort>;
    // Build the transcription port once and share the same Arc with the
    // extraction adapter (audio ingest) and IndexingDi (the transcribe command).
    let transcription = crate::features::transcription::di::build(db_pool).port;
    let content_extractor = Arc::new(ContentExtractionAdapter::with_transcription(
        transcription.clone(),
    )) as Arc<dyn ContentExtractionPort>;
    let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;

    let indexing_embedding =
        Arc::new(DynamicEmbedding::new(model_provider)) as Arc<dyn EmbeddingPort>;

    let indexing_state = Arc::new(crate::features::indexing::engine::IndexingState::new());
    let library_gc = Arc::new(LibraryGc::new(
        Arc::clone(&content_storage),
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
    ));

    let index_file_use_case = Arc::new(
        IndexFileUseCase::new(
            Arc::clone(&content_storage),
            file_storage.clone(),
            content_extractor.clone(),
            indexing_embedding.clone(),
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            embedding_repo,
            uow_factory.clone(),
        )
        .with_vector_search(vector_search.clone())
        .with_sparse_term_store(Arc::clone(&sparse_term_store)),
    );
    let index_directory_use_case = Arc::new(IndexDirectoryUseCase::new(
        index_file_use_case.clone(),
        indexing_state.clone(),
    ));
    let reindex_document_use_case = Arc::new(
        ReindexDocumentUseCase::new(
            file_storage.clone(),
            indexing_embedding,
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            uow_factory.clone(),
            vector_search.clone(),
        )
        .with_content_extractor(content_extractor.clone())
        .with_sparse_term_store(Arc::clone(&sparse_term_store)),
    );
    let delete_document_use_case = Arc::new(DeleteDocumentUseCase::new(
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
        vector_search,
        uow_factory.clone(),
        content_storage,
        Arc::clone(&library_gc),
        web_archive,
    ));
    let rename_document_use_case = Arc::new(RenameDocumentUseCase::new(
        document_repo.clone() as Arc<dyn DocumentRepositoryPort>
    ));

    Ok(IndexingDi {
        indexing_state,
        library_gc,
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

/// Indexing's registrar surface on `Container`.
impl Container {
    // Indexing (from IndexingModule)
    pub fn index_file_use_case(&self) -> Arc<IndexFileUseCase> {
        Arc::clone(self.indexing.index_file_use_case())
    }

    pub fn index_directory_use_case(&self) -> Arc<IndexDirectoryUseCase> {
        Arc::clone(self.indexing.index_directory_use_case())
    }

    pub fn reindex_document_use_case(&self) -> Arc<ReindexDocumentUseCase> {
        Arc::clone(self.indexing.reindex_document_use_case())
    }

    pub fn delete_document_use_case(&self) -> Arc<DeleteDocumentUseCase> {
        Arc::clone(self.indexing.delete_document_use_case())
    }

    pub fn rename_document_use_case(&self) -> Arc<RenameDocumentUseCase> {
        Arc::clone(self.indexing.rename_document_use_case())
    }

    /// Library blob collector, shared with the startup sweep.
    pub fn library_gc(&self) -> Arc<LibraryGc> {
        Arc::clone(self.indexing.library_gc())
    }

    /// Chunk repository accessor (from IndexingModule)
    pub fn chunk_repository(&self) -> Arc<dyn ChunkRepositoryPort> {
        Arc::clone(self.indexing.chunk_repository())
    }
}
