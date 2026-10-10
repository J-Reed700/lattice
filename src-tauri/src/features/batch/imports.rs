//! Starting, reading, cancelling, retrying and deleting batch imports. Each
//! import is one job; its items are saved with it in one transaction.
use super::dto::{
    BatchImportOptionsDto, BatchJobItemDto, BatchJobStatusDto, BatchJobSummaryDto,
    FileIndexingOptionsDto,
};
use super::file_job::FileImportRequest;
use super::items::{BatchItem, BatchItems, ItemCounts};
use crate::application::ports::document_scope::DocumentScopePort;
use crate::shared::{
    error::{AppError, Result},
    runtime::jobs::{JobRecord, JobRuntime, JobStore, NewJob},
    types::ValidatedFilePath,
};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};

pub const FILE_IMPORT: &str = "batch.file_import";
pub const URL_IMPORT: &str = "batch.url_import";
const KINDS: [&str; 2] = [FILE_IMPORT, URL_IMPORT];

const MAX_FILES: usize = 100;
const MAX_URLS: usize = 50;
const DEFAULT_HISTORY: i64 = 50;
const MAX_HISTORY: i64 = 100;

/// The import's type as the renderer names it.
fn job_type(kind: &str) -> &str {
    kind.strip_prefix("batch.").unwrap_or(kind)
}

fn timestamp(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(crate::shared::persistence::timestamps::format_db_timestamp)
        .unwrap_or_default()
}

fn summary(job: &JobRecord, counts: ItemCounts) -> BatchJobSummaryDto {
    BatchJobSummaryDto {
        job_id: job.id.clone(),
        job_type: job_type(&job.kind).to_string(),
        status: job.status.as_str().to_string(),
        total_items: i64::from(counts.total),
        completed_items: i64::from(counts.completed),
        failed_items: i64::from(counts.failed),
        created_at: timestamp(job.created_at),
    }
}

fn item_dto(item: BatchItem) -> BatchJobItemDto {
    BatchJobItemDto {
        item_id: item.id,
        target: item.target,
        status: item.state.as_str().to_string(),
        error_message: item.error_message,
        document_id: item.document_id,
    }
}

fn not_an_import() -> AppError {
    AppError::NotFound("Import not found".into())
}

fn validate_url(url: &str) -> Result<()> {
    if url.trim().is_empty() {
        return Err(AppError::InvalidInput("URL cannot be empty".into()));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::InvalidInput(
            "URL must start with http:// or https://".into(),
        ));
    }
    Ok(())
}

/// A file import as the user asked for it.
pub struct FileImport {
    pub file_paths: Vec<String>,
    pub space_id: Option<String>,
    pub owner_conversation_id: Option<String>,
    pub indexing: Option<FileIndexingOptionsDto>,
}

#[derive(Clone)]
pub struct BatchImports {
    jobs: Arc<JobRuntime>,
    items: BatchItems,
    document_scope: Arc<dyn DocumentScopePort>,
}

impl BatchImports {
    pub fn new(
        jobs: Arc<JobRuntime>,
        items: BatchItems,
        document_scope: Arc<dyn DocumentScopePort>,
    ) -> Self {
        Self {
            jobs,
            items,
            document_scope,
        }
    }

    fn store(&self) -> &JobStore {
        self.jobs.store()
    }

    /// Saves the job and its items together, then starts it.
    async fn start(
        &self,
        kind: &str,
        targets: &[String],
        requested: serde_json::Value,
    ) -> Result<String> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::to_vec(&(kind, targets, &requested))?;
        let job = NewJob {
            kind: kind.to_string(),
            subject_id: None,
            operation_id,
            payload_hash: format!("{:x}", Sha256::digest(payload)),
            requested,
            progress_total: u32::try_from(targets.len()).unwrap_or(u32::MAX),
            message: "Queued".into(),
        };
        let mut tx = self
            .items
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        let admitted = JobStore::submit_in(&mut tx, &job).await?;
        BatchItems::insert_in(&mut tx, &admitted.id, targets).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        self.jobs.submitted(&admitted.id).await;
        Ok(admitted.id)
    }

    /// Checks and starts a file import. Every path must be safe to read, and
    /// the space and conversation it names must exist.
    pub async fn start_files(&self, import: FileImport) -> Result<String> {
        let count = import.file_paths.len();
        if count == 0 {
            return Err(AppError::InvalidInput("No files provided".into()));
        }
        if count > MAX_FILES {
            return Err(AppError::InvalidInput(format!(
                "Too many files: {count} (max {MAX_FILES})"
            )));
        }
        for path in &import.file_paths {
            ValidatedFilePath::new(PathBuf::from(path))?;
        }
        if let Some(space_id) = &import.space_id {
            if !self.document_scope.space_exists(space_id).await? {
                return Err(AppError::InvalidInput(format!(
                    "Space not found: {space_id}"
                )));
            }
        }
        if let Some(conversation_id) = &import.owner_conversation_id {
            if !self
                .document_scope
                .conversation_exists(conversation_id)
                .await?
            {
                return Err(AppError::InvalidInput(format!(
                    "Conversation not found: {conversation_id}"
                )));
            }
        }
        if let Some(group) = import
            .indexing
            .as_ref()
            .and_then(|indexing| indexing.source_group.as_ref())
        {
            group.validate()?;
            let unique: std::collections::HashSet<_> = import.file_paths.iter().collect();
            if unique.len() != count {
                return Err(AppError::InvalidInput(
                    "The same file was selected more than once".into(),
                ));
            }
        }
        let request = serde_json::to_value(FileImportRequest {
            space_id: import.space_id,
            owner_conversation_id: import.owner_conversation_id,
            indexing: import.indexing,
        })?;
        self.start(FILE_IMPORT, &import.file_paths, request).await
    }

    /// Checks and starts a URL import.
    pub async fn start_urls(
        &self,
        urls: &[String],
        options: Option<BatchImportOptionsDto>,
    ) -> Result<String> {
        if urls.is_empty() {
            return Err(AppError::InvalidInput(
                "Batch must contain at least 1 URL".into(),
            ));
        }
        if urls.len() > MAX_URLS {
            return Err(AppError::InvalidInput(format!(
                "Batch size {} exceeds maximum of {MAX_URLS}",
                urls.len()
            )));
        }
        for url in urls {
            validate_url(url)?;
        }
        self.start(URL_IMPORT, urls, serde_json::to_value(options)?)
            .await
    }

    async fn import_job(&self, job_id: &str) -> Result<JobRecord> {
        if job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".into()));
        }
        let job = self.store().get(job_id).await?;
        if !KINDS.contains(&job.kind.as_str()) {
            return Err(not_an_import());
        }
        Ok(job)
    }

    /// The import and its items, read from one snapshot.
    pub async fn status(&self, job_id: &str) -> Result<BatchJobStatusDto> {
        if job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".into()));
        }
        let mut tx = self
            .items
            .pool()
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        let job = JobStore::find_in(&mut tx, job_id)
            .await?
            .filter(|job| KINDS.contains(&job.kind.as_str()))
            .ok_or_else(not_an_import)?;
        let items = BatchItems::list_in(&mut tx, job_id).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        let heading = summary(&job, ItemCounts::of(&items));
        Ok(BatchJobStatusDto {
            job_id: heading.job_id,
            job_type: heading.job_type,
            status: heading.status,
            total_items: heading.total_items,
            completed_items: heading.completed_items,
            failed_items: heading.failed_items,
            created_at: heading.created_at,
            completed_at: job.finished_at.map(timestamp),
            items: items.into_iter().map(item_dto).collect(),
        })
    }

    /// Imports newest first. A retried import is listed once, as its latest
    /// attempt.
    pub async fn list(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<BatchJobSummaryDto>> {
        let limit = match limit {
            Some(limit) if limit < 0 => {
                return Err(AppError::InvalidInput("Limit cannot be negative".into()))
            }
            Some(limit) => limit.min(MAX_HISTORY),
            None => DEFAULT_HISTORY,
        };
        let offset = match offset {
            Some(offset) if offset < 0 => {
                return Err(AppError::InvalidInput("Offset cannot be negative".into()))
            }
            Some(offset) => offset,
            None => 0,
        };
        let jobs = self
            .store()
            .list_latest(
                &KINDS,
                u32::try_from(limit).unwrap_or(u32::MAX),
                u32::try_from(offset).unwrap_or(u32::MAX),
            )
            .await?;
        let ids: Vec<String> = jobs.iter().map(|job| job.id.clone()).collect();
        let counts = self.items.counts(&ids).await?;
        Ok(jobs
            .iter()
            .map(|job| summary(job, counts.get(&job.id).copied().unwrap_or_default()))
            .collect())
    }

    /// Cancels the import and every item it has not finished. Returns how many
    /// items that stopped; an import that already ended stops none.
    pub async fn cancel(&self, job_id: &str) -> Result<usize> {
        let job = self.import_job(job_id).await?;
        if job.status.is_finished() {
            return Ok(0);
        }
        let mut tx = self
            .items
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        JobStore::cancel_in(&mut tx, job_id).await?;
        let cancelled = BatchItems::cancel_unfinished_in(&mut tx, job_id).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        self.jobs.cancelled(job_id).await;
        Ok(cancelled)
    }

    /// Deletes a finished import with its items and earlier attempts. The
    /// documents it imported stay.
    pub async fn delete(&self, job_id: &str) -> Result<()> {
        self.import_job(job_id).await?;
        self.store().delete(job_id).await
    }

    /// Queues the failed items again as a new attempt of the import, which
    /// takes over its items: all failed items, or the one `item_id` names. A
    /// file item may point at a replacement file. Returns the new attempt's
    /// ID and how many items it retries.
    pub async fn retry(
        &self,
        job_id: &str,
        item_id: Option<&str>,
        replacement: Option<&str>,
    ) -> Result<(String, usize)> {
        let job = self.import_job(job_id).await?;
        if let Some(path) = replacement {
            if job.kind != FILE_IMPORT {
                return Err(AppError::InvalidInput(
                    "This import does not support file recovery".into(),
                ));
            }
            if item_id.is_none() {
                return Err(AppError::InvalidInput(
                    "Choose one failed file to replace".into(),
                ));
            }
            ValidatedFilePath::new(PathBuf::from(path))?;
        }
        if !job.status.is_finished() {
            return Err(AppError::InvalidInput(
                "This import is still running".into(),
            ));
        }
        let mut tx = self
            .items
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        let attempt = JobStore::retry_in(
            &mut tx,
            job_id,
            &uuid::Uuid::new_v4().to_string(),
            "Retry queued",
        )
        .await?;
        if !attempt.created {
            return Err(AppError::InvalidState(
                "This import is already being retried. Refresh its status.".into(),
            ));
        }
        BatchItems::move_to_in(&mut tx, job_id, &attempt.id).await?;
        let retried =
            BatchItems::requeue_failed_in(&mut tx, &attempt.id, item_id, replacement).await?;
        let unfinished = BatchItems::list_in(&mut tx, &attempt.id)
            .await?
            .iter()
            .filter(|item| item.state == super::items::ItemState::Pending)
            .count();
        if unfinished == 0 || (item_id.is_some() && retried == 0) {
            return Err(AppError::InvalidInput(
                "No failed items to retry in this import".into(),
            ));
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        self.jobs.submitted(&attempt.id).await;
        Ok((attempt.id, retried))
    }
}
