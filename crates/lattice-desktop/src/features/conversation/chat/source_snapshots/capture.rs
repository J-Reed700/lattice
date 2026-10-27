//! Capturing cited pages after the answer is on screen. A cited search
//! snippet whose page the turn never read is captured by reading the page, so
//! the turn persists the answer with a pending marker and returns; this task
//! archives each page, patches the message's sources, tells the window, then
//! starts the grounding check.

use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use tracing::warn;

use super::{archive_page_for_url, web_snapshot, SourceDto};
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::chat::verification::BackgroundVerification;
use crate::features::conversation::chat::ChatStreamEventDto;
use crate::features::conversation::trait_def::ConversationServiceTrait;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// Stream status of the event that carries a finished capture.
pub(in crate::features::conversation) const SOURCES_CAPTURED_STATUS: &str = "captured";

/// The metadata key that says whether the capture is still running.
pub(in crate::features::conversation) const CAPTURE_KEY: &str = "capture";

/// The marker an answer is persisted with while its pages are still read.
pub(in crate::features::conversation) fn pending_capture() -> serde_json::Value {
    serde_json::json!({ "pending": true })
}

/// A cited page this conversation has no archive of yet, and the sources
/// that cite it.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::features::conversation) struct UncapturedPage {
    pub url: String,
    /// Indices into the answer's sources.
    pub sources: Vec<usize>,
}

/// A finished capture, addressed to the message it belongs to.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CapturedSourcesDto {
    pub message_id: String,
    /// The same list persisted under `metadata.sources`, with every page that
    /// could be read carrying its snapshot.
    pub sources: Vec<SourceDto>,
}

/// Everything the capture needs, owned, so it can outlive the turn.
#[derive(Clone)]
pub(in crate::features::conversation) struct CitationCapture {
    pub runtime: Arc<dyn ChatRuntime>,
    pub conversation_service: Arc<dyn ConversationServiceTrait>,
    pub conversation_id: String,
    pub request_id: String,
    pub message_id: String,
    /// The sources as the answer was persisted with them.
    pub sources: Vec<SourceDto>,
    pub pages: Vec<UncapturedPage>,
    pub emit: Arc<dyn Fn(ChatStreamEventDto) + Send + Sync>,
    /// Where the capture runs, so shutdown stops it and marks it interrupted.
    pub background: Arc<BackgroundTasks>,
    /// The grounding check, started once the pages it reads are archived.
    pub then: Option<BackgroundVerification>,
}

impl CitationCapture {
    pub(in crate::features::conversation) async fn spawn(self) {
        let background = Arc::clone(&self.background);
        let cancel = background.token();
        let finalizer = self.clone();
        if background.spawn(self.run(cancel)).is_none() {
            finalizer.finish(true).await;
        }
    }

    async fn run(mut self, cancel: tokio_util::sync::CancellationToken) {
        // Read concurrently, at most four at once. `read_page` validates URLs and
        // applies the web client's own network policy and timeouts, so no second
        // clock is wrapped around it.
        let reads = std::mem::take(&mut self.pages).into_iter().map(|page| {
            let reader = self.runtime.page_reader();
            async move {
                let result = reader.read_page(&page.url).await;
                (page, result)
            }
        });
        let mut reads = stream::iter(reads).buffer_unordered(4);
        let interrupted = loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => break true,
                next = reads.next() => next,
            };
            let Some((page, result)) = next else {
                break false;
            };
            // A page that cannot be read leaves its source without a snapshot,
            // as an answer that never cited it would.
            let Ok(read) = result else {
                continue;
            };
            let runtime = self.runtime.as_ref();
            archive_page_for_url(runtime, &self.conversation_id, &page.url, &read).await;
            if let Some(snapshot) = web_snapshot(runtime, &self.conversation_id, &page.url).await {
                for index in page.sources {
                    if let Some(source) = self.sources.get_mut(index) {
                        source.web_snapshot = Some(snapshot.clone());
                    }
                }
            }
        };
        drop(reads);
        self.finish(interrupted).await;
    }

    /// Persist what was captured, tell the window, and start the check. An
    /// interrupted capture keeps the pages it had already read.
    async fn finish(self, interrupted: bool) {
        let marker = if interrupted {
            serde_json::json!({ "pending": false, "interrupted": true })
        } else {
            serde_json::json!({ "pending": false })
        };
        let fields = vec![
            ("sources".to_string(), serde_json::json!(&self.sources)),
            (CAPTURE_KEY.to_string(), marker),
        ];
        if let Err(error) = self
            .conversation_service
            .set_message_metadata_fields(&self.message_id, fields)
            .await
        {
            // The message may have been deleted while the pages were read. The
            // window is still told, so it stops waiting for them.
            warn!(%error, "Could not persist the cited pages captured for this answer");
        }
        (self.emit)(ChatStreamEventDto {
            status: Some(SOURCES_CAPTURED_STATUS.to_owned()),
            captured: Some(CapturedSourcesDto {
                message_id: self.message_id.clone(),
                sources: self.sources,
            }),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        });
        // Spawned rather than run here, so a shutdown that interrupted the
        // capture finds admission closed and marks the check interrupted too.
        if let Some(verification) = self.then {
            verification.spawn().await;
        }
    }
}
