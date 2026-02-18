//! # File Use Cases
//!
//! Use cases for file management operations.
//!
//! This module provides use cases for:
//! - Opening files with default applications
//! - Revealing files in file explorers
//! - Reading file content and metadata
//! - Looking up file paths by document ID
//!
//! ## Security
//!
//! All use cases enforce security measures:
//! - **Path Validation** (CWE-22): All file paths are validated to prevent directory traversal
//! - **Command Injection Prevention** (CWE-78): File operations use secure system APIs
//! - **Resource Limits** (CWE-770): File read operations have size limits
//! - **Audit Logging**: Security-relevant operations are audited
//!
//! ## Use Cases
//!
//! ### OpenFileUseCase
//!
//! Opens a file with the system's default application.
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::OpenFileUseCase;
//! use vault_desktop::application::dtos::file_dto::OpenFileRequestDto;
//! use vault_desktop::application::services::FileType;
//!
//! # async fn example(use_case: OpenFileUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = OpenFileRequestDto {
//!     path: "/docs/report.pdf".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! assert_eq!(response.action, "opened_external");
//! assert_eq!(response.file_type, FileType::Pdf);
//! # Ok(())
//! # }
//! ```
//!
//! ### OpenFileByIdUseCase
//!
//! Opens a file by document ID (looks up path first).
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::OpenFileByIdUseCase;
//! use vault_desktop::application::dtos::file_dto::OpenFileByIdRequestDto;
//!
//! # async fn example(use_case: OpenFileByIdUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = OpenFileByIdRequestDto {
//!     document_id: "doc-123".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("{}", response.action);
//! # Ok(())
//! # }
//! ```
//!
//! ### ShowInFolderUseCase
//!
//! Reveals a file in the system file explorer.
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::ShowInFolderUseCase;
//! use vault_desktop::application::dtos::file_dto::ShowInFolderRequestDto;
//!
//! # async fn example(use_case: ShowInFolderUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = ShowInFolderRequestDto {
//!     path: "/docs/report.pdf".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### GetFileMetadataUseCase
//!
//! Retrieves metadata for a file (size, modified time, MIME type, etc.).
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::GetFileMetadataUseCase;
//! use vault_desktop::application::dtos::file_dto::GetFileMetadataRequestDto;
//!
//! # async fn example(use_case: GetFileMetadataUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GetFileMetadataRequestDto {
//!     path: "/docs/report.pdf".to_string(),
//! };
//!
//! let metadata = use_case.execute(request).await?;
//! println!("Size: {} bytes", metadata.size_bytes);
//! # Ok(())
//! # }
//! ```
//!
//! ### ReadFileContentUseCase
//!
//! Reads file content as UTF-8 text with size limits.
//!
//! **Security:** Enforces 10 MB file size limit to prevent DoS attacks (CWE-770).
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::ReadFileContentUseCase;
//! use vault_desktop::application::dtos::file_dto::ReadFileContentRequestDto;
//!
//! # async fn example(use_case: ReadFileContentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = ReadFileContentRequestDto {
//!     path: "/docs/notes.txt".to_string(),
//! };
//!
//! let content = use_case.execute(request).await?;
//! println!("Content: {}", content.content);
//! # Ok(())
//! # }
//! ```
//!
//! ### GetFilePathByIdUseCase
//!
//! Retrieves the file path for a document by its ID.
//!
//! **Example:**
//! ```rust,no_run
//! use vault_desktop::application::use_cases::file::GetFilePathByIdUseCase;
//! use vault_desktop::application::dtos::file_dto::GetFilePathByIdRequestDto;
//!
//! # async fn example(use_case: GetFilePathByIdUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GetFilePathByIdRequestDto {
//!     document_id: "doc-123".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Path: {}", response.path);
//! println!("Exists: {}", response.exists);
//! # Ok(())
//! # }
//! ```

pub mod get_file_metadata;
pub mod get_file_path_by_id;
pub mod open_file;
pub mod open_file_by_id;
pub mod read_file_bytes;
pub mod read_file_content;
pub mod show_in_folder;
pub mod update_file_metadata;

// Re-export use cases
pub use get_file_metadata::GetFileMetadataUseCase;
pub use get_file_path_by_id::GetFilePathByIdUseCase;
pub use open_file::OpenFileUseCase;
pub use open_file_by_id::OpenFileByIdUseCase;
pub use read_file_bytes::ReadFileBytesUseCase;
pub use read_file_content::ReadFileContentUseCase;
pub use show_in_folder::ShowInFolderUseCase;
pub use update_file_metadata::UpdateFileMetadataUseCase;
