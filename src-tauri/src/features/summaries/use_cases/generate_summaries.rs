//! Generate and index a document's summaries.
//!
//! Runs off the indexing critical path (see `features::summaries::trigger`):
//! nothing here is awaited by an import, and every failure is logged and
//! skipped rather than propagated, because a missing summary costs a retrieval
//! shortcut while a failed import costs the document.
//!
//! Disabled by default. `enabled` is a constructor flag, and a disabled use
//! case does not read the database, let alone call a model.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::features::summaries::entity::{DocumentSummary, SummaryLevel};
use crate::features::summaries::prompt::{
    self, DocumentPromptInput, SectionPromptInput, SummaryDraft,
};
use crate::features::summaries::repository::SummaryRepositoryPort;
use crate::features::summaries::runtime::SummaryRuntimePort;
use crate::features::summaries::source::SummarySourcePort;
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use lattice_core::application::ports::{EmbeddingPort, LLMPort};
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, EvidenceSelection, GroundedRequest,
};
use lattice_core::shared::error::{AppError, Result};

/// One utility-model call. Generous because this is background work, bounded
/// because a wedged model must not pin a task forever.
const CALL_TIMEOUT: Duration = Duration::from_secs(45);
const VECTOR_CLEANUP_BATCH: u32 = 256;

/// Model calls one run may make before it yields to other documents.
///
/// Summarizing is background work on what is often one local model slot, and
/// a 300-section manual is hundreds of calls: done in one go it held the
/// slot for hours and every other document waited behind it. A run does this
/// share and stops; the next resumes from the calls already finished (kept
/// in `summary_work_progress`). It bounds the work per run, not what is
/// summarized: every section and every part is, over as many runs as it
/// takes.
pub(crate) const CALLS_PER_RUN: usize = 16;

/// How one run over a document ended.
#[derive(Debug, PartialEq, Eq)]
pub enum SummaryRun {
    /// The summaries are published; how many (0: nothing to summarize).
    Published(usize),
    /// The run did its share of model calls; a later run resumes it.
    Paused,
}

/// The run's share of model calls is used up.
struct Paused;

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

    /// Summarize one document to the end, run after run. For callers outside
    /// the queue; the queue's worker calls [`Self::run`] so a long document
    /// yields to others between runs.
    pub async fn execute(&self, document_id: &str) -> Result<usize> {
        self.execute_cancellable(document_id, &CancellationToken::new())
            .await
    }

    /// [`Self::execute`], stopped by `cancel`.
    pub async fn execute_cancellable(
        &self,
        document_id: &str,
        cancel: &CancellationToken,
    ) -> Result<usize> {
        loop {
            if let SummaryRun::Published(written) = self.run(document_id, "", cancel).await? {
                return Ok(written);
            }
        }
    }

    /// One run over one revision of a document: up to [`CALLS_PER_RUN`] model
    /// calls, resuming from what earlier runs over the same revision
    /// finished. Publishes once every summary is in. Runtime and model
    /// failures are errors so the durable queue retains the request for
    /// retry.
    pub async fn run(
        &self,
        document_id: &str,
        revision: &str,
        cancel: &CancellationToken,
    ) -> Result<SummaryRun> {
        if !self.enabled {
            return Ok(SummaryRun::Published(0));
        }
        ensure_active(cancel)?;
        let Some(source) = self.source.load(document_id).await? else {
            return Ok(SummaryRun::Published(0));
        };
        if source.body.trim().is_empty() {
            return Ok(SummaryRun::Published(0));
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
        let mut run = Run {
            llm: llm.as_ref(),
            repository: self.repository.as_ref(),
            document_id,
            revision,
            done: self.repository.load_progress(document_id, revision).await?,
            calls_left: CALLS_PER_RUN,
            finished: 0,
            cancel,
        };
        let summaries = summarize(&mut run, &source, &identity).await;
        ensure_active(cancel)?;
        let Ok(summaries) = summaries else {
            // A run whose every call failed is a failing model, not progress:
            // the queue waits before trying it again rather than at once.
            if run.finished == 0 {
                return Err(AppError::Other(
                    "Summary generation produced no usable output; retry later".into(),
                ));
            }
            tracing::info!(
                document_id,
                finished_calls = run.done.len(),
                "Summary run did its share; the document resumes in a later run"
            );
            return Ok(SummaryRun::Paused);
        };
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
        if let Err(error) = self.repository.clear_progress(document_id).await {
            tracing::warn!(%error, document_id, "Could not clear finished summary calls");
        }
        tracing::info!(
            document_id,
            summaries = summaries.len(),
            "Indexed document summaries"
        );
        Ok(SummaryRun::Published(summaries.len()))
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

/// What one summary is asked for: the call's instructions and where its
/// text comes from.
struct Summarized {
    system: String,
    task: String,
    heading: &'static str,
    source_id: String,
}

/// One run over one document: its model, the calls earlier runs over the
/// same revision finished, and how many more this run may make.
struct Run<'a> {
    llm: &'a dyn LLMPort,
    repository: &'a dyn SummaryRepositoryPort,
    document_id: &'a str,
    revision: &'a str,
    done: HashMap<String, String>,
    calls_left: usize,
    /// Calls this run made that came back with a summary.
    finished: usize,
    cancel: &'a CancellationToken,
}

impl Run<'_> {
    /// The summary `call_id` asks for: an earlier run's when one finished it,
    /// otherwise one model call, kept for the runs after this one.
    async fn ask(
        &mut self,
        call_id: &str,
        request: GroundedRequest,
    ) -> std::result::Result<Option<String>, Paused> {
        if let Some(text) = self.done.get(call_id) {
            return Ok(Some(text.clone()));
        }
        if self.calls_left == 0 {
            return Err(Paused);
        }
        self.calls_left -= 1;
        let text = ask(self.llm, request, self.cancel)
            .await
            .map(SummaryDraft::into_summary_text);
        if let Some(text) = &text {
            if let Err(error) = self
                .repository
                .save_progress(self.document_id, self.revision, call_id, text)
                .await
            {
                tracing::warn!(%error, call_id, "Could not keep a finished summary call; a later run repeats it");
            }
            self.done.insert(call_id.to_string(), text.clone());
            self.finished += 1;
        }
        Ok(text)
    }
}

/// Every summary of `source`. Each part of the text is read once: a long
/// sectioned document is described from its sections' summaries and the text
/// under no heading, not read a second time for its own summary.
async fn summarize(
    run: &mut Run<'_>,
    source: &crate::features::summaries::source::SummarySource,
    identity: &str,
) -> std::result::Result<Vec<DocumentSummary>, Paused> {
    let document_id = source.document_id.as_str();
    let headings: Vec<String> = source
        .sections
        .iter()
        .map(|section| section.heading.clone())
        .collect();
    let document = Summarized {
        system: prompt::document_system_prompt(),
        task: prompt::render_document_task(&DocumentPromptInput {
            title: &source.title,
            cluster_label: source.cluster_label.as_deref(),
            section_headings: &headings,
        }),
        heading: prompt::DOCUMENT_BODY_HEADING,
        source_id: document_id.to_string(),
    };
    let document_summary = |text: String| {
        DocumentSummary::new(document_id, None, SummaryLevel::Document, text, identity)
    };
    // Two sections are a heading style, not a structure worth its own
    // retrieval tier; the document summary already covers them.
    let sectioned = source.sections.len() >= prompt::MIN_SECTIONS;
    let body_fits = window_parts(
        run.llm,
        &document,
        &document.task,
        document.heading,
        &source.body,
    )
    .len()
        <= 1;
    let mut summaries = Vec::new();
    if body_fits || !sectioned {
        if let Some(text) = summarize_whole(run, &document, &source.body).await? {
            summaries.push(document_summary(text));
        }
    }
    if !sectioned {
        return Ok(summaries);
    }

    let system = prompt::section_system_prompt();
    let mut by_section = Vec::new();
    for section in &source.sections {
        if run.cancel.is_cancelled() {
            return Ok(summaries);
        }
        let section_call = Summarized {
            system: system.clone(),
            task: prompt::render_section_task(&SectionPromptInput {
                title: &source.title,
                section: &section.heading,
                cluster_label: source.cluster_label.as_deref(),
            }),
            heading: prompt::SECTION_BODY_HEADING,
            source_id: format!("{document_id}#{}", section.heading),
        };
        if let Some(text) = summarize_whole(run, &section_call, &section.text).await? {
            by_section.push(format!("Section {}: {text}", section.heading));
            summaries.push(DocumentSummary::new(
                document_id,
                Some(section.heading.clone()),
                SummaryLevel::Section,
                text,
                identity,
            ));
        }
    }
    if !body_fits && !by_section.is_empty() {
        let mut text = by_section.join("\n");
        if !source.unsectioned.is_empty() {
            text.push_str("\n\nText under no heading:\n");
            text.push_str(&source.unsectioned);
        }
        let from_sections = Summarized {
            task: format!(
                "{}\nThe document is too long to read at once; summarize the whole of it from its sections' summaries below, and any text under no heading.",
                document.task
            ),
            heading: prompt::SECTION_SUMMARIES_HEADING,
            source_id: format!("{document_id}@sections"),
            ..document
        };
        if let Some(text) = summarize_whole(run, &from_sections, &text).await? {
            summaries.insert(0, document_summary(text));
        }
    }
    Ok(summaries)
}

/// Levels of part summaries before a text is summarized from what fits. Each
/// level shrinks the text to a few sentences per part, so a real document
/// never reaches it; it stops a window too small for one part summary from
/// recursing for ever.
const MAX_SUMMARY_LEVELS: usize = 4;

/// Summarize `text` from all of it: in one call when the window holds it,
/// otherwise part by part, in order, and then from the parts' summaries (and
/// so on, should those not fit either). A long document is described from
/// its whole text rather than its opening. Each call is one of the run's
/// share; when the share runs out the summary resumes in a later run.
async fn summarize_whole(
    run: &mut Run<'_>,
    call: &Summarized,
    text: &str,
) -> std::result::Result<Option<String>, Paused> {
    let cancel = run.cancel.clone();
    let mut task = call.task.clone();
    let mut heading = call.heading;
    let mut text = text.to_string();
    for level in 0..=MAX_SUMMARY_LEVELS {
        let parts = window_parts(run.llm, call, &task, heading, &text);
        if parts.len() <= 1 || level == MAX_SUMMARY_LEVELS {
            if parts.len() > 1 {
                tracing::warn!(
                    source_id = call.source_id,
                    parts = parts.len(),
                    "Summary parts still exceed the window; summarizing from what fits"
                );
            }
            let request = summary_request(call, task, heading, &call.source_id, &text, &cancel);
            return run
                .ask(&format!("{}@L{level}", call.source_id), request)
                .await;
        }
        let total = parts.len();
        let mut part_summaries = Vec::with_capacity(total);
        for (index, part) in parts.iter().enumerate() {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let part_task = format!(
                "{task}\nThe text is too long to read at once. This is part {} of {total}, in order; summarize this part.",
                index + 1
            );
            let id = format!("{}:part-{}", call.source_id, index + 1);
            let request = summary_request(call, part_task, heading, &id, part, &cancel);
            let call_id = format!("{}@L{level}:part-{}", call.source_id, index + 1);
            if let Some(summary) = run.ask(&call_id, request).await? {
                part_summaries.push(format!("Part {}: {summary}", index + 1));
            }
        }
        if part_summaries.is_empty() {
            return Ok(None);
        }
        task = format!(
            "{}\nThe text was too long to read at once; summarize the whole of it from the summaries of its parts below.",
            call.task
        );
        heading = prompt::PART_SUMMARIES_HEADING;
        text = part_summaries.join("\n");
    }
    Ok(None)
}

/// `text` in consecutive parts that each fit one summary call's evidence
/// room, whole passages in order. One part when the window holds it all.
fn window_parts(
    llm: &dyn LLMPort,
    call: &Summarized,
    task: &str,
    heading: &str,
    text: &str,
) -> Vec<String> {
    let bare = summary_request(
        call,
        task.to_string(),
        heading,
        &call.source_id,
        "",
        &CancellationToken::new(),
    );
    let room = grounded_generation::evidence_room(llm, &bare).unwrap_or(0);
    let passages =
        grounded_generation::passages_from_text(&call.source_id, text, prompt::BODY_PASSAGE_CHARS);
    let mut parts: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for passage in &passages {
        let cost = grounded_generation::passage_tokens(llm, passage);
        if !current.is_empty() && used + cost > room {
            parts.push(current.join("\n\n"));
            current.clear();
            used = 0;
        }
        used += cost;
        current.push(&passage.text);
    }
    if !current.is_empty() {
        parts.push(current.join("\n\n"));
    }
    parts
}

/// One grounded summary call: the task, then the source text, whole passages
/// in order as far as the window holds, then the response contract.
fn summary_request(
    call: &Summarized,
    task: String,
    heading: &str,
    source_id: &str,
    text: &str,
    cancel: &CancellationToken,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(call.system.clone(), task);
    request.evidence_heading = heading.to_string();
    request.evidence =
        grounded_generation::passages_from_text(source_id, text, prompt::BODY_PASSAGE_CHARS);
    request.selection = EvidenceSelection::Prefix;
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
