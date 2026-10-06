use super::ConversationRepository;
use crate::features::conversation::tangent_dto::{ConversationTangentDto, CreateTangentRequestDto};
use crate::shared::error::{AppError, Result};

const TANGENT_COLUMNS: &str = "id AS conversation_id, tangent_parent_id AS parent_conversation_id,
    forked_from_conversation_id AS source_conversation_id, forked_from_message_id AS source_message_id,
    tangent_selection AS selected_text, title, created_at, updated_at,
    tangent_context_message_count AS context_message_count";

impl ConversationRepository {
    pub async fn create_tangent(
        &self,
        request: CreateTangentRequestDto,
    ) -> Result<ConversationTangentDto> {
        let selected_text = request.selected_text.trim();
        if selected_text.is_empty() || selected_text.chars().count() > 8_000 {
            return Err(AppError::InvalidInput(
                "Select between 1 and 8,000 characters for a tangent.".into(),
            ));
        }
        let title: String = selected_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();
        let id = uuid::Uuid::new_v4().to_string();
        self.fork_with_tangent(
            &request.conversation_id,
            Some(&request.message_id),
            &id,
            &title,
            Some(selected_text),
        )
        .await?;
        sqlx::query_as::<_, ConversationTangentDto>(&format!(
            "SELECT {TANGENT_COLUMNS} FROM conversations WHERE id = ?"
        ))
        .bind(&id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read tangent: {e}")))
    }

    pub async fn list_tangents(&self, parent_id: &str) -> Result<Vec<ConversationTangentDto>> {
        sqlx::query_as::<_, ConversationTangentDto>(&format!(
            "SELECT {TANGENT_COLUMNS} FROM conversations WHERE tangent_parent_id = ? ORDER BY updated_at DESC, id"
        )).bind(parent_id).fetch_all(&self.pool).await
            .map_err(|e| AppError::Database(format!("Failed to list tangents: {e}")))
    }

    /// Promotion keeps the same id and transcript, including the fork context.
    /// Repeating it is harmless; an ordinary conversation remains ordinary.
    pub async fn promote_tangent(&self, id: &str) -> Result<()> {
        let result = sqlx::query("UPDATE conversations SET tangent_parent_id = NULL WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to promote tangent: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Tangent {id} not found")));
        }
        Ok(())
    }
}
