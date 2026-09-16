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

use crate::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use crate::application::ports::{EmbeddingPort, LLMPort};
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

pub struct GenerateDocumentSummariesUseCase {
    enabled: bool,
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
        source: Arc<dyn SummarySourcePort>,
        repository: Arc<dyn SummaryRepositoryPort>,
        index: Arc<dyn VectorSearchPort>,
        runtime: Arc<dyn SummaryRuntimePort>,
    ) -> Self {
        Self {
            enabled,
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
    /// written; `0` means "nothing to do", never "something broke".
    pub async fn execute(&self, document_id: &str) -> Result<usize> {
        if !self.enabled {
            return Ok(0);
        }
        let Some(source) = self.source.load(document_id).await? else {
            return Ok(0);
        };
        if source.body.trim().is_empty() {
            return Ok(0);
        }
        let Some(llm) = self.runtime.utility_llm().await else {
            tracing::debug!(document_id, "No utility model; skipping document summary");
            return Ok(0);
        };
        let Some(embedder) = self.runtime.embedder().await else {
            tracing::debug!(document_id, "No embedder; skipping document summary");
            return Ok(0);
        };
        let identity = embedder.model_identity();
        let count = |text: &str| llm.count_tokens(text);

        let headings: Vec<String> = source
            .sections
            .iter()
            .map(|section| section.heading.clone())
            .collect();
        let document_prompt = prompt::render_document_prompt(&DocumentPromptInput {
            title: &source.title,
            cluster_label: source.cluster_label.as_deref(),
            section_headings: &headings,
            body: &prompt::truncate_to_tokens(&source.body, prompt::DOCUMENT_BODY_TOKENS, count),
        });
        let mut summaries = Vec::new();
        if let Some(draft) = ask(
            llm.as_ref(),
            &prompt::document_system_prompt(),
            &document_prompt,
        )
        .await
        {
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
                let section_prompt = prompt::render_section_prompt(&SectionPromptInput {
                    title: &source.title,
                    section: &section.heading,
                    cluster_label: source.cluster_label.as_deref(),
                    body: &prompt::truncate_to_tokens(
                        &section.text,
                        prompt::SECTION_BODY_TOKENS,
                        count,
                    ),
                });
                if let Some(draft) = ask(llm.as_ref(), &system, &section_prompt).await {
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

        if summaries.is_empty() {
            tracing::warn!(document_id, "Summary generation produced nothing usable");
            return Ok(0);
        }
        let replaced = self
            .repository
            .replace_for_document(document_id, &identity, &summaries)
            .await?;
        self.publish(document_id, &summaries, &replaced, embedder.as_ref())
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
        let removed = self.repository.delete_for_document(document_id).await?;
        if removed.is_empty() {
            return Ok(());
        }
        let keys: Vec<String> = removed
            .iter()
            .map(|id| crate::features::summaries::entity::vector_id(id))
            .collect();
        let index = Arc::clone(&self.index);
        tokio::task::spawn_blocking(move || index.remove_embeddings(&keys))
            .await
            .map_err(|error| AppError::Other(format!("Summary index task failed: {error}")))?
    }

    /// Embed the new summaries and swap them for the old ones in the summary
    /// index. Rows are already committed, so a failure here leaves summaries
    /// that the next regeneration will republish — never a dangling vector
    /// that outranks a real document.
    async fn publish(
        &self,
        document_id: &str,
        summaries: &[DocumentSummary],
        replaced: &[String],
        embedder: &dyn EmbeddingPort,
    ) -> Result<()> {
        let texts: Vec<String> = summaries
            .iter()
            .map(|summary| summary.summary_text.clone())
            .collect();
        let vectors = embedder.embed_batch(&texts).await?;
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
        let stale: Vec<String> = replaced
            .iter()
            .map(|id| crate::features::summaries::entity::vector_id(id))
            .collect();
        let index = Arc::clone(&self.index);
        // HNSW insertion and its disk snapshot are blocking work.
        tokio::task::spawn_blocking(move || {
            index.remove_embeddings(&stale)?;
            index.publish_embeddings(entries)
        })
        .await
        .map_err(|error| AppError::Other(format!("Summary index task failed: {error}")))?
        .map_err(|error| {
            AppError::Other(format!(
                "Summaries saved for {document_id}, but summary indexing failed: {error}"
            ))
        })
    }
}

/// One model call, parsed. `None` on timeout, transport failure, or output
/// that carries no summary — all three mean "no summary", and the caller
/// treats them identically.
async fn ask(llm: &dyn LLMPort, system: &str, user_prompt: &str) -> Option<SummaryDraft> {
    // `LLMPort::generate` takes a prompt plus context; the system prompt goes
    // in as context, matching `corpus_shape::labeling`.
    let context = [system.to_string()];
    let response =
        match tokio::time::timeout(CALL_TIMEOUT, llm.generate(user_prompt, &context, None)).await {
            Ok(Ok(response)) => response,
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
