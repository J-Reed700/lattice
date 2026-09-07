//! File feature — use cases.

pub mod get_metadata;
pub mod get_path_by_id;
pub mod list_citing_conversations;
pub mod open;
pub mod open_by_id;
pub mod read_bytes;
pub mod read_content;
pub mod show_in_folder;
pub mod update_metadata;

pub use get_metadata::GetFileMetadataUseCase;
pub use get_path_by_id::GetFilePathByIdUseCase;
pub use list_citing_conversations::ListCitingConversationsUseCase;
pub use open::OpenFileUseCase;
pub use open_by_id::OpenFileByIdUseCase;
pub use read_bytes::ReadFileBytesUseCase;
pub use read_content::ReadFileContentUseCase;
pub use show_in_folder::ShowInFolderUseCase;
pub use update_metadata::UpdateFileMetadataUseCase;
