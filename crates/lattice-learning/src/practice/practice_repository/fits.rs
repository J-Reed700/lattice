//! Stored practice-fit verdicts: one per course, on the outline basis it was decided on.
use super::*;
use crate::practice::practice_fit::CourseOutline;

/// A verdict as stored.
pub(crate) struct StoredFit {
    pub basis: String,
    pub teaches_code: bool,
    pub reason: String,
}

/// What a fit answer rests on, read in one transaction.
pub(crate) struct FitState {
    pub language: Option<String>,
    pub outline: CourseOutline,
    pub stored: Option<StoredFit>,
}

impl LearningPracticeRepository {
    pub(crate) async fn fit_state(&self, _program_id: &str) -> Result<FitState> {
        Err(AppError::InvalidState("not built yet".into()))
    }

    pub(crate) async fn record_fit(
        &self,
        _program_id: &str,
        _basis: &str,
        _teaches_code: bool,
        _reason: &str,
        _model: &str,
        _now: i64,
    ) -> Result<()> {
        Err(AppError::InvalidState("not built yet".into()))
    }
}
