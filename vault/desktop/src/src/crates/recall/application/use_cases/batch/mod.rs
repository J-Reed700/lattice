//! # Batch Operations Use Cases
//!
//! Use cases for batch file and URL import operations.
//!
//! This module provides use cases for creating and monitoring batch import jobs:
//!
//! ## File Import
//! - **StartBatchFileImportUseCase**: Create batch file import job
//! - **GetBatchFileStatusUseCase**: Get batch job progress
//!
//! ## URL Import
//! - **StartBatchUrlImportUseCase**: Create batch URL import job
//! - **GetBatchJobStatusUseCase**: Get detailed batch job status
//! - **CancelBatchJobUseCase**: Cancel pending batch job items
//! - **ListBatchJobsUseCase**: List all batch jobs (paginated)
//! - **DeleteBatchJobUseCase**: Delete batch job and items
//! - **RetryFailedItemsUseCase**: Retry failed items from previous job
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::batch::{
//!     StartBatchFileImportUseCase,
//!     GetBatchFileStatusUseCase,
//! };
//! use vault_desktop::application::dtos::batch_dto::{
//!     StartBatchFileImportRequestDto,
//!     GetBatchStatusRequestDto,
//! };
//! use std::sync::Arc;
//!
//! # async fn example(
//! #     start_use_case: StartBatchFileImportUseCase,
//! #     status_use_case: GetBatchFileStatusUseCase,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! // Start batch import
//! let start_request = StartBatchFileImportRequestDto {
//!     file_paths: vec![
//!         "/path/to/file1.txt".to_string(),
//!         "/path/to/file2.pdf".to_string(),
//!     ],
//! };
//!
//! let start_response = start_use_case.execute(start_request).await?;
//! println!("Job started: {}", start_response.job_id);
//!
//! // Check status
//! let status_request = GetBatchStatusRequestDto {
//!     job_id: start_response.job_id.clone(),
//! };
//!
//! let status_response = status_use_case.execute(status_request).await?;
//! println!("Progress: {}/{}",
//!     status_response.progress.completed,
//!     status_response.progress.total
//! );
//! # Ok(())
//! # }
//! ```

// File import use cases
mod get_batch_file_status;
mod start_batch_file_import;

// URL import use cases
mod cancel_batch_job;
mod delete_batch_job;
mod get_batch_job_status;
mod list_batch_jobs;
mod retry_failed_items;
mod start_batch_url_import;

// File import exports
pub use get_batch_file_status::GetBatchFileStatusUseCase;
pub use start_batch_file_import::StartBatchFileImportUseCase;

// URL import exports
pub use cancel_batch_job::CancelBatchJobUseCase;
pub use delete_batch_job::DeleteBatchJobUseCase;
pub use get_batch_job_status::GetBatchJobStatusUseCase;
pub use list_batch_jobs::ListBatchJobsUseCase;
pub use retry_failed_items::RetryFailedItemsUseCase;
pub use start_batch_url_import::StartBatchUrlImportUseCase;
