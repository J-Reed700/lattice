//! Judges a review against a drill's key. One question decides what came of every issue of the key, and which findings
//! are about it; a second decides what came of the findings no issue claims. An issue or finding left unanswered is
//! the call's error, never a guess.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::SamplingOverride;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{ask_each, Settled};
use lattice_core::application::services::model_contract::shown::{Scope, Told};
use lattice_core::domain::learning::drills::{Assessment, IssueOutcome};
use lattice_core::shared::error::{AppError, Result};

use super::grader::{Review, ReviewGrader};
use super::material::JudgeWording;
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use crate::model_call;

mod contract;
mod derive;
mod task;

use contract::{ExtrasInput, FindingView, GradeExtras, GradeReview, Input};

#[cfg(test)]
#[path = "grading_tests.rs"]
mod tests;

/// Why an issue is missed when the review made no findings.
const NO_FINDINGS: &str = "The review made no findings.";

/// What does not fit, when the window cannot hold the drill and the review.
const DOES_NOT_FIT: &str = "The drill and the review do not fit";

/// Grades a review, whoever made the review.
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
        let mut call = model_call::answer_options();
        call.sampling = Some(SamplingOverride::deterministic());
        let issues = task::issue_views(review, &wording);
        let findings = task::finding_views(review, &wording);

        let context = task::context(review, &wording, wording.system);
        let noun = Told(wording.lines.to_owned());
        let asking = Asking {
            progress: None,
            reasoning: Some(ReasonFirst {
                kind: CallKind::Classifier,
                subject: "the grade of the review",
                brief: wording.brief,
            }),
        };
        let settled = ask_each::<GradeReview>(
            self.llm,
            &asking,
            &mut Scope::for_question(),
            &issues,
            |open, _| Input {
                context: context.clone(),
                noun: noun.clone(),
                issues: open.iter().map(|issue| (*issue).clone()).collect(),
                findings: findings.clone(),
            },
            &call,
        )
        .await
        .map_err(|error| self.too_large(error))?;
        let mut judged = Vec::with_capacity(issues.len());
        for (issue, settled) in issues.iter().zip(settled) {
            match settled {
                Settled::Answered(judgement) => judged.push((issue.at, judgement)),
                Settled::Unanswered(_) => return Err(derive::unusable("an issue is unanswered")),
            }
        }
        let credits = derive::credits(review.drill.key.len(), findings.len(), &judged)?;

        let unclaimed = credits.unclaimed();
        if unclaimed.is_empty() {
            return credits.assessment(&[]);
        }
        let context = task::context(review, &wording, wording.extras);
        let asking = Asking {
            progress: None,
            reasoning: Some(ReasonFirst {
                kind: CallKind::Classifier,
                subject: "the grade of the findings no issue claims",
                brief: wording.extras_brief,
            }),
        };
        let asked: Vec<FindingView> = findings
            .iter()
            .filter(|finding| unclaimed.contains(&finding.at))
            .cloned()
            .collect();
        let settled = ask_each::<GradeExtras>(
            self.llm,
            &asking,
            &mut Scope::for_question(),
            &asked,
            |open, _| ExtrasInput {
                context: context.clone(),
                key: issues.clone(),
                findings: open.iter().map(|finding| (*finding).clone()).collect(),
            },
            &call,
        )
        .await
        .map_err(|error| self.too_large(error))?;
        let mut verdicts = Vec::with_capacity(asked.len());
        for (finding, settled) in asked.iter().zip(settled) {
            match settled {
                Settled::Answered(verdict) => verdicts.push((finding.at, verdict)),
                Settled::Unanswered(_) => return Err(derive::unusable("a finding is unanswered")),
            }
        }
        credits.assessment(&verdicts)
    }
}

impl JudgeGrader<'_> {
    /// `error`, as the learner reads it when the window cannot hold the drill and the review.
    fn too_large(&self, error: AppError) -> AppError {
        match error {
            AppError::PromptExceedsWindow { .. } => {
                model_call::window_too_small(self.llm, DOES_NOT_FIT)
            }
            error => error,
        }
    }
}
