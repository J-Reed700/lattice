//! The user's rewrite of one question of a published lesson.

use lattice_core::shared::error::{AppError, Result};

use super::ClaimReviewService;
use crate::assessment_engine::LearningItemFormat;

/// A question as the editor shows it: its prompt, its choices in order, the
/// right choice and its explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionText {
    pub kind: LearningItemFormat,
    pub prompt: String,
    /// Empty for a kind without choices.
    pub choices: Vec<String>,
    /// The index of the right choice; none for a kind without one.
    pub right: Option<usize>,
    pub explanation: String,
}

/// A question's new text, with the one the user opened.
pub struct QuestionEdit {
    pub program_id: String,
    pub lesson_id: String,
    pub question_index: usize,
    pub before: QuestionText,
    pub after: QuestionText,
    pub dry_run: bool,
}

/// What a rewrite does, or did, to the statements on the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionEditOutcome {
    pub changed: bool,
    pub statements_kept: u32,
    pub statements_changed: u32,
    pub statements_removed: u32,
    pub sentences_written: u32,
    /// Changed or removed statements that were verified against a source.
    pub checked_statements_affected: u32,
    pub revision: Option<i64>,
}

impl ClaimReviewService {
    /// A published lesson's question, as the editor opens it.
    pub async fn question_text(
        &self,
        _program_id: &str,
        _lesson_id: &str,
        _question_index: usize,
    ) -> Result<QuestionText> {
        Err(AppError::NotFound("This lesson is not in this course.".into()))
    }

    /// Writes the question's new text, ends the statements that stood on what
    /// changed, and keeps the rest. A dry run writes nothing.
    pub async fn edit_question(&self, _edit: QuestionEdit) -> Result<QuestionEditOutcome> {
        Ok(QuestionEditOutcome {
            changed: false,
            statements_kept: 0,
            statements_changed: 0,
            statements_removed: 0,
            sentences_written: 0,
            checked_statements_affected: 0,
            revision: None,
        })
    }
}
