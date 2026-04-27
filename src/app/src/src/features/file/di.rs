//! File feature dependency injection.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::{
    DocumentRepository, DocumentRepositoryPort, FileStoragePort, FileSystemPort,
};
use crate::features::file::use_cases::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, OpenFileByIdUseCase, OpenFileUseCase,
    ReadFileBytesUseCase, ReadFileContentUseCase, ShowInFolderUseCase, UpdateFileMetadataUseCase,
};
use crate::features::tags::TagServiceTrait;
use crate::infrastructure::security::FileAccessConfig;

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
}

pub fn build(
    document_repo: Arc<dyn DocumentRepository>,
    file_system: Arc<dyn FileSystemPort>,
    file_storage: Arc<dyn FileStoragePort>,
    tag_service: Arc<dyn TagServiceTrait>,
    file_access_config: Arc<FileAccessConfig>,
    vault_path: PathBuf,
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
    }
}
