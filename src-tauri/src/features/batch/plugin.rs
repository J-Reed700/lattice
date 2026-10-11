//! Batch plugin: file and URL imports, run as jobs.

use super::dto::{
    BatchJobStatusDto, CancelBatchJobRequestDto, CancelBatchJobResponseDto,
    DeleteBatchJobResponseDto, GetBatchJobStatusRequestDto, ListBatchJobsRequestDto,
    ListBatchJobsResponseDto, RetryFailedItemsResponseDto, StartBatchFileImportRequestDto,
    StartBatchFileImportResponseDto, StartBatchUrlImportRequestDto, StartBatchUrlImportResponseDto,
};
use super::file_job::{EmbeddingLoader, FileImporter};
use super::imports::{BatchImports, FileImport};
use super::items::BatchItems;
use super::url_job::UrlImporter;
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::infrastructure::persistence::repositories::document_scope::SqliteDocumentScope;
use crate::interfaces::di::Container;
use crate::shared::{error::AppError, ipc::ApiError};
use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

fn imports(container: &Container) -> BatchImports {
    BatchImports::new(
        container.jobs().clone(),
        BatchItems::new(container.db_pool().clone()),
        Arc::new(SqliteDocumentScope::new(container.db_pool().clone())),
    )
}

/// An import waits for the embedding model, so starting one without a model
/// that can load is refused up front.
async fn require_embedding(container: &Container) -> Result<(), AppError> {
    let embedding = container.get_or_load_embedding().await.map_err(|error| {
        AppError::ServiceNotAvailable(format!(
            "Could not load the active embedding model: {error}"
        ))
    })?;
    if !embedding.is_ready().await? {
        return Err(AppError::AiModelsNotInstalled(
            "AI embedding model is not ready yet. Activate an embedding model in Settings -> Models and try again."
                .into(),
        ));
    }
    Ok(())
}

/// Records an import action in the audit trail (CWE-778).
async fn audit(action: AuditAction, resource_id: String, error: Option<&AppError>, items: usize) {
    let outcome = match error {
        None => AuditResult::success(),
        Some(error) => AuditResult::failure(error.to_string()),
    };
    let event = AuditEvent::new(action, outcome)
        .with_resource_id(resource_id)
        .with_metadata("item_count", items.to_string());
    if let Err(error) = get_audit_logger().log(event).await {
        tracing::warn!(%error, "Failed to write audit log");
    }
}

/// Audits the start of an import.
async fn audit_start(resource: &str, started: &Result<String, AppError>, items: usize) {
    let resource_id = match started {
        Ok(job_id) => format!("{resource}:{job_id}"),
        Err(_) => format!("{resource}:failed"),
    };
    audit(
        AuditAction::DataImported,
        resource_id,
        started.as_ref().err(),
        items,
    )
    .await;
}

#[tauri::command]
#[specta::specta]
pub async fn batch_import_files(
    request: StartBatchFileImportRequestDto,
    container: State<'_, Container>,
) -> Result<StartBatchFileImportResponseDto, ApiError> {
    let count = request.file_paths.len();
    let started = match require_embedding(&container).await {
        Ok(()) => {
            imports(&container)
                .start_files(FileImport {
                    file_paths: request.file_paths,
                    space_id: request.space_id,
                    owner_conversation_id: request.owner_conversation_id,
                    indexing: request.indexing,
                })
                .await
        }
        Err(error) => Err(error),
    };
    audit_start("batch_file_job", &started, count).await;
    Ok(StartBatchFileImportResponseDto { job_id: started? })
}

#[tauri::command]
#[specta::specta]
pub async fn batch_import_urls(
    request: StartBatchUrlImportRequestDto,
    container: State<'_, Container>,
) -> Result<StartBatchUrlImportResponseDto, ApiError> {
    let count = request.urls.len();
    let started = async {
        require_embedding(&container).await?;
        container
            .security_context()
            .rate_limiters()
            .web_ingest
            .check_rate_limit("batch_url_import")
            .await
            .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
        imports(&container)
            .start_urls(&request.urls, request.options)
            .await
    }
    .await;
    audit_start("batch_url_job", &started, count).await;
    Ok(StartBatchUrlImportResponseDto { job_id: started? })
}

#[tauri::command]
#[specta::specta]
pub async fn get_batch_status(
    request: GetBatchJobStatusRequestDto,
    container: State<'_, Container>,
) -> Result<BatchJobStatusDto, ApiError> {
    Ok(imports(&container).status(&request.job_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_batch(
    request: CancelBatchJobRequestDto,
    container: State<'_, Container>,
) -> Result<CancelBatchJobResponseDto, ApiError> {
    let cancelled = imports(&container).cancel(&request.job_id).await;
    audit(
        AuditAction::Custom("Batch job cancelled".into()),
        format!("batch_job:{}", request.job_id),
        cancelled.as_ref().err(),
        cancelled.as_ref().copied().unwrap_or_default(),
    )
    .await;
    Ok(CancelBatchJobResponseDto {
        cancelled_count: cancelled?,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_batch_history(
    request: ListBatchJobsRequestDto,
    container: State<'_, Container>,
) -> Result<ListBatchJobsResponseDto, ApiError> {
    Ok(ListBatchJobsResponseDto {
        jobs: imports(&container)
            .list(request.limit, request.offset)
            .await?,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn delete_batch_job(
    job_id: String,
    container: State<'_, Container>,
) -> Result<DeleteBatchJobResponseDto, ApiError> {
    imports(&container).delete(&job_id).await?;
    Ok(DeleteBatchJobResponseDto { success: true })
}

/// Retries an import's failed items as a new attempt that takes over its
/// items; `item_id` retries one, optionally from `replacement_path`.
#[tauri::command]
#[specta::specta]
pub async fn retry_failed_items(
    job_id: String,
    item_id: Option<String>,
    replacement_path: Option<String>,
    container: State<'_, Container>,
) -> Result<RetryFailedItemsResponseDto, ApiError> {
    let (new_job_id, retried_count) = imports(&container)
        .retry(&job_id, item_id.as_deref(), replacement_path.as_deref())
        .await?;
    Ok(RetryFailedItemsResponseDto {
        new_job_id,
        retried_count,
    })
}

fn embedding_loader(container: &Container) -> EmbeddingLoader {
    let container = container.clone();
    Arc::new(move || {
        let container = container.clone();
        Box::pin(async move { container.get_or_load_embedding().await })
    })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("batch")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            let files = FileImporter {
                index_file: Arc::clone(container.indexing.index_file_use_case()),
                uow_factory: Arc::clone(container.indexing.uow_factory()),
                document_scope: Arc::new(SqliteDocumentScope::new(container.db_pool().clone())),
                load_embedding: embedding_loader(&container),
            };
            let urls = UrlImporter {
                ingest: Arc::clone(container.indexing.ingest_web_url_use_case()),
                load_embedding: embedding_loader(&container),
            };
            // Registration settles imports the last process left running,
            // then resumes them from their pending items.
            tauri::async_runtime::block_on(super::worker::register(
                container.jobs(),
                &BatchItems::new(container.db_pool().clone()),
                files,
                urls,
            ))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            batch_import_files,
            batch_import_urls,
            get_batch_status,
            cancel_batch,
            get_batch_history,
            delete_batch_job,
            retry_failed_items,
        ])
        .build()
}
