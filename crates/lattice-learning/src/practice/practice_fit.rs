//! Whether a course teaches writing, reading or debugging program code, which decides if Practice offers coding.
use crate::dto::PracticeFitDto;
use crate::practice_repository::LearningPracticeRepository;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use serde::Serialize;

/// The course as the check reads it.
#[derive(Debug, Clone, Serialize)]
pub struct CourseOutline {
    pub title: String,
    pub modules: Vec<OutlineModule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineModule {
    pub title: String,
    pub lessons: Vec<OutlineLesson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineLesson {
    pub title: String,
    pub objective: String,
}

/// What a verdict rests on: the check's text and the outline.
pub fn basis_of(_outline: &CourseOutline) -> Result<String> {
    Err(AppError::InvalidState("not built yet".into()))
}

/// The fit now known, without asking the model.
pub async fn practice_fit(
    _repo: &LearningPracticeRepository,
    _program_id: &str,
) -> Result<PracticeFitDto> {
    Err(AppError::InvalidState("not built yet".into()))
}

/// The fit, asking the model when there is no verdict on the current basis.
pub async fn check_practice_fit(
    _repo: &LearningPracticeRepository,
    _llm: &dyn LLMPort,
    _program_id: &str,
) -> Result<PracticeFitDto> {
    Err(AppError::InvalidState("not built yet".into()))
}
