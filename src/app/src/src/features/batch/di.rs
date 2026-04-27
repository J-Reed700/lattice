//! Batch feature dependency injection.

use std::sync::Arc;

use crate::application::ports::BatchJobRepositoryPort;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::features::batch::services::url_import::BatchUrlImportService;
use crate::features::batch::use_cases::{
    CancelBatchJobUseCase, DeleteBatchJobUseCase, GetBatchJobStatusUseCase,
    ListBatchJobsUseCase, RetryFailedItemsUseCase, StartBatchFileImportUseCase,
    StartBatchUrlImportUseCase,
};
use crate::features::batch::{BatchFileImportServiceTrait, BatchUrlImportServiceTrait};
use crate::features::indexing::use_cases::IndexFileUseCase;
use crate::features::web::WebIngestionServiceTrait;
use crate::features::web::use_cases::IngestWebUrlUseCase;
use crate::infrastructure::setup::degraded_mocks;

#[derive(Clone)]
pub struct BatchDi {
    pub batch_file_import_service: Arc<dyn BatchFileImportServiceTrait>,
    pub batch_url_import_service: Arc<dyn BatchUrlImportServiceTrait>,

    pub start_batch_file_import_use_case: Arc<StartBatchFileImportUseCase>,
    pub start_batch_url_import_use_case: Arc<StartBatchUrlImportUseCase>,
    pub get_batch_job_status_use_case: Arc<GetBatchJobStatusUseCase>,
    pub cancel_batch_job_use_case: Arc<CancelBatchJobUseCase>,
    pub list_batch_jobs_use_case: Arc<ListBatchJobsUseCase>,
    pub delete_batch_job_use_case: Arc<DeleteBatchJobUseCase>,
    pub retry_failed_items_use_case: Arc<RetryFailedItemsUseCase>,
}

pub fn build(
    batch_job_repo: Arc<dyn BatchJobRepositoryPort>,
    index_file_use_case: Arc<IndexFileUseCase>,
    ingest_web_url_use_case: Arc<IngestWebUrlUseCase>,
    web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
) -> BatchDi {
    // Batch file import is degraded by default (depends on real indexing service).
    let batch_file_import_service = degraded_mocks::create_degraded_batch_file_import();
    let batch_url_import_service = Arc::new(BatchUrlImportService::new(
        web_ingestion_service,
        batch_job_repo.clone(),
    )) as Arc<dyn BatchUrlImportServiceTrait>;

    let start_batch_file_import_use_case = Arc::new(StartBatchFileImportUseCase::new(
        batch_job_repo.clone(),
        index_file_use_case,
        uow_factory,
    ));
    let start_batch_url_import_use_case = Arc::new(StartBatchUrlImportUseCase::new(
        batch_job_repo.clone(),
        ingest_web_url_use_case,
    ));
    let retry_failed_items_use_case = Arc::new(RetryFailedItemsUseCase::new(
        batch_job_repo.clone(),
        start_batch_url_import_use_case.clone(),
    ));

    BatchDi {
        batch_file_import_service,
        batch_url_import_service,
        start_batch_file_import_use_case,
        start_batch_url_import_use_case,
        get_batch_job_status_use_case: Arc::new(GetBatchJobStatusUseCase::new(
            batch_job_repo.clone(),
        )),
        cancel_batch_job_use_case: Arc::new(CancelBatchJobUseCase::new(batch_job_repo.clone())),
        list_batch_jobs_use_case: Arc::new(ListBatchJobsUseCase::new(batch_job_repo.clone())),
        delete_batch_job_use_case: Arc::new(DeleteBatchJobUseCase::new(batch_job_repo)),
        retry_failed_items_use_case,
    }
}
