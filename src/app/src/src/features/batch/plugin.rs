//! Batch Plugin - Batch file and URL import operations
//!
//! Migrated from ipc/domains/batch.rs as part of Operation Scorched Earth Batch 4

use crate::features::batch::commands::{
    file_import as batch_file_import, history as batch_history, url_import as batch_url_import,
};
use crate::features::batch::dto::{
    BatchJobStatusDto, CancelBatchJobRequestDto, CancelBatchJobResponseDto,
    GetBatchJobStatusRequestDto, ListBatchJobsRequestDto, ListBatchJobsResponseDto,
    StartBatchFileImportRequestDto, StartBatchFileImportResponseDto, StartBatchUrlImportRequestDto,
    StartBatchUrlImportResponseDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn batch_import_files(
    request: StartBatchFileImportRequestDto,
    container: State<'_, Container>,
) -> Result<StartBatchFileImportResponseDto, ApiError> {
    let job_id = batch_file_import::start_batch_file_import(request.file_paths, container)
        .await
        .map_err(ApiError::from)?;

    Ok(StartBatchFileImportResponseDto { job_id })
}

#[tauri::command]
#[specta::specta]
pub async fn batch_import_urls(
    request: StartBatchUrlImportRequestDto,
    container: State<'_, Container>,
) -> Result<StartBatchUrlImportResponseDto, ApiError> {
    let batch_request = crate::features::batch::commands::url_import::StartBatchImportRequest {
        urls: request.urls,
        options: request.options.map(|opts| {
            crate::features::batch::commands::url_import::BatchImportOptions {
                extract_article: opts.extract_article,
            }
        }),
    };

    let job_id = batch_url_import::start_batch_url_import(batch_request, container)
        .await
        .map_err(ApiError::from)?;

    Ok(StartBatchUrlImportResponseDto { job_id })
}

#[tauri::command]
#[specta::specta]
pub async fn get_batch_status(
    request: GetBatchJobStatusRequestDto,
    container: State<'_, Container>,
) -> Result<BatchJobStatusDto, ApiError> {
    let status = batch_url_import::get_batch_job_status(request.job_id.clone(), container)
        .await
        .map_err(ApiError::from)?;

    Ok(status)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_batch(
    request: CancelBatchJobRequestDto,
    container: State<'_, Container>,
) -> Result<CancelBatchJobResponseDto, ApiError> {
    let cancelled_count = batch_url_import::cancel_batch_job(request.job_id, container)
        .await
        .map_err(ApiError::from)?;

    Ok(CancelBatchJobResponseDto { cancelled_count })
}

#[tauri::command]
#[specta::specta]
pub async fn get_batch_history(
    request: ListBatchJobsRequestDto,
    container: State<'_, Container>,
) -> Result<ListBatchJobsResponseDto, ApiError> {
    let response = batch_history::list_batch_jobs(request.limit, request.offset, container)
        .await
        .map_err(ApiError::from)?;

    Ok(response)
}

// Legacy compatibility commands (frontend expects these names)
#[tauri::command]
#[specta::specta]
pub async fn start_batch_file_import(
    file_paths: Vec<String>,
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    batch_file_import::start_batch_file_import(file_paths, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_batch_url_import(
    urls: Vec<String>,
    extract_article: Option<bool>,
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    let request = crate::features::batch::commands::url_import::StartBatchImportRequest {
        urls,
        options: Some(
            crate::features::batch::commands::url_import::BatchImportOptions { extract_article },
        ),
    };
    batch_url_import::start_batch_url_import(request, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_batch_job_status(
    job_id: String,
    container: State<'_, Container>,
) -> Result<BatchJobStatusDto, ApiError> {
    batch_url_import::get_batch_job_status(job_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_batch_job(
    job_id: String,
    container: State<'_, Container>,
) -> Result<usize, ApiError> {
    batch_url_import::cancel_batch_job(job_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn list_batch_jobs(
    limit: Option<i64>,
    offset: Option<i64>,
    container: State<'_, Container>,
) -> Result<ListBatchJobsResponseDto, ApiError> {
    batch_history::list_batch_jobs(limit, offset, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_batch_job(
    job_id: String,
    container: State<'_, Container>,
) -> Result<crate::features::batch::dto::DeleteBatchJobResponseDto, ApiError> {
    batch_history::delete_batch_job(job_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_failed_items(
    job_id: String,
    container: State<'_, Container>,
) -> Result<crate::features::batch::dto::RetryFailedItemsResponseDto, ApiError> {
    batch_history::retry_failed_items(job_id, container)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("batch")
        .invoke_handler(tauri::generate_handler![
            batch_import_files,
            batch_import_urls,
            get_batch_status,
            cancel_batch,
            get_batch_history,
            start_batch_file_import,
            start_batch_url_import,
            get_batch_job_status,
            cancel_batch_job,
            list_batch_jobs,
            delete_batch_job,
            retry_failed_items,
        ])
        .build()
}
