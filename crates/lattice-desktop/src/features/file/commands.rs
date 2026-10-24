use crate::features::file::dto::{
    FileMetadataDto, GetFileMetadataRequestDto, GetFilePathByIdRequestDto, OpenFileByIdRequestDto,
    OpenFileRequestDto, OpenFileResponseDto, ReadFileBytesRequestDto, ReadFileContentRequestDto,
    ShowInFolderRequestDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

pub use lattice_core::application::contracts::file_library::{IndexedFolder, IndexingActivity};

pub async fn open_file_impl(
    container: &Container,
    path: String,
) -> Result<OpenFileResponseDto, AppError> {
    let use_case = container.open_file_use_case();
    let request = OpenFileRequestDto { path };
    use_case.execute(request).await
}

#[tauri::command]
#[specta::specta]
pub async fn open_file(
    path: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_impl(&container, path).await
}

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

#[tauri::command]
#[specta::specta]
pub async fn open_file_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_by_id_impl(&container, file_id).await
}

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

pub async fn get_indexed_folders(
    container: State<'_, Container>,
) -> Result<Vec<IndexedFolder>, AppError> {
    container.file_library().indexed_folders().await
}

pub async fn get_indexing_activities(
    container: State<'_, Container>,
    limit: usize,
) -> Result<Vec<IndexingActivity>, AppError> {
    container.file_library().indexing_activities(limit).await
}

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

#[tauri::command]
#[specta::specta]
pub async fn read_file_content(
    path: String,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    read_file_content_impl(&container, path).await
}

#[tauri::command]
#[specta::specta]
pub async fn read_file_bytes(
    path: String,
    container: State<'_, Container>,
) -> Result<Vec<u8>, AppError> {
    read_file_bytes_impl(&container, path).await
}

pub async fn get_file_metadata_impl(
    container: &Container,
    path: String,
) -> Result<FileMetadataDto, AppError> {
    let use_case = container.get_file_metadata_use_case();
    let request = GetFileMetadataRequestDto { path };
    let response = use_case.execute(request).await?;

    Ok(response)
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_metadata(
    path: String,
    container: State<'_, Container>,
) -> Result<FileMetadataDto, AppError> {
    get_file_metadata_impl(&container, path).await
}

pub async fn show_in_folder_impl(container: &Container, path: String) -> Result<(), AppError> {
    let use_case = container.show_in_folder_use_case();
    let request = ShowInFolderRequestDto { path };
    use_case.execute(request).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn show_in_folder(path: String, container: State<'_, Container>) -> Result<(), AppError> {
    show_in_folder_impl(&container, path).await
}

#[cfg(test)]
mod tests;
