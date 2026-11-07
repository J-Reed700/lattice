//! Judges a review against a drill's key. One model call decides, for every issue of the key and every finding of the
//! review, what came of it. The answer is checked in code before anyone sees it, and one the code cannot accept is
//! the call's error, never a guess.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::SamplingOverride;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::domain::learning::drills::{Assessment, IssueOutcome};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

use super::grader::{Review, ReviewGrader};
use super::material::JudgeWording;
use crate::lessons::structured_call::{
    complete_checked_request, CallKind, ReasonFirst,
};
use crate::model_call;

mod accept;
mod task;

#[cfg(test)]
#[path = "grading_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "live_grading_eval.rs"]
pub(in crate::drills) mod live_grading_eval;

/// Why an issue is missed when the review made no findings.
const NO_FINDINGS: &str = "The review made no findings.";

/// What does not fit, when the window cannot hold the drill and the review.
const DOES_NOT_FIT: &str = "The drill and the review do not fit";

/// Grades a review with one model call, whoever made the review.
pub struct JudgeGrader<'a> {
    llm: &'a dyn LLMPort,
}

impl<'a> JudgeGrader<'a> {
    pub fn new(llm: &'a dyn LLMPort) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl ReviewGrader for JudgeGrader<'_> {
    async fn assess(&self, review: &Review<'_>) -> Result<Assessment> {
        if review.findings.is_empty() {
            return Ok(Assessment {
                issues: vec![
                    (IssueOutcome::Missed, NO_FINDINGS.to_owned());
                    review.drill.key.len()
                ],
                findings: Vec::new(),
            });
        }
        let wording = JudgeWording::of(&review.drill.material)?;
        let request = request(review, &wording);
        if !model_call::fits(self.llm, &request).await {
            return Err(model_call::window_too_small(self.llm, DOES_NOT_FIT));
        }
        let reasoning = ReasonFirst {
            kind: CallKind::Classifier,
            subject: "the grade of the review",
            brief: wording.brief,
        };
        complete_checked_request(self.llm, request, None, Some(reasoning), |raw| {
            accept::accept(raw, review)
        })
        .await
        .map_err(|error| match error {
            AppError::PromptExceedsWindow { .. } => {
                model_call::window_too_small(self.llm, DOES_NOT_FIT)
            }
            error => error,
        })
    }
}

/// The whole request for `review`, answered deterministically and with the planner's reservation for the answer.
fn request(review: &Review<'_>, wording: &JudgeWording) -> GroundedRequest {
    let mut request = model_call::structured(
        wording.system,
        task::task(review, wording),
        schema(),
        0,
        "low",
    );
    request.call.sampling = Some(SamplingOverride::deterministic());
    request
}

/// An entry for every issue and every finding, each with its reason before its outcome.
fn schema() -> Value {
    fn index() -> Value {
        json!({"type": "integer", "minimum": 1})
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["issues", "findings"],
        "properties": {
            "issues": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["issue", "reason", "outcome", "finding", "quote"],
                    "properties": {
                        "issue": index(),
                        "reason": {"type": "string"},
                        "outcome": {"enum": ["found", "half_found", "missed"]},
                        "finding": {"type": ["integer", "null"], "minimum": 1},
                        "quote": {"type": ["string", "null"]}
                    }
                }
            },
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["finding", "reason", "outcome", "issues"],
                    "properties": {
                        "finding": index(),
                        "reason": {"type": "string"},
                        "outcome": {"enum": ["matches", "valid_extra", "false_positive"]},
                        "issues": {"type": "array", "items": index()}
                    }
                }
            }
        }
    })
}
