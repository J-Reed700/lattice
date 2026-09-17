//! Conversation rows: create, read, list, update and delete.

use super::ConversationRepository;
use crate::domain::conversation::{CompactionRecord, Conversation, ConversationAggregate};
use crate::features::conversation::persistence_mapper::{
    ConversationModel, ConversationRowMapper, ConversationSummaryMapper, ConversationSummaryModel,
};
use crate::shared::domain_types::ConversationId;
use crate::shared::error::{AppError, Result};
use chrono::Utc;

impl ConversationRepository {
    /// Alias for create_conversation with no system prompt
    ///
    /// # Arguments
    /// * `title` - Conversation title
    /// * `model_name` - LLM model identifier
    /// * `system_prompt` - Optional system prompt
    ///
    /// # Returns
    /// Newly created conversation entity
    pub async fn create(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        self.create_conversation(title, model_name, system_prompt)
            .await
    }

    /// List all conversations with pagination
    ///
    /// # Arguments
    /// * `limit` - Maximum number of conversations to return
    /// * `offset` - Number of conversations to skip
    ///
    /// # Returns
    /// Vector of conversation entities ordered by updated_at DESC
    pub async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>> {
        self.find_all(limit, offset).await
    }

    /// Update conversation metadata
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    /// * `title` - Optional new title
    /// * `system_prompt` - Optional new system prompt (Some(None) to clear)
    /// * `message_count` - Optional new message count
    /// * `total_tokens` - Optional new total tokens
    ///
    /// # Returns
    /// Updated conversation or error if not found
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

    /// Load full conversation aggregate
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    ///
    /// # Returns
    /// Full aggregate with messages and document references
    pub async fn load_aggregate(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.find_aggregate_by_id(id).await
    }
    pub async fn create_conversation(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        let id = ConversationId::new();
        let now = Utc::now().to_rfc3339();

        // Store value to avoid E0716 temporary value dropped error
        let id_str = id.to_string();

        sqlx::query!(
            r#"
            INSERT INTO conversations (id, title, model_name, system_prompt, created_at, updated_at, message_count, total_tokens)
            VALUES (?, ?, ?, ?, ?, ?, 0, 0)
            "#,
            id_str,
            title,
            model_name,
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
            model_name: model_name.to_string(),
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
            SELECT id, title, model_name, system_prompt, created_at, updated_at,
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
    /// can exist per conversation (see [`Self::upsert_summary`]).
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
            SELECT id, title, model_name, system_prompt, created_at, updated_at,
                   message_count, total_tokens
            FROM conversations
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

        // Persist the active compaction summary, if any (upsert).
        if let Some(record) = aggregate.compaction() {
            self.upsert_summary(record).await?;
        }

        Ok(())
    }

    /// Insert or update the compaction summary for a conversation.
    ///
    /// One row per conversation: `conversation_summaries.conversation_id` is
    /// UNIQUE, and conflicting on that column updates the existing row in
    /// place, so its primary key stays stable across re-compactions. Reading a
    /// summary back can therefore never have to choose between rows. Doing this
    /// in one statement also keeps two concurrent compactions of the same
    /// conversation from both inserting.
    pub async fn upsert_summary(&self, record: &CompactionRecord) -> Result<()> {
        let conversation_id = record.conversation_id.to_string();
        let created_at = record.created_at.to_rfc3339();

        let result = sqlx::query(
            r#"
            INSERT INTO conversation_summaries (
                id, conversation_id, summary_text, up_to_message_id,
                original_message_count, original_tokens, summary_tokens,
                compression_ratio, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (conversation_id) DO UPDATE SET
                summary_text = excluded.summary_text,
                up_to_message_id = excluded.up_to_message_id,
                original_message_count = excluded.original_message_count,
                original_tokens = excluded.original_tokens,
                summary_tokens = excluded.summary_tokens,
                compression_ratio = excluded.compression_ratio,
                created_at = excluded.created_at
            "#,
        )
        .bind(&record.id)
        .bind(&conversation_id)
        .bind(&record.summary_text)
        .bind(&record.up_to_message_id)
        .bind(record.original_message_count)
        .bind(record.original_tokens)
        .bind(record.summary_tokens)
        .bind(record.compression_ratio)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save conversation summary: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::Database(
                "Conversation summary upsert affected no rows".into(),
            ));
        }

        Ok(())
    }

    pub async fn count(&self) -> Result<usize> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
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
