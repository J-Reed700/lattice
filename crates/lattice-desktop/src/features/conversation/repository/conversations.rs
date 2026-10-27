//! Conversation rows: create, read, list, update and delete.

use super::ConversationRepository;
use crate::features::conversation::persistence_mapper::{
    ConversationModel, ConversationRowMapper, ConversationSummaryMapper, ConversationSummaryModel,
    ModelChoiceColumn,
};
use chrono::Utc;
use lattice_core::domain::conversation::{
    ChatModelChoice, CompactionRecord, Conversation, ConversationAggregate,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::ConversationId;

impl ConversationRepository {
    /// Alias for `create_conversation`; the conversation follows the Settings
    /// default model.
    pub async fn create(&self, title: &str, system_prompt: Option<&str>) -> Result<Conversation> {
        self.create_conversation(title, system_prompt).await
    }

    /// Conversations, most recently updated first.
    pub async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>> {
        self.find_all(limit, offset).await
    }

    /// Updates conversation metadata; `system_prompt` of `Some(None)` clears it.
    pub async fn update(
        &self,
        id: &str,
        title: Option<&str>,
        system_prompt: Option<Option<&str>>,
        _message_count: Option<i64>,
        _total_tokens: Option<i64>,
    ) -> Result<()> {
        if let Some(new_title) = title {
            self.update_title(id, new_title).await?;
        }

        if let Some(new_system_prompt) = system_prompt {
            self.update_system_prompt(id, new_system_prompt).await?;
        }

        Ok(())
    }

    /// The full aggregate, with messages and document references.
    pub async fn load_aggregate(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.find_aggregate_by_id(id).await
    }
    pub async fn create_conversation(
        &self,
        title: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        let id = ConversationId::new();
        let now = Utc::now().to_rfc3339();

        // Store value to avoid E0716 temporary value dropped error
        let id_str = id.to_string();

        sqlx::query!(
            r#"
            INSERT INTO conversations (id, title, system_prompt, created_at, updated_at, message_count, total_tokens)
            VALUES (?, ?, ?, ?, ?, 0, 0)
            "#,
            id_str,
            title,
            system_prompt,
            now,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create conversation: {}", e)))?;

        Ok(Conversation {
            id,
            title: title.to_string(),
            model_choice: ChatModelChoice::Default,
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 0,
            total_tokens: 0,
        })
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>> {
        let db_model = sqlx::query_as::<_, ConversationModel>(
            r#"
            SELECT id, title, model_choice, system_prompt, created_at, updated_at,
                   message_count, total_tokens
            FROM conversations
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find conversation: {}", e)))?;

        match db_model {
            Some(model) => Ok(Some(ConversationRowMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    pub async fn find_aggregate_by_id(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        let conversation = match self.find_by_id(id).await? {
            Some(conv) => conv,
            None => return Ok(None),
        };

        let messages = self.get_messages(id).await?;

        let document_refs = self.get_document_references(id).await?;

        let compaction = self.get_summary(id).await?;

        // Reconstruct aggregate
        let aggregate = ConversationAggregate::from_persistence(
            conversation,
            messages,
            document_refs,
            compaction,
        );

        Ok(Some(aggregate))
    }

    /// Load the compaction summary for a conversation, if any. At most one row
    /// can exist per conversation: `conversation_summaries.conversation_id` is
    /// UNIQUE and the memory commit conflicts on it, so a re-compaction updates
    /// the row in place and a read never has to choose between two.
    pub async fn get_summary(&self, id: &str) -> Result<Option<CompactionRecord>> {
        let model = sqlx::query_as::<_, ConversationSummaryModel>(
            r#"
            SELECT id, conversation_id, summary_text, up_to_message_id,
                   original_message_count, original_tokens, summary_tokens,
                   compression_ratio, created_at
            FROM conversation_summaries
            WHERE conversation_id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load conversation summary: {}", e)))?;

        match model {
            Some(m) => Ok(Some(ConversationSummaryMapper::to_entity(&m)?)),
            None => Ok(None),
        }
    }

    pub async fn find_all(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Conversation>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let db_models = sqlx::query_as::<_, ConversationModel>(
            r#"
            SELECT id, title, model_choice, system_prompt, created_at, updated_at,
                   message_count, total_tokens
            FROM conversations
            WHERE tangent_parent_id IS NULL
            ORDER BY updated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list conversations: {}", e)))?;

        Ok(ConversationRowMapper::to_entities(&db_models))
    }

    pub async fn update_title(&self, id: &str, new_title: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET title = ?, updated_at = ?
            WHERE id = ?
            "#,
            new_title,
            now,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation title: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        Ok(())
    }

    pub async fn update_system_prompt(&self, id: &str, system_prompt: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET system_prompt = ?, updated_at = ?
            WHERE id = ?
            "#,
            system_prompt,
            now,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to update conversation system prompt: {}",
                e
            ))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        Ok(())
    }

    /// The model this chat asked for.
    pub async fn model_choice(&self, conversation_id: &str) -> Result<ChatModelChoice> {
        let column: Option<String> =
            sqlx::query_scalar("SELECT model_choice FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to read model choice: {}", e)))?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Conversation not found: {}", conversation_id))
                })?;
        ChatModelChoice::from_column(conversation_id, column.as_deref())
    }

    /// Records the model this chat asks for. Settings and every other chat are
    /// untouched; a turn already running keeps the model it pinned.
    pub async fn set_model_choice(
        &self,
        conversation_id: &str,
        choice: &ChatModelChoice,
    ) -> Result<()> {
        let column = ModelChoiceColumn::to_column(choice)?;
        let result =
            sqlx::query("UPDATE conversations SET model_choice = ?, updated_at = ? WHERE id = ?")
                .bind(column)
                .bind(Utc::now().to_rfc3339())
                .bind(conversation_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to set model choice: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let result = sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete conversation: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }
    pub async fn save_aggregate(&self, aggregate: &ConversationAggregate) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conversation = aggregate.conversation();

        // Store value to avoid E0716 temporary value dropped error
        let id_str = conversation.id.to_string();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET title = ?,
                system_prompt = ?,
                message_count = ?,
                total_tokens = ?,
                updated_at = ?
            WHERE id = ?
            "#,
            conversation.title,
            conversation.system_prompt,
            conversation.message_count,
            conversation.total_tokens,
            now,
            id_str
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save aggregate: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation.id
            )));
        }

        Ok(())
    }

    pub async fn count(&self) -> Result<usize> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversations WHERE tangent_parent_id IS NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map(|c| c as usize)
        .map_err(|e| AppError::Database(format!("Failed to count conversations: {}", e)))
    }

    pub async fn exists(&self, id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to check conversation existence: {}", e))
            })
    }
}
