//! Asking the utility model for a plan, and for a corrected one.
use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::application::contracts::search::CorpusDocument;
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
use crate::application::ports::LLMPort;
use crate::shared::error::{AppError, Result};

use super::{
    ordered_learning_plan, prompt, shortlist, CorpusSearchPlan, PlannerTurn, PLANNER_SYSTEM,
    PLANNER_TIMEOUT,
};

/// What the first retrieval pass already tried. Serialized into the planner's
/// user turn so a correction can ask for different queries without a second
/// system prompt to keep in sync with `PLANNER_SYSTEM`.
#[derive(Debug, Default, Serialize)]
pub(in super::super) struct CorrectionRequest {
    pub queries_already_tried: Vec<String>,
    pub top_result_titles: Vec<String>,
    pub why_insufficient: Vec<&'static str>,
}

/// Plan retrieval from a shortlist of the catalog, showing each document's
/// generated summary alongside its title and opening.
///
/// `summaries` is empty whenever the summary tier is off, which is the default,
/// and an empty map leaves the rows without a summary field. `turn` carries the
/// local search's candidate documents and this turn's timeout guard.
pub(in super::super) async fn plan(
    llm: &dyn LLMPort,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    summaries: &HashMap<String, String>,
    turn: &PlannerTurn<'_>,
    cancel: &CancellationToken,
) -> Result<CorpusSearchPlan> {
    if let Some(plan) = ordered_learning_plan(question, catalog) {
        return Ok(plan);
    }
    plan_with(
        llm, question, history, catalog, None, summaries, turn, cancel,
    )
    .await
}

/// One replanned retry for a turn whose first pass was judged insufficient.
///
/// The ordered-learning shortcut is deliberately not consulted: the first pass
/// already established this is a focused search, and a correction that answered
/// with the same front matter would not be a correction. Neither is a plan that
/// repeats a query that already failed.
pub(in super::super) async fn plan_correction(
    llm: &dyn LLMPort,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    correction: &CorrectionRequest,
    turn: &PlannerTurn<'_>,
    cancel: &CancellationToken,
) -> Result<CorpusSearchPlan> {
    let tried: HashSet<String> = correction
        .queries_already_tried
        .iter()
        .map(|query| query.to_lowercase())
        .collect();
    let plan = plan_with(
        llm,
        question,
        history,
        catalog,
        Some(correction),
        &HashMap::new(),
        turn,
        cancel,
    )
    .await?;
    let queries: Vec<String> = plan
        .queries
        .into_iter()
        .filter(|query| !tried.contains(&query.to_lowercase()))
        .collect();
    if queries.is_empty() {
        return Err(AppError::InvalidState(
            "Corrective retrieval planning returned no new queries".into(),
        ));
    }
    Ok(CorpusSearchPlan {
        queries,
        opening_document_ids: Vec::new(),
        start_at_beginning: false,
    })
}

/// Whether the server refused a request as larger than its context window.
/// llama-server names the error type; the adapter keeps the name and drops
/// the server's message.
fn is_window_overflow(error: &AppError) -> bool {
    error.to_string().contains("exceed_context_size")
}

async fn plan_with(
    llm: &dyn LLMPort,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    correction: Option<&CorrectionRequest>,
    summaries: &HashMap<String, String>,
    turn: &PlannerTurn<'_>,
    cancel: &CancellationToken,
) -> Result<CorpusSearchPlan> {
    if let Some(reason) = turn.unavailable() {
        return Err(AppError::InvalidState(format!(
            "Document retrieval planning skipped: {reason}"
        )));
    }
    let target = turn.health.prompt_tokens();
    let history = prompt::recent_history(llm, history, target.min(llm.max_context_tokens()));
    let ranked = shortlist::shortlist(catalog, turn.candidate_ids(), question);
    let parts = prompt::PromptParts {
        question,
        history: &history,
        catalog_total: catalog.len(),
        correction,
        summaries,
    };
    let prompt = prompt::build(llm, &parts, &ranked, catalog, target).await?;
    tracing::info!(
        catalog_total = catalog.len(),
        candidates = turn.candidate_ids().len(),
        shown = prompt.shown,
        prompt_tokens = prompt.tokens,
        exact_count = prompt.exact,
        prompt_limit = prompt.limit,
        correction = correction.is_some(),
        "Corpus planner prompt"
    );
    let started = tokio::time::Instant::now();
    let response = tokio::time::timeout(
        PLANNER_TIMEOUT,
        llm.complete(&CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: PLANNER_SYSTEM.into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: prompt.user.clone(),
                },
            ],
            json_schema: Some(serde_json::json!({
                "type":"object", "additionalProperties":false,
                "properties":{
                    "queries":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":3},
                    "opening_document_ids":{"type":"array","items":{"type":"string"},"maxItems":4},
                    "start_at_beginning":{"type":"boolean"}
                }, "required":["queries","opening_document_ids","start_at_beginning"]
            })),
            max_output_tokens: Some(prompt::PLANNER_OUTPUT_TOKENS),
            priority: crate::application::ports::llm_port::InferencePriority::Interactive,
            // A stop press takes the plan out of the backend's queue.
            cancel: Some(cancel.clone()),
            ..Default::default()
        }),
    )
    .await;
    let response = match response {
        Err(_) => {
            turn.record_timeout(prompt.tokens);
            tracing::warn!(
                timeout_s = PLANNER_TIMEOUT.as_secs(),
                prompt_tokens = prompt.tokens,
                next_prompt_tokens = turn.health.prompt_tokens(),
                "Corpus planner timed out; the next attempt gets a smaller prompt"
            );
            return Err(AppError::InvalidState(
                "Document retrieval planning timed out".into(),
            ));
        }
        Ok(Err(error)) => {
            if is_window_overflow(&error) {
                turn.health.overflowed(prompt.tokens);
                tracing::warn!(
                    prompt_tokens = prompt.tokens,
                    exact_count = prompt.exact,
                    window = llm.max_context_tokens(),
                    "The server refused the planner prompt as too large for its window; \
                     the reported window is larger than the real one"
                );
            }
            return Err(error);
        }
        Ok(Ok(response)) => response.text,
    };
    turn.health.answered(started.elapsed());
    let response = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let mut plan = serde_json::from_str::<CorpusSearchPlan>(response)?;
    // The planner names documents by the handles it was shown. Anything else
    // passes through, and validation drops what is not in the catalog.
    plan.opening_document_ids = plan
        .opening_document_ids
        .into_iter()
        .map(|id| {
            prompt
                .handles
                .get(id.trim().to_lowercase().as_str())
                .cloned()
                .unwrap_or(id)
        })
        .collect();
    plan.validate(catalog)
}
