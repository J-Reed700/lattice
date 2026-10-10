//! Generate and index a document's summaries.
//!
//! Runs off the indexing critical path (see `features::summaries::trigger`):
//! nothing here is awaited by an import, and every failure is logged and
//! skipped rather than propagated, because a missing summary costs a retrieval
//! shortcut while a failed import costs the document.
//!
//! Disabled by default. `enabled` is a constructor flag, and a disabled use
//! case does not read the database, let alone call a model.

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::application::ports::llm_port::InferencePriority;
use crate::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::application::services::grounded_generation::{
    self, CallOptions, EvidenceSelection, GroundedRequest,
};
use crate::features::summaries::entity::{DocumentSummary, SummaryLevel};
use crate::features::summaries::prompt::{
    self, DocumentPromptInput, SectionPromptInput, SummaryDraft,
};
use crate::features::summaries::repository::SummaryRepositoryPort;
use crate::features::summaries::runtime::SummaryRuntimePort;
use crate::features::summaries::source::SummarySourcePort;
use crate::shared::error::{AppError, Result};

/// One utility-model call. Generous because this is background work, bounded
/// because a wedged model must not pin a task forever.
const CALL_TIMEOUT: Duration = Duration::from_secs(45);
const VECTOR_CLEANUP_BATCH: u32 = 256;

pub struct GenerateDocumentSummariesUseCase {
    enabled: bool,
    identity: String,
    source: Arc<dyn SummarySourcePort>,
    repository: Arc<dyn SummaryRepositoryPort>,
    index: Arc<dyn VectorSearchPort>,
    runtime: Arc<dyn SummaryRuntimePort>,
}

impl GenerateDocumentSummariesUseCase {
    /// `enabled` is the opt-in flag for the whole tier; pass `false` (the
    /// product default) and every call becomes a no-op.
    pub fn new(
        enabled: bool,
        identity: impl Into<String>,
        source: Arc<dyn SummarySourcePort>,
        repository: Arc<dyn SummaryRepositoryPort>,
        index: Arc<dyn VectorSearchPort>,
        runtime: Arc<dyn SummaryRuntimePort>,
    ) -> Self {
        Self {
            enabled,
            identity: identity.into(),
            source,
            repository,
            index,
            runtime,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Summarize one freshly indexed document. Returns how many summaries were
    /// written; `0` means ineligible source. Runtime/model failures are errors
    /// so the durable queue retains the request for retry.
    pub async fn execute(&self, document_id: &str) -> Result<usize> {
        self.execute_cancellable(document_id, &CancellationToken::new())
            .await
    }

    /// Cancel model work promptly, but join an admitted index publication so
    /// shutdown/deletion cannot race a detached mutation.
    pub async fn execute_cancellable(
        &self,
        document_id: &str,
        cancel: &CancellationToken,
    ) -> Result<usize> {
        if !self.enabled {
            return Ok(0);
        }
        ensure_active(cancel)?;
        let Some(source) = self.source.load(document_id).await? else {
            return Ok(0);
        };
        if source.body.trim().is_empty() {
            return Ok(0);
        }
        let llm = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AppError::Other("Summary generation cancelled".into())),
            value = self.runtime.utility_llm() => value,
        };
        let Some(llm) = llm else {
            return Err(AppError::InvalidState(
                "Summary utility model is not ready; retry later".into(),
            ));
        };
        let embedder = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AppError::Other("Summary generation cancelled".into())),
            value = self.runtime.embedder() => value,
        };
        let Some(embedder) = embedder else {
            return Err(AppError::InvalidState(
                "Summary embedder is not ready; retry later".into(),
            ));
        };
        let identity = embedder.model_identity();
        if identity != self.identity {
            return Err(AppError::InvalidState(format!(
                "Summary embedder identity changed from {} to {identity}; retry after re-registration",
                self.identity
            )));
        }
        let headings: Vec<String> = source
            .sections
            .iter()
            .map(|section| section.heading.clone())
            .collect();
        let document_request = summary_request(
            prompt::document_system_prompt(),
            prompt::render_document_task(&DocumentPromptInput {
                title: &source.title,
                cluster_label: source.cluster_label.as_deref(),
                section_headings: &headings,
            }),
            prompt::DOCUMENT_BODY_HEADING,
            document_id,
            &source.body,
            prompt::DOCUMENT_BODY_TOKENS,
            cancel,
        );
        let mut summaries = Vec::new();
        if let Some(draft) = ask(llm.as_ref(), document_request, cancel).await {
            summaries.push(DocumentSummary::new(
                document_id,
                None,
                SummaryLevel::Document,
                draft.into_summary_text(),
                &identity,
            ));
        }

        // Two sections are a heading style, not a structure worth its own
        // retrieval tier; the document summary already covers them.
        if source.sections.len() >= prompt::MIN_SECTIONS {
            let system = prompt::section_system_prompt();
            for section in source.sections.iter().take(prompt::MAX_SECTION_SUMMARIES) {
                ensure_active(cancel)?;
                let section_request = summary_request(
                    system.clone(),
                    prompt::render_section_task(&SectionPromptInput {
                        title: &source.title,
                        section: &section.heading,
                        cluster_label: source.cluster_label.as_deref(),
                    }),
                    prompt::SECTION_BODY_HEADING,
                    &format!("{document_id}#{}", section.heading),
                    &section.text,
                    prompt::SECTION_BODY_TOKENS,
                    cancel,
                );
                if let Some(draft) = ask(llm.as_ref(), section_request, cancel).await {
                    summaries.push(DocumentSummary::new(
                        document_id,
                        Some(section.heading.clone()),
                        SummaryLevel::Section,
                        draft.into_summary_text(),
                        &identity,
                    ));
                }
            }
        }

        ensure_active(cancel)?;
        if summaries.is_empty() {
            return Err(AppError::Other(
                "Summary generation produced no usable output; retry later".into(),
            ));
        }
        self.publish(
            document_id,
            &summaries,
            &identity,
            embedder.as_ref(),
            cancel,
        )
        .await?;
        tracing::info!(
            document_id,
            summaries = summaries.len(),
            "Indexed document summaries"
        );
        Ok(summaries.len())
    }

    /// Drop a document's summaries and their vectors.
    ///
    /// The migration's `ON DELETE CASCADE` already removes the rows when the
    /// document goes, so this exists to take the vectors with them, and must
    /// run *before* the document row is deleted while the ids are still
    /// readable. Skipping it is survivable — a vector whose row is gone is
    /// filtered out at read time — but it leaves dead weight in the index.
    pub async fn forget(&self, document_id: &str) -> Result<()> {
        self.repository
            .delete_for_document(document_id, &self.identity)
            .await?;
        self.drain_vector_cleanups().await?;
        Ok(())
    }

    /// Retry durable vector removals independently of summary generation. The
    /// tombstone is acknowledged only after the derived index accepts removal.
    pub async fn drain_vector_cleanups(&self) -> Result<usize> {
        let vector_ids = self
            .repository
            .pending_vector_cleanup(&self.identity, VECTOR_CLEANUP_BATCH)
            .await?;
        if vector_ids.is_empty() {
            return Ok(0);
        }
        let index = Arc::clone(&self.index);
        let ids = vector_ids.clone();
        tokio::task::spawn_blocking(move || index.remove_embeddings(&ids))
            .await
            .map_err(|error| {
                AppError::Other(format!("Summary index cleanup task failed: {error}"))
            })??;
        self.repository
            .acknowledge_vector_cleanup(&self.identity, &vector_ids)
            .await?;
        Ok(vector_ids.len())
    }

    /// Embed the new summaries and swap them for the old ones in the summary
    /// index. Rows are already committed, so a failure here leaves summaries
    /// that the next regeneration will republish — never a dangling vector
    /// that outranks a real document.
    async fn publish(
        &self,
        document_id: &str,
        summaries: &[DocumentSummary],
        identity: &str,
        embedder: &dyn EmbeddingPort,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let texts: Vec<String> = summaries
            .iter()
            .map(|summary| summary.summary_text.clone())
            .collect();
        let vectors = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AppError::Other("Summary generation cancelled".into())),
            result = embedder.embed_batch(&texts) => result?,
        };
        if vectors.len() != summaries.len() {
            return Err(AppError::InvalidState(format!(
                "summary embedding returned {} vectors for {} summaries",
                vectors.len(),
                summaries.len()
            )));
        }
        let entries: Vec<VectorIndexEntry> = summaries
            .iter()
            .zip(vectors)
            .map(|(summary, embedding)| VectorIndexEntry {
                id: summary.vector_id(),
                embedding,
                content: summary.summary_text.clone(),
                chunk_id: summary.id.clone(),
                document_id: summary.document_id.clone(),
            })
            .collect();
        ensure_active(cancel)?;
        // Keep the old rows/ids available until embeddings are ready. Once
        // replacement begins, join publication even when shutdown is requested.
        self.repository
            .replace_for_document(document_id, identity, summaries)
            .await?;
        self.drain_vector_cleanups().await?;
        let index = Arc::clone(&self.index);
        // HNSW insertion and its disk snapshot are blocking work.
        tokio::task::spawn_blocking(move || index.publish_embeddings(entries))
            .await
            .map_err(|error| AppError::Other(format!("Summary index task failed: {error}")))?
            .map_err(|error| {
                AppError::Other(format!(
                    "Summaries saved for {document_id}, but summary indexing failed: {error}"
                ))
            })
    }
}

fn ensure_active(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(AppError::Other("Summary generation cancelled".into()))
    } else {
        Ok(())
    }
}

/// One grounded summary call: the task, then as much of the source text's
/// opening as the window and `ceiling` allow, then the response contract.
fn summary_request(
    system: String,
    task: String,
    heading: &str,
    source_id: &str,
    text: &str,
    ceiling: usize,
    cancel: &CancellationToken,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(system, task);
    request.evidence_heading = heading.to_string();
    request.evidence =
        grounded_generation::passages_from_text(source_id, text, prompt::BODY_PASSAGE_CHARS);
    request.selection = EvidenceSelection::Prefix;
    request.evidence_limit = Some(ceiling);
    request.closing = prompt::response_contract().to_string();
    request.call = CallOptions {
        // Upkeep: queued behind anyone waiting on the model.
        priority: InferencePriority::Maintenance,
        cancel: Some(cancel.clone()),
        ..Default::default()
    };
    request
}

/// One model call, parsed. `None` on timeout, transport failure, a source the
/// window cannot hold any of, or output that carries no summary — each means
/// "no summary", and the caller treats them identically.
async fn ask(
    llm: &dyn LLMPort,
    request: GroundedRequest,
    cancel: &CancellationToken,
) -> Option<SummaryDraft> {
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return None,
        result = tokio::time::timeout(CALL_TIMEOUT, grounded_generation::generate(llm, request)) => result,
    };
    let response = match result {
        Ok(Ok(output)) => {
            tracing::debug!(
                passages = output.used_evidence_ids.len(),
                left_out = output.accounting.evicted.len(),
                "Summary request planned"
            );
            output.text
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "Summary generation call failed");
            return None;
        }
        Err(_) => {
            tracing::warn!("Summary generation call timed out");
            return None;
        }
    };
    let draft = prompt::parse_summary_response(&response);
    if draft.is_none() {
        tracing::warn!(
            response = %response.chars().take(200).collect::<String>(),
            "Could not parse a summary from the model response"
        );
    }
    draft
}
