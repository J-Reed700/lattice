//! Indexing feature dependency injection.

use std::sync::Arc;

use lattice_core::shared::persistence::Database;

use crate::features::indexing::content_extraction_adapter::ContentExtractionAdapter;
use crate::features::indexing::engine::extraction::{ContentExtractor, ExtractionWorkers};
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};
use crate::features::indexing::LibraryGc;
use crate::infrastructure::file_system::{FileSystemAdapter, SecureFileStorage};
use crate::infrastructure::persistence::repositories::{
    ChunkRepositoryImpl, DocumentRepositoryImpl,
};
use crate::infrastructure::storage::content_addressed_storage::ContentAddressedStorage;
use crate::infrastructure::storage::FileIntents;
use crate::interfaces::di::Container;
use lattice_core::application::ports::{
    ChunkRepositoryPort, ContentAddressedStoragePort, ContentExtractionPort, DocumentIndexObserver,
    DocumentRepository, DocumentRepositoryPort, EmbeddingPort, EmbeddingRepositoryPort,
    FileStoragePort, FileSystemPort, OcrPort, SearchResultCachePort, TranscriptionPort,
    VectorSearchPort,
};
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::shared::error::Result;

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

    // Shared state the composition root hands to other features
    pub chunk_repo: Arc<dyn ChunkRepositoryPort>,
    pub document_repo: Arc<dyn DocumentRepository>,
    pub file_storage: Arc<dyn FileStoragePort>,
    pub file_system: Arc<dyn FileSystemPort>,

    /// On-device speech-to-text, shared with the content-extraction adapter so
    /// audio ingest and the explicit `transcribe_file` command use one engine.
    pub transcription: Arc<dyn TranscriptionPort>,
}

/// What indexing shares with the rest of the app: the search cache it
/// invalidates, the summary tier it reports to, and the parser workers every
/// extractor counts against.
pub struct IndexingShared {
    pub query_cache: Arc<dyn SearchResultCachePort>,
    pub index_observer: Arc<dyn DocumentIndexObserver>,
    pub extraction_workers: ExtractionWorkers,
    /// Open once startup finds this database owns the library; until then
    /// no blob is removed.
    pub sweep_gate: Arc<lattice_core::shared::fs::SweepGate>,
}

/// What indexing writes through that other features own, built by the
/// composition root: the embedder and its vector rows, and on-device
/// speech-to-text for audio.
pub struct IndexingPorts {
    pub embedding: Arc<dyn EmbeddingPort>,
    pub embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
    pub transcription: Arc<dyn TranscriptionPort>,
}

pub fn build(
    db_pool: Database,
    ports: IndexingPorts,
    vector_search: Arc<dyn VectorSearchPort>,
    web_archive: Arc<dyn ArticleArchive>,
    ocr: Arc<dyn OcrPort>,
    shared: IndexingShared,
) -> Result<IndexingDi> {
    let IndexingShared {
        query_cache,
        index_observer,
        extraction_workers,
        sweep_gate,
    } = shared;
    let IndexingPorts {
        embedding: indexing_embedding,
        embedding_repo,
        transcription,
    } = ports;
    let document_repo =
        Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;
    let chunk_repo =
        Arc::new(ChunkRepositoryImpl::new(db_pool.clone())) as Arc<dyn ChunkRepositoryPort>;
    let file_intents = FileIntents::new(db_pool.clone());

    let content_storage =
        Arc::new(ContentAddressedStorage::new()?) as Arc<dyn ContentAddressedStoragePort>;
    let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;
    // The transcription port is shared by the extraction adapter (audio
    // ingest) and IndexingDi (the transcribe command).
    // Import and re-index share this adapter, so both read scanned pages.
    let content_extractor = Arc::new(
        ContentExtractionAdapter::with_transcription(transcription.clone())
            .with_extractor(ContentExtractor::new().with_workers(extraction_workers))
            .with_ocr(ocr),
    ) as Arc<dyn ContentExtractionPort>;
    let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;

    let indexing_state = Arc::new(crate::features::indexing::engine::IndexingState::new());
    let library_gc = Arc::new(
        LibraryGc::new(
            Arc::clone(&content_storage),
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            sweep_gate,
        )
        .with_intents(file_intents),
    );

    let index_file_use_case = Arc::new(
        IndexFileUseCase::new(
            Arc::clone(&content_storage),
            file_storage.clone(),
            content_extractor.clone(),
            indexing_embedding.clone(),
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            embedding_repo,
            db_pool.clone(),
        )
        .with_vector_search(vector_search.clone())
        .with_query_cache(Arc::clone(&query_cache))
        .with_index_observer(Arc::clone(&index_observer))
        .with_library_gc(Arc::clone(&library_gc)),
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
            db_pool.clone(),
            vector_search.clone(),
        )
        .with_content_extractor(content_extractor.clone())
        .with_query_cache(Arc::clone(&query_cache)),
    );
    let delete_document_use_case = Arc::new(
        DeleteDocumentUseCase::new(
            vector_search,
            db_pool,
            content_storage,
            Arc::clone(&library_gc),
            web_archive,
        )
        .with_query_cache(query_cache)
        .with_index_observer(index_observer),
    );
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
        chunk_repo,
        document_repo,
        file_storage,
        file_system,
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

    /// The stored chunk vectors the indexer writes (from IndexingModule).
    pub fn embedding_repository(&self) -> Arc<dyn EmbeddingRepositoryPort> {
        Arc::clone(self.indexing.embedding_repository())
    }
}
