//! The user's rewrite of one section of a published lesson.

use lattice_core::shared::error::{AppError, Result};

use super::ClaimReviewService;

/// The text a section changed since the user opened it.
pub const SECTION_CHANGED: &str =
    "This section changed since you opened it. Copy your changes, close the editor and open it again.";

/// A section's new title and text, with the ones the user opened.
pub struct SectionEdit {
    pub program_id: String,
    pub lesson_id: String,
    pub section_index: usize,
    pub before_title: String,
    pub before_body: String,
    pub title: String,
    pub body: String,
    pub dry_run: bool,
}

/// What a rewrite does, or did, to the statements on the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEditOutcome {
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
    /// Writes the section's new title and text, ends the statements that stood
    /// on what changed, and keeps the rest. A dry run writes nothing.
    pub async fn edit_section(&self, _edit: SectionEdit) -> Result<SectionEditOutcome> {
        Err(AppError::InternalError("not built yet".into()))
    }
}
