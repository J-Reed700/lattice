//! Preparing for: course-level practice profiles.
use super::*;
use sqlx::SqliteConnection;

fn not_built() -> AppError {
    AppError::InvalidState("not built yet".into())
}

impl LearningPracticeRepository {
    /// A course's practice profile; the empty profile at revision 0 when none was saved.
    pub async fn get_profile(&self, _program_id: &str) -> Result<LearningPracticeProfileDto> {
        Err(not_built())
    }

    /// Saves a course's practice profile when `expected_revision` is its current revision.
    pub async fn save_profile(
        &self,
        _request: &SaveLearningPracticeProfileRequestDto,
    ) -> Result<LearningPracticeProfileDto> {
        Err(not_built())
    }
}

/// The saved profile of a course, if any.
pub(crate) async fn profile_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
) -> Result<Option<LearningPracticeProfileDto>> {
    Err(not_built())
}
