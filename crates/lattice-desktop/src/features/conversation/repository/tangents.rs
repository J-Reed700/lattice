use super::ConversationRepository;
use crate::features::conversation::tangent_dto::{ConversationTangentDto, CreateTangentRequestDto};
use lattice_core::shared::error::{AppError, Result};

const TANGENT_COLUMNS: &str = "id AS conversation_id, tangent_parent_id AS parent_conversation_id,
    forked_from_conversation_id AS source_conversation_id, forked_from_message_id AS source_message_id,
    tangent_selection AS selected_text, title, created_at, updated_at,
    tangent_context_message_count AS context_message_count";

#[derive(sqlx::FromRow)]
struct ConversationTangentRow {
    conversation_id: String,
    parent_conversation_id: String,
    source_conversation_id: String,
    source_message_id: String,
    selected_text: String,
    title: String,
    created_at: String,
    updated_at: String,
    context_message_count: i64,
}

impl From<ConversationTangentRow> for ConversationTangentDto {
    fn from(row: ConversationTangentRow) -> Self {
        Self {
            conversation_id: row.conversation_id,
            parent_conversation_id: row.parent_conversation_id,
            source_conversation_id: row.source_conversation_id,
            source_message_id: row.source_message_id,
            selected_text: row.selected_text,
            title: row.title,
            created_at: row.created_at,
            updated_at: row.updated_at,
            context_message_count: row.context_message_count,
        }
    }
}

impl ConversationRepository {
    pub async fn create_tangent(
        &self,
        request: CreateTangentRequestDto,
    ) -> Result<ConversationTangentDto> {
        let selected_text = request.selected_text.trim();
        if selected_text.is_empty() {
            return Err(AppError::InvalidInput(
                "Select some text for a tangent.".into(),
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
        let mut read = self.db.begin_read().await?;
        let tangent = sqlx::query_as::<_, ConversationTangentRow>(&format!(
            "SELECT {TANGENT_COLUMNS} FROM conversations WHERE id = ?"
        ))
        .bind(&id)
        .fetch_one(&mut *read)
        .await
        .map(Into::into)
        .map_err(|e| AppError::Database(format!("Failed to read tangent: {e}")))?;
        read.commit().await?;
        Ok(tangent)
    }

    pub async fn list_tangents(&self, parent_id: &str) -> Result<Vec<ConversationTangentDto>> {
        let mut read = self.db.begin_read().await?;
        let tangents = sqlx::query_as::<_, ConversationTangentRow>(&format!(
            "SELECT {TANGENT_COLUMNS} FROM conversations WHERE tangent_parent_id = ? ORDER BY updated_at DESC, id"
        )).bind(parent_id).fetch_all(&mut *read).await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(|e| AppError::Database(format!("Failed to list tangents: {e}")))?;
        read.commit().await?;
        Ok(tangents)
    }

    /// Promotion keeps the same id and transcript, including the fork context.
    /// Repeating it is harmless; an ordinary conversation remains ordinary.
    pub async fn promote_tangent(&self, id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        let result = sqlx::query("UPDATE conversations SET tangent_parent_id = NULL WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to promote tangent: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Tangent {id} not found")));
        }
        tx.commit().await?;
        Ok(())
    }
}
