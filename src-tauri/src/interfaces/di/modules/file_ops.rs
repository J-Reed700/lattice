//! The file operations module: opening, reading and updating library files.

use super::CoreModule;
use crate::infrastructure::persistence::repositories::file_library::SqliteFileLibrary;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - File Operations
use crate::features::file::use_cases::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, ListCitingConversationsUseCase,
    OpenFileByIdUseCase, OpenFileUseCase, ReadFileBytesUseCase, ReadFileContentUseCase,
    ShowInFolderUseCase, UpdateFileMetadataUseCase,
};

// Application Use Cases - Extraction
use crate::features::extraction::use_cases::{
    ExtractAndResolveLinksUseCase, ExtractDocumentTitleUseCase, ParseWikilinksUseCase,
    ResolveWikilinkUseCase,
};

// Application Ports
use crate::application::ports::{
    DocumentRepository, FileStoragePort, FileSystemPort, RepositoryPort,
};

// Service Traits
use crate::features::tags::TagServiceTrait;

/// File operations module for opening files, metadata, wikilinks
///
/// **Fields**: ~10
/// - 9 use cases (file operations, extraction)
/// - File system adapter
/// - Document repository (read-only for paths)
#[derive(Clone)]
pub struct FileOpsModule {
    library: Arc<dyn crate::application::ports::file_library::FileLibraryPort>,
    file: crate::features::file::di::FileDi,
    extraction: crate::features::extraction::di::ExtractionDi,
}

impl FileOpsModule {
    pub fn library(&self) -> &Arc<dyn crate::application::ports::file_library::FileLibraryPort> {
        &self.library
    }
    /// Build FileOpsModule by composing the file + extraction feature builders.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        extraction_workers: crate::features::indexing::engine::extraction::ExtractionWorkers,
    ) -> crate::shared::error::Result<Self> {
        use crate::features::indexing::engine::extraction::ContentExtractor;
        use crate::features::tags::service::TagService;
        use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
        use crate::infrastructure::file_system::file_storage::SecureFileStorage;
        use crate::infrastructure::file_system::FileSystemAdapter;
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;

        let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;
        let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;
        let file_access_config = Arc::clone(core.file_access_config());
        let vault_path = core
            .data_dir()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| core.data_dir().clone());

        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;
        let tag_service = Arc::new(TagService::new(db_pool.clone())) as Arc<dyn TagServiceTrait>;

        let library = Arc::new(SqliteFileLibrary::new(db_pool.clone()));
        // Previews of PDFs and office files parse under the same worker bound
        // as indexing, which builds its extractor from the same workers.
        let preview_extractor = Arc::new(
            ContentExtractionAdapter::new().with_extractor(
                ContentExtractor::with_max_size(
                    crate::features::file::use_cases::read_content::MAX_FILE_SIZE_BYTES,
                )
                .with_workers(extraction_workers),
            ),
        );
        let file = crate::features::file::di::build(
            document_repo.clone(),
            file_system,
            file_storage,
            tag_service,
            file_access_config,
            vault_path,
            db_pool,
            preview_extractor,
        );
        let extraction = crate::features::extraction::di::build(
            document_repo as Arc<dyn RepositoryPort<crate::domain::entities::Document>>,
        );

        Ok(Self {
            file,
            extraction,
            library,
        })
    }

    // File operation use case getters
    pub fn open_file_use_case(&self) -> &Arc<OpenFileUseCase> {
        &self.file.open_file_use_case
    }

    pub fn open_file_by_id_use_case(&self) -> &Arc<OpenFileByIdUseCase> {
        &self.file.open_file_by_id_use_case
    }

    pub fn get_file_path_by_id_use_case(&self) -> &Arc<GetFilePathByIdUseCase> {
        &self.file.get_file_path_by_id_use_case
    }

    pub fn show_in_folder_use_case(&self) -> &Arc<ShowInFolderUseCase> {
        &self.file.show_in_folder_use_case
    }

    pub fn get_file_metadata_use_case(&self) -> &Arc<GetFileMetadataUseCase> {
        &self.file.get_file_metadata_use_case
    }

    pub fn read_file_content_use_case(&self) -> &Arc<ReadFileContentUseCase> {
        &self.file.read_file_content_use_case
    }

    pub fn read_file_bytes_use_case(&self) -> &Arc<ReadFileBytesUseCase> {
        &self.file.read_file_bytes_use_case
    }

    pub fn update_file_metadata_use_case(&self) -> &Arc<UpdateFileMetadataUseCase> {
        &self.file.update_file_metadata_use_case
    }

    pub fn list_citing_conversations_use_case(&self) -> &Arc<ListCitingConversationsUseCase> {
        &self.file.list_citing_conversations_use_case
    }

    // Extraction use case getters
    pub fn parse_wikilinks_use_case(&self) -> &Arc<ParseWikilinksUseCase> {
        &self.extraction.parse_wikilinks_use_case
    }

    pub fn extract_document_title_use_case(&self) -> &Arc<ExtractDocumentTitleUseCase> {
        &self.extraction.extract_document_title_use_case
    }

    pub fn resolve_wikilink_use_case(&self) -> &Arc<ResolveWikilinkUseCase> {
        &self.extraction.resolve_wikilink_use_case
    }

    pub fn extract_and_resolve_links_use_case(&self) -> &Arc<ExtractAndResolveLinksUseCase> {
        &self.extraction.extract_and_resolve_links_use_case
    }
}
