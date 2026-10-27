use crate::features::file::dto::{
    FileMetadataDto, GetFileMetadataRequestDto, GetFilePathByIdRequestDto, OpenFileByIdRequestDto,
    OpenFileRequestDto, OpenFileResponseDto, ReadFileBytesRequestDto, ReadFileContentRequestDto,
    ShowInFolderRequestDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

pub use lattice_core::application::contracts::file_library::{IndexedFolder, IndexingActivity};

/// Core implementation - Opens a file in the system's default application or returns internal rendering info
///
/// Opens the specified file using the operating system's default application for that file type.
/// For web articles (detected by article.md + page.html), returns internal rendering info instead.
/// Uses platform-specific APIs to prevent command injection vulnerabilities.
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
/// - **Command Injection Prevention (CWE-78)**: Uses `opener` crate with platform-specific APIs that don't invoke shell
pub async fn open_file_impl(
    container: &Container,
    path: String,
) -> Result<OpenFileResponseDto, AppError> {
    let use_case = container.open_file_use_case();
    let request = OpenFileRequestDto { path };
    use_case.execute(request).await
}

/// Legacy Tauri shim - Opens a file in the system's default application or returns internal rendering info
///
/// Opens the specified file using the operating system's default application for that file type.
/// For web articles (detected by article.md + page.html), returns internal rendering info instead.
/// Uses platform-specific APIs to prevent command injection vulnerabilities.
#[tauri::command]
#[specta::specta]
pub async fn open_file(
    path: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_impl(&container, path).await
}

/// Core implementation - Opens a file by database ID in the system's default application or returns internal rendering info
///
/// Looks up the file path from the database using the document ID, then either opens it in the
/// system's default application or returns information for internal rendering (for web articles).
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Resolves and validates full path before opening
/// - **Command Injection Prevention (CWE-78)**: Uses `opener` crate with safe platform APIs
pub async fn open_file_by_id_impl(
    container: &Container,
    file_id: String,
) -> Result<OpenFileResponseDto, AppError> {
    let use_case = container.open_file_by_id_use_case();
    let request = OpenFileByIdRequestDto {
        document_id: file_id,
    };
    use_case.execute(request).await
}

/// Legacy Tauri shim - Opens a file by database ID in the system's default application or returns internal rendering info
#[tauri::command]
#[specta::specta]
pub async fn open_file_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_by_id_impl(&container, file_id).await
}

/// Retrieves the absolute file path for a file by its database ID
///
/// Queries the database for the file path using document ID. Useful for displaying
/// file locations or performing file operations that require the full path.
///
/// # Errors
///
/// * `AppError::Other` - Database query failed
/// * `Err(String)` - Document ID not found in database (converted from `ok_or_else`)
/// * `AppError::Other` - File exists in database but not on disk
///
/// # Security
///
/// - **TOCTOU Prevention**: Validates file exists on disk before returning path
pub async fn get_file_path_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    let use_case = container.get_file_path_by_id_use_case();
    let request = GetFilePathByIdRequestDto {
        document_id: file_id,
    };
    let response = use_case.execute(request).await?;

    if response.exists {
        Ok(response.path)
    } else {
        Err(AppError::NotFound(format!(
            "File not found on disk: {}",
            response.path
        )))
    }
}

/// Retrieves all folders currently being watched for indexing
///
/// Returns a list of all folders in the watch list with their configuration and statistics.
/// For each folder, includes the document count, last scan time, and whether the folder
/// is enabled for monitoring.
///
/// # Errors
///
/// * `AppError::Other` - Failed to fetch indexed folders from database
/// * `AppError::Other` - Failed to count documents for a folder
///
/// # Security
///
/// - **SQL Injection Prevention (CWE-89)**: Uses parameterized queries for document count
/// - **No Authorization Required**: Listing watch folders is a read-only operation
pub async fn get_indexed_folders(
    container: State<'_, Container>,
) -> Result<Vec<IndexedFolder>, AppError> {
    container.file_library().indexed_folders().await
}

/// Retrieves recent indexing activities with configurable limit
///
/// Returns the most recent indexing activities (document indexing operations) ordered
/// by timestamp. Useful for displaying indexing history or monitoring indexing progress.
///
/// # Errors
///
/// * `AppError::Other` - Failed to fetch indexing activities from database
///
/// # Security
///
/// - **DoS Prevention (CWE-770)**: Limit is capped at 10,000 to prevent excessive resource usage
/// - **SQL Injection Prevention**: Uses parameterized queries with bind
pub async fn get_indexing_activities(
    container: State<'_, Container>,
    limit: usize,
) -> Result<Vec<IndexingActivity>, AppError> {
    container.file_library().indexing_activities(limit).await
}

/// Core implementation - Reads the content of a file for preview purposes
///
/// # Security
/// - Path validation prevents directory traversal (CWE-22)
/// - File size limited to 10MB to prevent DoS (CWE-770)
pub async fn read_file_content_impl(
    container: &Container,
    path: String,
) -> Result<String, AppError> {
    let use_case = container.read_file_content_use_case();
    let request = ReadFileContentRequestDto { path };
    let result = use_case.execute(request).await;

    match result {
        Ok(dto) => Ok(dto.content),
        Err(AppError::FileTooLarge { size_bytes, .. }) => Err(AppError::InvalidInput(format!(
            "File too large to preview (max 10MB, file is {} bytes)",
            size_bytes
        ))),
        Err(e) => Err(e),
    }
}

/// Core implementation - Reads the content of a file as bytes for preview purposes
///
/// # Security
/// - Path validation prevents directory traversal (CWE-22)
/// - File size limited to prevent DoS (CWE-770)
pub async fn read_file_bytes_impl(
    container: &Container,
    path: String,
) -> Result<Vec<u8>, AppError> {
    let use_case = container.read_file_bytes_use_case();
    let request = ReadFileBytesRequestDto { path };
    let result = use_case.execute(request).await;

    match result {
        Ok(bytes) => Ok(bytes),
        Err(AppError::FileTooLarge { size_bytes, .. }) => Err(AppError::InvalidInput(format!(
            "File too large to preview (max 10MB, file is {} bytes)",
            size_bytes
        ))),
        Err(e) => Err(e),
    }
}

/// Legacy Tauri shim - Reads the content of a file for preview purposes
#[tauri::command]
#[specta::specta]
pub async fn read_file_content(
    path: String,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    read_file_content_impl(&container, path).await
}

/// Legacy Tauri shim - Reads the content of a file as bytes for preview purposes
#[tauri::command]
#[specta::specta]
pub async fn read_file_bytes(
    path: String,
    container: State<'_, Container>,
) -> Result<Vec<u8>, AppError> {
    read_file_bytes_impl(&container, path).await
}

/// Core implementation - Retrieves file metadata (size and modification time) for a given path
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
pub async fn get_file_metadata_impl(
    container: &Container,
    path: String,
) -> Result<FileMetadataDto, AppError> {
    let use_case = container.get_file_metadata_use_case();
    let request = GetFileMetadataRequestDto { path };
    let response = use_case.execute(request).await?;

    Ok(response)
}

/// Legacy Tauri shim - Retrieves file metadata (size and modification time) for a given path
#[tauri::command]
#[specta::specta]
pub async fn get_file_metadata(
    path: String,
    container: State<'_, Container>,
) -> Result<FileMetadataDto, AppError> {
    get_file_metadata_impl(&container, path).await
}

/// Core implementation - Reveals a file in the system file explorer
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
/// - **Command Injection Prevention (CWE-78)**: Uses defense-in-depth approach with platform-specific safety
pub async fn show_in_folder_impl(container: &Container, path: String) -> Result<(), AppError> {
    let use_case = container.show_in_folder_use_case();
    let request = ShowInFolderRequestDto { path };
    use_case.execute(request).await?;
    Ok(())
}

/// Legacy Tauri shim - Reveals a file in the system file explorer
#[tauri::command]
#[specta::specta]
pub async fn show_in_folder(path: String, container: State<'_, Container>) -> Result<(), AppError> {
    show_in_folder_impl(&container, path).await
}

#[cfg(test)]
mod tests;
