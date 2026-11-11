//! The questions of a lesson or other material, reviewed as the quiz items
//! they are.
use crate::review_findings::ReviewFinding;
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde_json::Value;

/// What the questions are for.
pub(crate) struct Outcome<'a> {
    pub title: &'a str,
    pub description: &'a str,
}

/// One finding per question that review faults, placed at that question.
pub(crate) async fn check(
    llm: &dyn LLMPort,
    candidate: &Value,
    outcome: Outcome<'_>,
) -> Result<Vec<ReviewFinding>> {
    let _ = (outcome.title, outcome.description);
    crate::key_review::check(llm, candidate).await
}
