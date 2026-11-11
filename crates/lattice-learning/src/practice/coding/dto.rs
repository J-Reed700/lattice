//! What the build of a coding problem takes and answers.

use super::problem::ProblemDifficulty;
use lattice_core::domain::learning::practice::work::Form;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartPracticeBuildRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub form: Form,
    pub language: String,
    pub difficulty: ProblemDifficulty,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartPracticeBuildResultDto {
    pub job_id: String,
}
