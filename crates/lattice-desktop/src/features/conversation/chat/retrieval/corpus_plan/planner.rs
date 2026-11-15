//! Asking the utility model for a plan, and for a corrected one.
use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use lattice_core::application::contracts::search::CorpusDocument;
use lattice_core::application::ports::hybrid_search::HybridSearchTrait;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::{ask, Plain, Settled};
use lattice_core::application::services::model_contract::shown::{Scope, Told};
use lattice_core::shared::error::{AppError, Result};

use super::answer::{Correction, Plan, PlanContract, PlanInput};
use super::{ordered_learning_plan, prompt, shortlist, CorpusSearchPlan, PlannerTurn};

/// What the first retrieval pass already tried. Serialized into the planner's
/// user turn so a correction can ask for different queries without a second
/// system prompt to keep in sync with `PLANNER_SYSTEM`.
#[derive(Debug, Default, Serialize)]
pub(in super::super) struct CorrectionRequest {
    pub queries_already_tried: Vec<String>,
    /// Every distinct title the first pass returned, best first.
    pub top_result_titles: Vec<String>,
    pub why_insufficient: Vec<&'static str>,
    /// Titles past those the prompt has room for, counted.
    #[serde(skip_serializing_if = "is_zero")]
    pub more_titles_not_shown: usize,
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

impl CorrectionRequest {
    /// This correction with as many of its titles, best first, as `room`
    /// tokens hold, and the rest counted.
    pub(super) fn fitted(&self, llm: &dyn LLMPort, room: usize) -> Self {
        let mut used = 0usize;
        let shown: Vec<String> = self
            .top_result_titles
            .iter()
            .take_while(|title| {
                used += llm.count_tokens(title) + 1;
                used <= room
            })
            .cloned()
            .collect();
        Self {
            queries_already_tried: self.queries_already_tried.clone(),
            more_titles_not_shown: self.top_result_titles.len() - shown.len(),
            top_result_titles: shown,
            why_insufficient: self.why_insufficient.clone(),
        }
    }
}

/// Plan retrieval from a shortlist of the catalog, showing each document's
/// generated summary alongside its title and opening.
///
/// `summaries` is empty whenever the summary tier is off, which is the default,
/// and an empty map leaves the rows without a summary field. `turn` carries the
/// local search's candidate documents and what refusals taught about the
/// planner's window.
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

/// Rank the documents the planner is shown, once per turn: as many as the
/// planner's prompt on `llm` has rows for.
pub(in super::super) async fn shortlist_candidates(
    hybrid: &dyn HybridSearchTrait,
    llm: &dyn LLMPort,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    summaries: &HashMap<String, String>,
    document_ids: &HashSet<String>,
    turn: &mut PlannerTurn<'_>,
) {
    if turn.candidates.is_some() {
        return;
    }
    let rows =
        shortlist_rows(llm, question, history, catalog, summaries, turn).unwrap_or_else(|error| {
            warn!(%error, "Planner rows could not be sized; showing the catalog in its own order");
            0
        });
    turn.candidates =
        Some(shortlist::candidate_documents(hybrid, question, history, document_ids, rows).await);
}

/// How many catalog rows the planner's next prompt on `llm` holds for this
/// request: as many documents as the shortlist search ranks, since the
/// prompt shows no more. Sized as `plan_with` sizes the prompt, from the
/// window and the conversation it carries.
fn shortlist_rows(
    llm: &dyn LLMPort,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    summaries: &HashMap<String, String>,
    turn: &PlannerTurn<'_>,
) -> Result<usize> {
    let target = turn.health.prompt_tokens(llm);
    let history = prompt::recent_history(llm, history, target);
    let parts = prompt::PromptParts {
        question,
        history: &history,
        catalog_total: catalog.len(),
        correction: None,
        summaries,
    };
    prompt::rows_held(llm, &parts, catalog, target)
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
    // What the planner's window holds beside the plan: no clock sizes it.
    let target = turn.health.prompt_tokens(llm);
    // Built to the planner's history share from whole turns by the caller;
    // held to it here in whole lines, with the rest counted.
    let history = prompt::recent_history(llm, history, target);
    let ranked = shortlist::shortlist(catalog, turn.candidate_ids(), question);
    // The first pass's titles, best first, as many as the history's share of
    // the prompt holds, and the rest counted.
    let correction = correction.map(|correction| {
        Correction::of(&correction.fitted(llm, prompt::history_tokens(target)))
    });
    let parts = prompt::PromptParts {
        question,
        history: &history,
        catalog_total: catalog.len(),
        correction: correction.as_ref(),
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
    // No timeout: a slow planner is not a failed one. The user's Stop
    // cancels the call, and the adapter's stall detection ends one whose
    // server has gone quiet. Any error falls back to the local plan.
    let input = PlanInput {
        question: Told(question.to_owned()),
        history: Told(history),
        catalog_total: Told(catalog.len()),
        correction: correction.map(Told),
        documents: prompt.documents,
    };
    let call = CallOptions {
        priority: lattice_core::application::ports::llm_port::InferencePriority::Interactive,
        // A stop press takes the plan out of the backend's queue.
        cancel: Some(cancel.clone()),
        no_time_limit: true,
        ..Default::default()
    };
    let settled = ask::<PlanContract>(llm, &Plain, &mut Scope::for_question(), &input, &call).await;
    match settled {
        Ok(Settled::Answered(plan)) => plan_from(plan, catalog),
        Ok(Settled::Unanswered(_)) => Err(AppError::ModelAnswerUnusable(
            "The retrieval planner's answer could not be used".into(),
        )),
        Err(error) => {
            if is_window_overflow(&error) {
                turn.health.overflowed(llm.model_name(), prompt.tokens);
                tracing::warn!(
                    prompt_tokens = prompt.tokens,
                    exact_count = prompt.exact,
                    window = llm.max_context_tokens(),
                    "The server refused the planner prompt as too large for its window;                      the reported window is larger than the real one"
                );
            }
            Err(error)
        }
    }
}

/// The planner's statement as a plan over `catalog`. Its `start_at_beginning`
/// is the planner's call, kept as given: whether a request asks to read from
/// the beginning is a judgement about intent the model makes in any language.
fn plan_from(plan: Plan, catalog: &[CorpusDocument]) -> Result<CorpusSearchPlan> {
    CorpusSearchPlan {
        queries: plan.queries.into_iter().map(|query| query.0).collect(),
        opening_document_ids: plan
            .opening
            .into_iter()
            .map(|picked| picked.reference)
            .collect(),
        start_at_beginning: plan.start_at_beginning,
    }
    .validate(catalog)
}

#[cfg(test)]
mod tests {
    use super::super::answer::Query;
    use super::*;

    fn document(id: &str, name: &str) -> CorpusDocument {
        CorpusDocument {
            sections: Vec::new(),
            source_context: None,
            id: id.into(),
            name: name.into(),
            opening: String::new(),
            chapter_number: None,
        }
    }

    #[test]
    fn the_planners_reading_flag_survives_a_request_worded_in_any_language() {
        // "Wo fange ich am besten an?" matches no phrase list; the planner's
        // judgement that it asks to start reading is what the plan keeps.
        let catalog = vec![
            document("doc-zellen", "Leitfaden Zellbiologie"),
            document("doc-genetik", "Leitfaden Genetik"),
        ];
        let statement = Plan {
            queries: vec![Query("Einführung Zellbiologie".into())],
            opening: Vec::new(),
            start_at_beginning: true,
        };

        let plan = plan_from(statement, &catalog).expect("plan");

        assert!(plan.start_at_beginning);
        assert_eq!(plan.queries, ["Einführung Zellbiologie"]);
    }
}
