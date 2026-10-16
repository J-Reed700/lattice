//! # Frozen web snapshots: storage, rows, job, URL scheme
//!
//! Everything around the capture engine (`features::web::services::snapshot`):
//! the files under `~/.lattice/web-snapshots/`, the `web_page_snapshots` rows,
//! the durable `web_page_snapshot` job, the private `lattice-snapshot` URL
//! scheme, the commands, and the sweep. Design:
//! `docs/design/2026-10-13-frozen-web-snapshots.md` §7, §8, §10, §11.

pub mod commands;
pub mod dto;
pub mod gc;
pub mod job;
pub mod protocol;
pub mod repository;
pub mod store;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use serde_json::json;
use sqlx::SqlitePool;

pub use dto::{
    WebPageSnapshotChangedEvent, WebPageSnapshotDto, WebPageSnapshotLinkEvent,
    WebPageSnapshotStatus, CHANGED_EVENT, LINK_EVENT,
};
pub use gc::{SweepReport, WebSnapshotGc};

use crate::features::web::traits::page_snapshot::PageCapturePort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::http::normalize_web_source_url;
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_core::shared::runtime::jobs::{JobRuntime, NewJob};
use repository::{SnapshotRepository, SnapshotRow};
use store::SnapshotStore;

/// Delivers the feature's events to the main window.
pub trait SnapshotEvents: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

type Emit = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Emits to the main window through the app handle [`AppEvents::attach`]
/// stored. Until then — in tests, or before setup — events go nowhere.
#[derive(Default)]
pub struct AppEvents {
    emit: OnceLock<Emit>,
}

impl AppEvents {
    /// Give the feature the running app so it can emit to the main window.
    pub fn attach<R: tauri::Runtime>(&self, app: tauri::AppHandle<R>) {
        let _ = self
            .emit
            .set(Arc::new(move |event: &str, payload: serde_json::Value| {
                use tauri::Emitter;
                if let Err(error) = app.emit_to("main", event, payload) {
                    tracing::debug!(%error, event, "Could not publish a web snapshot event");
                }
            }));
    }
}

impl SnapshotEvents for AppEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Some(emit) = self.emit.get() {
            emit(event, payload);
        }
    }
}

/// What the service and its job handler share. The handler holds this, not
/// the service, so the job runtime never owns a handle back to itself.
pub(crate) struct Shared {
    pub(crate) repository: SnapshotRepository,
    pub(crate) store: SnapshotStore,
    pub(crate) capture: Arc<dyn PageCapturePort>,
    pub(crate) events: Arc<dyn SnapshotEvents>,
}

impl Shared {
    /// The DTO for `row`; ready rows have their file re-hashed for `verified`.
    pub(crate) async fn dto(
        &self,
        row: &SnapshotRow,
        progress_message: Option<String>,
    ) -> WebPageSnapshotDto {
        let verified = match (row.status(), row.sha256.as_deref()) {
            (WebPageSnapshotStatus::Ready, Some(sha)) => {
                Some(self.store.verify(&row.id, sha).await)
            }
            (WebPageSnapshotStatus::Ready, None) => Some(false),
            _ => None,
        };
        let mut dto = to_dto(row, progress_message, verified);
        dto.reader_text = self
            .repository
            .find_reader_text(
                &row.conversation_id,
                &row.normalized_url,
                &row.requested_url,
            )
            .await
            .ok()
            .flatten();
        dto
    }

    pub(crate) fn emit_changed(&self, snapshot: WebPageSnapshotDto) {
        let payload = WebPageSnapshotChangedEvent {
            conversation_id: snapshot.conversation_id.clone(),
            url: snapshot.requested_url.clone(),
            snapshot,
        };
        match serde_json::to_value(&payload) {
            Ok(value) => self.events.emit(CHANGED_EVENT, value),
            Err(error) => tracing::debug!(%error, "Could not serialize a web snapshot event"),
        }
    }

    /// Reloads the row and announces its current state.
    pub(crate) async fn announce(&self, id: &str) {
        match self.repository.get(id).await {
            Ok(Some(row)) => {
                let dto = self.dto(&row, None).await;
                self.emit_changed(dto);
            }
            Ok(None) => {}
            Err(error) => tracing::debug!(%error, "Could not reload a web snapshot to announce it"),
        }
    }
}

pub(crate) fn to_dto(
    row: &SnapshotRow,
    progress_message: Option<String>,
    verified: Option<bool>,
) -> WebPageSnapshotDto {
    let status = row.status();
    WebPageSnapshotDto {
        id: row.id.clone(),
        conversation_id: row.conversation_id.clone(),
        requested_url: row.requested_url.clone(),
        final_url: row.final_url.clone(),
        normalized_url: row.normalized_url.clone(),
        title: row.title.clone(),
        status,
        method: row.method.clone(),
        captured_at: row.captured_at.clone(),
        byte_size: row.byte_size.map(|size| size.max(0) as u64),
        sha256: row.sha256.clone(),
        resource_count: row.resource_count.max(0) as u32,
        missing_resource_count: row.missing_resource_count.max(0) as u32,
        partial: row.partial,
        partial_reasons: row
            .partial_reasons
            .as_deref()
            .and_then(|reasons| serde_json::from_str(reasons).ok())
            .unwrap_or_default(),
        error_code: row.error_code.clone(),
        error_message: row.error_message.clone(),
        favicon_data_url: row.favicon_data_url.clone(),
        progress_message: if status == WebPageSnapshotStatus::Pending {
            progress_message
        } else {
            None
        },
        verified,
        reader_text: None,
    }
}

/// The frozen-copy feature: one per process.
pub struct WebPageSnapshotService {
    shared: Arc<Shared>,
    jobs: Arc<JobRuntime>,
    gc: WebSnapshotGc,
    background: Arc<BackgroundTasks>,
}

impl WebPageSnapshotService {
    pub fn new(
        pool: SqlitePool,
        root: PathBuf,
        capture: Arc<dyn PageCapturePort>,
        jobs: Arc<JobRuntime>,
        events: Arc<dyn SnapshotEvents>,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        let repository = SnapshotRepository::new(pool);
        let store = SnapshotStore::new(root);
        Self {
            gc: WebSnapshotGc::new(store.clone(), repository.clone()),
            shared: Arc::new(Shared {
                repository,
                store,
                capture,
                events,
            }),
            jobs,
            background,
        }
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    pub fn attach_browser(
        &self,
        browser: Arc<crate::features::web::services::browser_reader::BrowserReader>,
    ) {
        self.shared.capture.attach_browser(browser);
    }

    /// Registers the `web_page_snapshot` job kind, which settles the jobs the
    /// last process left running and starts delivering saved work.
    pub async fn register_job(&self) -> Result<()> {
        self.jobs
            .register(
                job::KIND,
                Arc::new(job::WebPageSnapshotJob::new(Arc::clone(&self.shared))),
                job::job_config(),
            )
            .await
    }

    /// The row for this page in this conversation, created and queued when
    /// there is none. A failed row is queued again only when `retry` is set
    /// (an explicit user action); any other existing row is left alone.
    pub async fn ensure_queued(
        &self,
        conversation_id: &str,
        url: &str,
        retry: bool,
    ) -> Result<SnapshotRow> {
        let requested = url.trim();
        let parsed = url::Url::parse(requested)
            .map_err(|e| AppError::InvalidInput(format!("not a web address: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::InvalidInput(
                "only http and https pages can be frozen".into(),
            ));
        }
        let normalized = normalize_web_source_url(requested);
        let resolved_conversation_id = self
            .shared
            .repository
            .resolve_conversation_for_capture(conversation_id, &normalized)
            .await?;
        let (mut row, created) = self
            .shared
            .repository
            .insert_or_get(
                &store::new_id(),
                &resolved_conversation_id,
                &normalized,
                requested,
            )
            .await?;
        let attempt = if created {
            None
        } else if retry
            && matches!(
                row.status(),
                WebPageSnapshotStatus::Failed | WebPageSnapshotStatus::Ready
            )
            && self.shared.repository.reset_for_retry(&row.id).await?
        {
            Some(store::new_id())
        } else {
            return Ok(row);
        };
        row.status = WebPageSnapshotStatus::Pending.as_str().into();
        row.error_code = None;
        row.error_message = None;
        // Announce pending before the job can run, so a fast capture's
        // `ready` never arrives ahead of it.
        self.shared
            .emit_changed(to_dto(&row, Some("Queued".into()), None));
        match self.submit(&row, attempt.as_deref()).await {
            Ok(job_id) => {
                self.shared.repository.set_job(&row.id, &job_id).await?;
                row.job_id = Some(job_id);
            }
            Err(error) => {
                let message = format!("Could not queue the capture: {error}");
                self.shared
                    .repository
                    .mark_failed(&row.id, "queue", &message)
                    .await?;
                self.shared.announce(&row.id).await;
                return Err(error);
            }
        }
        Ok(row)
    }

    async fn submit(&self, row: &SnapshotRow, attempt: Option<&str>) -> Result<String> {
        let base = format!(
            "web-page-snapshot:{}:{}",
            row.conversation_id, row.normalized_url
        );
        let operation_id = match attempt {
            None => base,
            Some(attempt) => format!("{base}:retry:{attempt}"),
        };
        let job = self
            .jobs
            .submit(&NewJob {
                kind: job::KIND.into(),
                subject_id: Some(row.id.clone()),
                operation_id,
                payload_hash: row.id.clone(),
                requested: json!({
                    "conversationId": row.conversation_id,
                    "url": row.requested_url,
                    "snapshotId": row.id,
                }),
                progress_total: 100,
                message: "Queued".into(),
            })
            .await?;
        Ok(job.id)
    }

    /// `ensure_queued`, answered as the viewer's DTO.
    pub async fn ensure_capture(
        &self,
        conversation_id: &str,
        url: &str,
        retry: bool,
    ) -> Result<WebPageSnapshotDto> {
        let row = self.ensure_queued(conversation_id, url, retry).await?;
        Ok(self.dto(&row).await)
    }

    pub async fn get(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<WebPageSnapshotDto>> {
        let normalized = normalize_web_source_url(url);
        match self
            .shared
            .repository
            .find_for_viewer(conversation_id, &normalized)
            .await?
        {
            Some(row) => Ok(Some(self.dto(&row).await)),
            None => Ok(None),
        }
    }

    pub async fn list(&self, conversation_id: &str) -> Result<Vec<WebPageSnapshotDto>> {
        let rows = self.shared.repository.list(conversation_id).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(self.dto(row).await);
        }
        Ok(out)
    }

    async fn dto(&self, row: &SnapshotRow) -> WebPageSnapshotDto {
        let progress = match (row.status(), row.job_id.as_deref()) {
            (WebPageSnapshotStatus::Pending, Some(job_id)) => self
                .jobs
                .store()
                .get(job_id)
                .await
                .ok()
                .map(|job| job.progress_message),
            (WebPageSnapshotStatus::Pending, None) => Some("Queued".into()),
            _ => None,
        };
        self.shared.dto(row, progress).await
    }

    /// Removes stale partial writes and files no pending or ready row claims.
    pub async fn sweep(&self) -> Result<SweepReport> {
        self.gc.sweep().await
    }

    /// The sweep, run in the background with its outcome logged.
    pub fn sweep_in_background(self: &Arc<Self>, reason: &'static str) {
        let service = Arc::clone(self);
        self.background.spawn(async move {
            match service.sweep().await {
                Ok(report) => tracing::info!(
                    reason,
                    removed_parts = report.removed_parts,
                    removed_orphans = report.removed_orphans,
                    retained = report.retained,
                    bytes_freed = report.bytes_freed,
                    "Swept frozen web snapshots"
                ),
                Err(error) => {
                    tracing::warn!(%error, reason, "Failed to sweep frozen web snapshots")
                }
            }
        });
    }
}
