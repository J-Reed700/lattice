//! HyDE Generator Service
//!
//! Generates hypothetical document embeddings (HyDE) for query expansion.
//! Uses LLM to rewrite questions into retrieval-oriented intent expansions
//! that semantically match what relevant documents would contain.
//!
//! ## Architecture
//!
//! ```text
//! QueryType + Query Text
//!         ↓
//!   HyDEGenerator
//!         ↓
//!   ┌─────┴─────┐
//!   │           │
//! Greeting   Question/Command
//!   │           │
//! Fast-path   LLM Call
//!   │           │
//!   └─────┬─────┘
//!         ↓
//! HyDEInterpretation
//! ```

use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::qa::hyde::{HyDEInterpretation, QueryType};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::safe_truncate;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

mod prompts;
mod text;

use prompts::{
    COMMAND_HYDE_TEMPLATE, FOLLOWUP_CLASSIFIER_TEMPLATE, FOLLOWUP_HYDE_TEMPLATE,
    QUESTION_HYDE_TEMPLATE, RESEARCH_FOLLOWUPS_TEMPLATE, WEB_SEARCH_QUERY_TEMPLATE,
    WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE,
};
use text::{
    build_retrieval_fallback, compact_hyde_text, has_placeholder_artifacts,
    looks_like_direct_answer, normalize_web_search_query, parse_research_followups,
    MAX_WEB_SEARCH_QUERY_WORDS,
};

/// Wall-clock cap for one query-planning completion, including queueing and
/// prompt processing. Local models can spend more than 30 seconds on those
/// phases alone, especially when classification and rewriting share a model.
/// Allow three minutes before falling back, while retaining a finite bound
/// below the provider's ten-minute default for these pre-retrieval calls.
const REWRITE_TIME_BUDGET: Duration = Duration::from_secs(3 * 60);

/// Service for generating HyDE interpretations.
///
/// Coordinates LLM calls to expand queries into hypothetical documents
/// for semantic search enhancement.
///
/// # Design Decision: No Tool Intent Classification
///
/// Tool intent classification (vault_only, web_only, etc.) was previously
/// performed as a second LLM call per query, but the result was never consumed
/// by the RAG pipeline. This added 500-2000ms of wasted latency per query.
/// The classification has been removed; if routing is needed in the future,
/// it should be wired end-to-end before being re-enabled.
pub struct HyDEGenerator {
    llm: Arc<dyn LLMPort>,
    /// The turn's stop button, when the rewrites run inside one.
    cancel: Option<CancellationToken>,
}

impl HyDEGenerator {
    /// Create a new HyDE generator with an LLM backend.
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self { llm, cancel: None }
    }

    /// Withdraw every rewrite from the backend's queue, or abort it, when
    /// `cancel` fires.
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Run a self-contained rewrite prompt with the caller's time budget.
    ///
    /// Keep the model's normal reasoning behavior. A truncated or failed
    /// completion is an error, never a partial rewrite.
    async fn rewrite(&self, prompt: &str) -> Result<String> {
        let response = self
            .llm
            .complete(&CompletionRequest {
                input: vec![CompletionInput::Message {
                    role: "user".into(),
                    content: prompt.to_string(),
                }],
                time_budget: Some(REWRITE_TIME_BUDGET),
                // Rewrites run inside a chat turn, which someone is waiting on.
                priority:
                    lattice_core::application::ports::llm_port::InferencePriority::Interactive,
                cancel: self.cancel.clone(),
                ..Default::default()
            })
            .await?;
        if !response.tool_calls.is_empty() {
            return Err(AppError::InvalidState(
                "Unexpected tool call in query rewrite".into(),
            ));
        }
        if matches!(
            response.finish_reason.as_str(),
            "length" | "max_tokens" | "incomplete" | "failed"
        ) {
            return Err(AppError::InvalidState(format!(
                "Query rewrite stopped before completing: {}",
                response.finish_reason
            )));
        }
        Ok(response.text)
    }

    /// Classify whether a query is context-dependent follow-up.
    pub async fn classify_followup_with_context(
        &self,
        query: &str,
        conversation_context: &str,
    ) -> Result<bool> {
        if query.trim().is_empty() || conversation_context.trim().is_empty() {
            return Ok(false);
        }

        let prompt = FOLLOWUP_CLASSIFIER_TEMPLATE
            .replace("{query}", query)
            .replace("{context}", conversation_context);
        let output = self.rewrite(&prompt).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "Follow-up classifier LLM call failed"
            );
            AppError::Other(format!("Follow-up classification failed: {}", e))
        })?;

        let decision = output
            .chars()
            .find(|ch| *ch == '0' || *ch == '1')
            .unwrap_or('0');
        Ok(decision == '1')
    }

    /// Generate a search-bar-ready web query from user input and optional context.
    pub async fn generate_web_search_query(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<String> {
        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        let prompt =
            if let Some(context) = conversation_context.filter(|ctx| !ctx.trim().is_empty()) {
                WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE
                    .replace("{context}", context)
                    .replace("{query}", query)
            } else {
                WEB_SEARCH_QUERY_TEMPLATE.replace("{query}", query)
            };

        debug!(
            query_len = query.len(),
            context_len = conversation_context.map(|ctx| ctx.len()).unwrap_or(0),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE web query prompt prepared"
        );

        let generated = self.rewrite(&prompt).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE web query generation LLM call failed"
            );
            AppError::Other(format!("HyDE web query generation failed: {}", e))
        })?;

        let normalized = normalize_web_search_query(generated.trim());
        if normalized.is_empty() {
            return Ok(query.trim().to_string());
        }

        let word_count = normalized.split_whitespace().count();
        if word_count > MAX_WEB_SEARCH_QUERY_WORDS {
            // The caller falls back to its lexical query builder, which is
            // built from the reader's own words and bounded by construction.
            warn!(
                word_count,
                query_preview = %safe_truncate(&normalized, 180),
                "Discarding a web-search rewrite that came back as a retelling, not a query"
            );
            return Err(AppError::InvalidState(format!(
                "Query rewrite returned {word_count} words; a search query may have at most \
                 {MAX_WEB_SEARCH_QUERY_WORDS}"
            )));
        }

        info!(
            query_len = normalized.len(),
            query_preview = %safe_truncate(&normalized, 180),
            "Generated web-search query via HyDE"
        );
        Ok(normalized)
    }

    /// Up to `count` further searches on the subject of `root`, each from a
    /// different angle, for deep research to run after it.
    ///
    /// Fewer than `count` — or none — when the model's lines are not usable
    /// queries. Deep research then searches less; it never pads with noise.
    pub async fn generate_research_followups(
        &self,
        root: &str,
        query: &str,
        conversation_context: Option<&str>,
        count: usize,
    ) -> Result<Vec<String>> {
        if count == 0 || root.trim().is_empty() {
            return Ok(Vec::new());
        }
        let context = conversation_context
            .filter(|ctx| !ctx.trim().is_empty())
            .unwrap_or("(none)");
        let prompt = RESEARCH_FOLLOWUPS_TEMPLATE
            .replace("{count}", &count.to_string())
            .replace("{context}", context)
            .replace("{query}", query)
            .replace("{root}", root.trim());
        let generated = self.rewrite(&prompt).await.map_err(|e| {
            error!(error = %e, root = %root, "Research follow-up generation failed");
            AppError::Other(format!("Research follow-up generation failed: {e}"))
        })?;
        let followups = parse_research_followups(&generated, root, count);
        info!(
            requested = count,
            written = followups.len(),
            followups = ?followups,
            "Generated deep-research follow-up searches"
        );
        Ok(followups)
    }

    /// Generate HyDE interpretation for a query.
    ///
    /// Handles different query types:
    /// - **Greeting**: Fast-path, returns immediately without LLM call
    /// - **Question**: Calls LLM to generate hypothetical answer document
    /// - **Command**: Calls LLM to expand command interpretation
    /// - **Followup**: Currently uses question template (future: context-aware)
    pub async fn generate(
        &self,
        query_type: QueryType,
        query: impl Into<String>,
    ) -> Result<HyDEInterpretation> {
        self.generate_with_context(query_type, query, None).await
    }

    /// Generate HyDE interpretation for a query with optional conversation context.
    pub async fn generate_with_context(
        &self,
        query_type: QueryType,
        query: impl Into<String>,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        let query = query.into();

        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        match query_type {
            QueryType::Greeting => self.generate_greeting(query).await,
            QueryType::Question => self.generate_question(query).await,
            QueryType::Command => self.generate_command(query).await,
            QueryType::Followup => self.generate_followup(query, conversation_context).await,
        }
    }

    /// Fast-path for greetings - no LLM call needed.
    async fn generate_greeting(&self, query: String) -> Result<HyDEInterpretation> {
        debug!("HyDE fast-path for greeting: {}", query);
        Ok(HyDEInterpretation::for_greeting(query))
    }

    /// Generate retrieval-focused expansion text for a question.
    async fn generate_question(&self, query: String) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for question: {}", query);

        let prompt = QUESTION_HYDE_TEMPLATE.replace("{query}", &query);
        debug!(
            query_len = query.len(),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE question prompt prepared"
        );

        let hyde_text = self.rewrite(&prompt).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE question generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if looks_like_direct_answer(&query, &hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE question output resembled a direct answer; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE question output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Question));
        }

        info!(
            "Generated HyDE ({} chars) for question: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE question output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE question output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::for_question(query, hyde_text))
    }

    /// Generate command interpretation.
    async fn generate_command(&self, query: String) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for command: {}", query);

        let prompt = COMMAND_HYDE_TEMPLATE.replace("{query}", &query);
        debug!(
            query_len = query.len(),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE command prompt prepared"
        );

        let hyde_text = self.rewrite(&prompt).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE command generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if looks_like_direct_answer(&query, &hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE followup output resembled a direct answer; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE command output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text for command, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Command));
        }

        info!(
            "Generated HyDE ({} chars) for command: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE command output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE command output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::hybrid(
            query,
            hyde_text,
            QueryType::Command,
        ))
    }

    /// Generate interpretation for follow-up question.
    async fn generate_followup(
        &self,
        query: String,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for followup: {}", query);

        let prompt =
            if let Some(context) = conversation_context.filter(|ctx| !ctx.trim().is_empty()) {
                FOLLOWUP_HYDE_TEMPLATE
                    .replace("{context}", context)
                    .replace("{query}", &query)
            } else {
                QUESTION_HYDE_TEMPLATE.replace("{query}", &query)
            };
        debug!(
            query_len = query.len(),
            context_len = conversation_context.map(|ctx| ctx.len()).unwrap_or(0),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE followup prompt prepared"
        );

        let hyde_text = self.rewrite(&prompt).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE followup generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE followup output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text for followup, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Followup));
        }

        info!(
            "Generated HyDE ({} chars) for followup: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE followup output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE followup output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::hybrid(
            query,
            hyde_text,
            QueryType::Followup,
        ))
    }
}

#[cfg(test)]
mod tests;
