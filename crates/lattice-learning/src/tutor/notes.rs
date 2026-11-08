//! The learner's summary, saved to the course Notebook: onto the note of the
//! lesson, made if the lesson has none yet.

use super::course_turn::CourseTurn;
use crate::repository::{lesson_note_draft, LearningRepository};
use lattice_core::shared::error::{AppError, Result};
use lattice_store::workspace_notes::DailyNotesRepository;

/// `note` with the learner's summary added after what it holds.
fn with_summary(note: &str, summary: &str) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d");
    format!(
        "{}\n\n## In your own words, {date}\n\n{}\n",
        note.trim_end(),
        summary.trim()
    )
}

impl CourseTurn {
    pub(super) async fn save_note(&self, lesson_id: Option<&str>, summary: &str) -> Result<String> {
        let lesson_id = match lesson_id {
            Some(id) => id.to_string(),
            None => self.working_lesson().await?.ok_or_else(|| {
                AppError::InvalidInput("This course has no lesson to save a note on.".into())
            })?,
        };
        let program = self.program().await?;
        let (lesson_title, title, content) = program
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .find(|lesson| lesson.id == lesson_id)
            .map(|lesson| {
                let (title, content) = lesson_note_draft(program, lesson);
                (lesson.title.clone(), title, content)
            })
            .ok_or_else(|| {
                AppError::NotFound("Lesson does not belong to this learning program.".into())
            })?;
        let (note_id, _) = LearningRepository::new(self.pool.clone())
            .ensure_lesson_note(&self.program_id, &lesson_id, &title, &content)
            .await?;
        let notes = DailyNotesRepository::new(self.pool.clone());
        let mut note = notes.get(&note_id).await?;
        note.content = with_summary(&note.content, summary);
        note.updated_at = chrono::Utc::now().to_rfc3339();
        notes.update(&note).await?;
        (self.note_written)();
        Ok(format!("Saved to the Notebook, on {lesson_title}."))
    }
}
