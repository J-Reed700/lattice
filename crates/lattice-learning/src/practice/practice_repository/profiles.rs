//! Preparing for: course-level practice profiles.
use super::*;

impl LearningPracticeRepository {
    /// Reads a course's practice profile, returning the default empty profile if none exists yet.
    pub async fn get_profile(&self, program_id: &str) -> Result<LearningPracticeProfileDto> {
        uuid(program_id, "program")?;
        // Stage 1 stub: compiles without panicking, fails assertions in tests.
        Ok(LearningPracticeProfileDto {
            program_id: program_id.to_string(),
            details: "stub_details".to_string(),
            language: Some("stub_language".to_string()),
            interview_at: Some(0),
            company: Some("stub_company".to_string()),
            role: Some("stub_role".to_string()),
            revision: -1,
        })
    }

    /// Saves a course's practice profile, validating optimistic locking via expected_revision.
    pub async fn save_profile(
        &self,
        request: &SaveLearningPracticeProfileRequestDto,
    ) -> Result<LearningPracticeProfileDto> {
        uuid(&request.program_id, "program")?;
        // Stage 1 stub: compiles without panicking, fails assertions in tests.
        Ok(LearningPracticeProfileDto {
            program_id: request.program_id.clone(),
            details: String::new(),
            language: None,
            interview_at: None,
            company: None,
            role: None,
            revision: 0,
        })
    }
}
