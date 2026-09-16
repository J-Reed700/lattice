//! File feature dependency injection.

use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{
    DocumentRepository, DocumentRepositoryPort, FileStoragePort, FileSystemPort,
};
use crate::features::file::repository::{
    CitingConversationsRepositoryPort, SqliteCitingConversationsRepository,
};
use crate::features::file::use_cases::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, ListCitingConversationsUseCase,
    OpenFileByIdUseCase, OpenFileUseCase, ReadFileBytesUseCase, ReadFileContentUseCase,
    ShowInFolderUseCase, UpdateFileMetadataUseCase,
};
use crate::features::tags::TagServiceTrait;
use crate::infrastructure::security::FileAccessConfig;
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct FileDi {
    pub open_file_use_case: Arc<OpenFileUseCase>,
    pub open_file_by_id_use_case: Arc<OpenFileByIdUseCase>,
    pub get_file_path_by_id_use_case: Arc<GetFilePathByIdUseCase>,
    pub show_in_folder_use_case: Arc<ShowInFolderUseCase>,
    pub get_file_metadata_use_case: Arc<GetFileMetadataUseCase>,
    pub read_file_content_use_case: Arc<ReadFileContentUseCase>,
    pub read_file_bytes_use_case: Arc<ReadFileBytesUseCase>,
    pub update_file_metadata_use_case: Arc<UpdateFileMetadataUseCase>,
    pub list_citing_conversations_use_case: Arc<ListCitingConversationsUseCase>,
}

pub fn build(
    document_repo: Arc<dyn DocumentRepository>,
    file_system: Arc<dyn FileSystemPort>,
    file_storage: Arc<dyn FileStoragePort>,
    tag_service: Arc<dyn TagServiceTrait>,
    file_access_config: Arc<FileAccessConfig>,
    vault_path: PathBuf,
    db_pool: SqlitePool,
) -> FileDi {
    FileDi {
        open_file_use_case: Arc::new(OpenFileUseCase::new(
            file_system.clone(),
            file_storage.clone(),
            file_access_config.clone(),
        )),
        open_file_by_id_use_case: Arc::new(OpenFileByIdUseCase::new(
            document_repo.clone(),
            file_system.clone(),
            file_storage.clone(),
            file_access_config.clone(),
            vault_path.clone(),
        )),
        get_file_path_by_id_use_case: Arc::new(GetFilePathByIdUseCase::new(
            document_repo.clone(),
            file_storage.clone(),
            file_access_config.clone(),
            vault_path,
        )),
        show_in_folder_use_case: Arc::new(ShowInFolderUseCase::new(
            file_system,
            file_storage.clone(),
            file_access_config.clone(),
        )),
        get_file_metadata_use_case: Arc::new(GetFileMetadataUseCase::new(
            file_storage.clone(),
            file_access_config.clone(),
        )),
        read_file_content_use_case: Arc::new(ReadFileContentUseCase::new(
            file_storage.clone(),
            file_access_config.clone(),
        )),
        read_file_bytes_use_case: Arc::new(ReadFileBytesUseCase::new(
            file_storage,
            file_access_config,
        )),
        update_file_metadata_use_case: Arc::new(UpdateFileMetadataUseCase::new(
            document_repo as Arc<dyn DocumentRepositoryPort>,
            tag_service,
        )),
        list_citing_conversations_use_case: Arc::new(ListCitingConversationsUseCase::new(
            Arc::new(SqliteCitingConversationsRepository::new(db_pool))
                as Arc<dyn CitingConversationsRepositoryPort>,
        )),
    }
}

/// File operations' registrar surface on `Container`.
impl Container {
    // File Operations (from FileOpsModule)
    pub fn open_file_use_case(&self) -> Arc<OpenFileUseCase> {
        Arc::clone(self.file_ops.open_file_use_case())
    }

    pub fn open_file_by_id_use_case(&self) -> Arc<OpenFileByIdUseCase> {
        Arc::clone(self.file_ops.open_file_by_id_use_case())
    }

    pub fn get_file_path_by_id_use_case(&self) -> Arc<GetFilePathByIdUseCase> {
        Arc::clone(self.file_ops.get_file_path_by_id_use_case())
    }

    pub fn show_in_folder_use_case(&self) -> Arc<ShowInFolderUseCase> {
        Arc::clone(self.file_ops.show_in_folder_use_case())
    }

    pub fn get_file_metadata_use_case(&self) -> Arc<GetFileMetadataUseCase> {
        Arc::clone(self.file_ops.get_file_metadata_use_case())
    }

    pub fn read_file_content_use_case(&self) -> Arc<ReadFileContentUseCase> {
        Arc::clone(self.file_ops.read_file_content_use_case())
    }

    pub fn read_file_bytes_use_case(&self) -> Arc<ReadFileBytesUseCase> {
        Arc::clone(self.file_ops.read_file_bytes_use_case())
    }

    pub fn update_file_metadata_use_case(&self) -> Arc<UpdateFileMetadataUseCase> {
        Arc::clone(self.file_ops.update_file_metadata_use_case())
    }

    pub fn list_citing_conversations_use_case(&self) -> Arc<ListCitingConversationsUseCase> {
        Arc::clone(self.file_ops.list_citing_conversations_use_case())
    }

    pub fn file_library(
        &self,
    ) -> Arc<dyn crate::application::ports::file_library::FileLibraryPort> {
        Arc::clone(self.file_ops.library())
    }
}
