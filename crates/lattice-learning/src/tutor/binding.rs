//! Which course a conversation teaches from: `conversations.program_id` and
//! `conversations.lesson_id`.

use super::repository;
use crate::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Transactions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetConversationCourseRequestDto {
    pub conversation_id: String,
    pub program_id: String,
    /// The lesson the chat was opened from, when it was.
    pub lesson_id: Option<String>,
}

/// The course a conversation is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CourseBinding {
    pub program_id: String,
    pub lesson_id: Option<String>,
}

/// Binds a conversation to an accepted course and, optionally, one of its
/// lessons. A conversation reads one source, so a folder's thread is refused.
pub async fn bind_conversation(
    database: &Database,
    request: &SetConversationCourseRequestDto,
) -> Result<()> {
    let mut tx = database.begin_write().await.map_err(db)?;
    let status = repository::program_status(&mut tx, &request.program_id).await?;
    match status.as_deref() {
        Some("active") => {}
        Some(_) => {
            return Err(AppError::InvalidInput(
                "Accept the course before opening its tutor.".into(),
            ))
        }
        None => return Err(AppError::NotFound("Learning program not found.".into())),
    }
    if let Some(lesson_id) = &request.lesson_id {
        if !repository::lesson_in_program(&mut tx, lesson_id, &request.program_id).await? {
            return Err(AppError::NotFound(
                "Lesson does not belong to this learning program.".into(),
            ));
        }
    }
    let bound = repository::bind_course(
        &mut tx,
        &request.conversation_id,
        &request.program_id,
        request.lesson_id.as_deref(),
    )
    .await?;
    if !bound {
        return Err(AppError::NotFound(format!(
            "No conversation to bind to a course: {}",
            request.conversation_id
        )));
    }
    tx.commit().await.map_err(db)
}
