//! Short performance tasks with private keys and evidence-bound placement advice.
mod evaluation;
mod tasks;
#[cfg(test)]
mod tests;

use crate::plan_dto::*;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::same_text;
use serde::{Deserialize, Serialize};

pub use evaluation::evaluate;
pub use tasks::author;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticTask {
    pub outcome_id: String,
    pub prompt: String,
    pub expected_answer: String,
    pub criteria: String,
}

/// What the learner is told when feedback could not be tied to their responses.
const UNTIED: &str = "Starting-point feedback could not be tied to your actual answers. Your answers have not been submitted.";

/// Whether `quote` is in `response`, apart from spacing, typography and
/// letter case.
fn quote_stands(response: &str, quote: &str) -> bool {
    same_text::find(response, quote).is_some()
}

pub fn validate_findings(
    attempt: &LearningDiagnosticAttemptDto,
    responses: &[LearningDiagnosticResponseDto],
    findings: &[LearningDiagnosticFindingDto],
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    if findings.len() != attempt.prompts.len()
        || findings.iter().any(|f| {
            let response = responses.iter().find(|r| r.prompt_id == f.prompt_id);
            !seen.insert(&f.prompt_id)
                || !attempt
                    .prompts
                    .iter()
                    .any(|p| p.id == f.prompt_id && p.outcome_id == f.outcome_id)
                || f.feedback.trim().is_empty()
                || match &f.evidence_quote {
                    Some(q) => {
                        q.trim().is_empty()
                            || !response.is_some_and(|r| quote_stands(&r.response, q))
                    }
                    None => f.signal != LearningDiagnosticSignal::Uncertain,
                }
        })
    {
        return Err(AppError::InvalidInput(UNTIED.into()));
    }
    Ok(())
}
