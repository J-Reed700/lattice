//! Minimal compiling stub types for problem patterns.
//! Owned by cs-debrief; replaced at merge time.

use serde::{Deserialize, Serialize};

use super::hints::CodeHintRung;
use crate::recall::study_dto::StudyRating;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternOutcomeInput {
    pub all_cases_passed: bool,
    pub highest_hint_rung: Option<CodeHintRung>,
    pub solution_seen_before_pass: bool,
}

pub fn rate_problem_pattern(_input: &PatternOutcomeInput) -> StudyRating {
    StudyRating::Again
}
