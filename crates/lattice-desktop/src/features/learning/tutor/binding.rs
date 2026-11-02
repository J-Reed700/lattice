//! Which course a conversation teaches from: `conversations.program_id` and
//! `conversations.lesson_id`.

use crate::features::learning::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

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
    pool: &SqlitePool,
    request: &SetConversationCourseRequestDto,
) -> Result<()> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(pool)
            .await
            .map_err(db)?;
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
        let owned: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(lesson_id)
                .bind(&request.program_id)
                .fetch_optional(pool)
                .await
                .map_err(db)?;
        if owned.is_none() {
            return Err(AppError::NotFound(
                "Lesson does not belong to this learning program.".into(),
            ));
        }
    }
    let bound = sqlx::query(
        "UPDATE conversations SET program_id=?, lesson_id=? WHERE id=? AND explorer_root IS NULL",
    )
    .bind(&request.program_id)
    .bind(&request.lesson_id)
    .bind(&request.conversation_id)
    .execute(pool)
    .await
    .map_err(db)?;
    if bound.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "No conversation to bind to a course: {}",
            request.conversation_id
        )));
    }
    Ok(())
}

/// The course `conversation_id` is bound to; `None` for every other chat.
pub(super) async fn conversation_course(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<CourseBinding>> {
    let row = sqlx::query("SELECT program_id, lesson_id FROM conversations WHERE id=?")
        .bind(conversation_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?;
    Ok(row.and_then(|row| {
        let program_id: Option<String> = row.get("program_id");
        program_id.map(|program_id| CourseBinding {
            program_id,
            lesson_id: row.get("lesson_id"),
        })
    }))
}
