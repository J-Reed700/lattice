use super::*;
use crate::features::conversation::repository::space_rows::ConversationJournalRow;
use lattice_core::shared::persistence::Transactions;
use sqlx::SqliteConnection;

pub(super) async fn ensure_journal_space(
    connection: &mut SqliteConnection,
    journal_space_id: &str,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM journals WHERE id = ?")
        .bind(journal_space_id)
        .fetch_one(connection)
        .await
        .map_err(|e| AppError::Database(format!("Failed to verify journal: {}", e)))?;

    if exists == 0 {
        return Err(AppError::NotFound(format!(
            "Journal not found: {}",
            journal_space_id
        )));
    }

    Ok(())
}

impl ConversationRepository {
    pub async fn create_journal(
        &self,
        request: CreateConversationJournalRequestDto,
    ) -> Result<ConversationJournalDto, AppError> {
        let CreateConversationJournalRequestDto {
            name,
            description,
            icon,
            accent_color,
            space_prompt,
            tool_preferences_json,
        } = request;

        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput(
                "Journal name cannot be empty".to_string(),
            ));
        }

        let id = format!("journal_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();

        let mut tx = self
            .pool
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        sqlx::query(
            r#"
        INSERT INTO journals (
            id, name, description, icon, accent_color, space_prompt,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
        "#,
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(icon)
        .bind(accent_color)
        .bind(space_prompt)
        .bind(tool_preferences_json)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create journal: {}", e)))?;

        let created = sqlx::query_as::<_, ConversationJournalRow>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch created journal: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(created.into())
    }

    pub async fn list_journals(&self) -> Result<Vec<ConversationJournalDto>, AppError> {
        let journals = sqlx::query_as::<_, ConversationJournalRow>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        ORDER BY is_archived ASC, sort_order ASC, updated_at DESC
        "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list journals: {}", e)))?;

        Ok(journals.into_iter().map(Into::into).collect())
    }

    pub async fn update_journal(
        &self,
        request: UpdateConversationJournalRequestDto,
    ) -> Result<ConversationJournalDto, AppError> {
        let now = Utc::now().to_rfc3339();
        let is_archived: Option<i64> = request.is_archived.map(|v| if v { 1 } else { 0 });

        let mut tx = self
            .pool
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        let result = sqlx::query(
            r#"
        UPDATE journals
        SET
            name = COALESCE(?, name),
            description = COALESCE(?, description),
            icon = COALESCE(?, icon),
            accent_color = COALESCE(?, accent_color),
            space_prompt = COALESCE(?, space_prompt),
            tool_preferences_json = COALESCE(?, tool_preferences_json),
            is_archived = COALESCE(?, is_archived),
            sort_order = COALESCE(?, sort_order),
            updated_at = ?
        WHERE id = ?
        "#,
        )
        .bind(request.name)
        .bind(request.description)
        .bind(request.icon)
        .bind(request.accent_color)
        .bind(request.space_prompt)
        .bind(request.tool_preferences_json)
        .bind(is_archived)
        .bind(request.sort_order)
        .bind(&now)
        .bind(&request.journal_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update journal: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Journal not found: {}",
                request.journal_id
            )));
        }

        let updated = sqlx::query_as::<_, ConversationJournalRow>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
        )
        .bind(&request.journal_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch updated journal: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(updated.into())
    }

    pub async fn archive_journal(
        &self,
        request: ArchiveConversationJournalRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let now = Utc::now().to_rfc3339();
        let archived = if request.archived { 1 } else { 0 };

        let result = sqlx::query(
            r#"
        UPDATE journals
        SET is_archived = ?, updated_at = ?
        WHERE id = ?
        "#,
        )
        .bind(archived)
        .bind(&now)
        .bind(&request.journal_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to archive journal: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Journal not found: {}",
                request.journal_id
            )));
        }

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn delete_journal(
        &self,
        request: DeleteConversationJournalRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let journal_id = request.journal_id.trim();
        if journal_id.is_empty() {
            return Err(AppError::InvalidInput("journalId is required".to_string()));
        }

        let result = sqlx::query(
            r#"
        DELETE FROM journals
        WHERE id = ?
        "#,
        )
        .bind(journal_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete journal: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Journal not found: {}",
                journal_id
            )));
        }

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn add_conversation_to_journal(
        &self,
        request: AddConversationToJournalRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let journal_space_id = request.journal_space_id.trim();
        let conversation_id = request.conversation_id.trim();

        if journal_space_id.is_empty() {
            return Err(AppError::InvalidInput(
                "journalSpaceId is required".to_string(),
            ));
        }
        if conversation_id.is_empty() {
            return Err(AppError::InvalidInput(
                "conversationId is required".to_string(),
            ));
        }

        let mut tx = self
            .pool
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        ensure_journal_space(&mut tx, journal_space_id).await?;

        let conversation_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify conversation: {}", e)))?;

        if conversation_exists == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }

        sqlx::query(
        r#"
        INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(journal_space_id)
    .bind(conversation_id)
    .bind(lattice_core::shared::persistence::timestamps::now_db_timestamp())
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to add conversation to journal: {}",
            e
        ))
    })?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn remove_conversation_from_journal(
        &self,
        request: RemoveConversationFromJournalRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let journal_space_id = request.journal_space_id.trim();
        let conversation_id = request.conversation_id.trim();

        if journal_space_id.is_empty() {
            return Err(AppError::InvalidInput(
                "journalSpaceId is required".to_string(),
            ));
        }
        if conversation_id.is_empty() {
            return Err(AppError::InvalidInput(
                "conversationId is required".to_string(),
            ));
        }

        let mut tx = self
            .pool
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        ensure_journal_space(&mut tx, journal_space_id).await?;

        sqlx::query(
            r#"
        DELETE FROM journal_conversation_entries
        WHERE journal_space_id = ? AND conversation_id = ?
        "#,
        )
        .bind(journal_space_id)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to remove conversation from journal: {}", e))
        })?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    /// Pins or unpins one journal entry. Pinning an entry that is already
    /// pinned keeps when it was first pinned.
    pub async fn set_journal_entry_pinned(
        &self,
        request: SetJournalEntryPinnedRequestDto,
    ) -> Result<(), AppError> {
        let journal_space_id = request.journal_space_id.trim();
        let conversation_id = request.conversation_id.trim();
        if journal_space_id.is_empty() || conversation_id.is_empty() {
            return Err(AppError::InvalidInput(
                "journalSpaceId and conversationId are required".to_string(),
            ));
        }
        let mut tx = self
            .pool
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        ensure_journal_space(&mut tx, journal_space_id).await?;

        let result = sqlx::query(
            "UPDATE journal_conversation_entries
             SET pinned_at = CASE WHEN ? THEN COALESCE(pinned_at, ?) ELSE NULL END
             WHERE journal_space_id = ? AND conversation_id = ?",
        )
        .bind(request.pinned)
        .bind(lattice_core::shared::persistence::timestamps::now_db_timestamp())
        .bind(journal_space_id)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to pin journal entry: {}", e)))?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation {} is not in journal {}",
                conversation_id, journal_space_id
            )));
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    /// The conversations pinned in a journal, the most recently pinned first.
    pub async fn list_journal_entry_pins(
        &self,
        journal_space_id: &str,
    ) -> Result<Vec<String>, AppError> {
        let journal_space_id = journal_space_id.trim();
        let mut tx = self
            .pool
            .begin_read()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        ensure_journal_space(&mut tx, journal_space_id).await?;
        let pins = sqlx::query_scalar(
            "SELECT conversation_id FROM journal_conversation_entries
             WHERE journal_space_id = ? AND pinned_at IS NOT NULL
             ORDER BY pinned_at DESC, conversation_id",
        )
        .bind(journal_space_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list journal pins: {}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;
        Ok(pins)
    }

    pub async fn list_journal_conversations(
        &self,
        query: ListJournalConversationsQueryDto,
    ) -> Result<ListConversationsResponseDto, AppError> {
        let journal_space_id = query.journal_space_id.trim();
        if journal_space_id.is_empty() {
            return Err(AppError::InvalidInput(
                "journalSpaceId is required".to_string(),
            ));
        }

        let page = Page::new(query.limit, query.offset)?;
        let mut qb = ConversationListing {
            journal_space_id: Some(journal_space_id),
            include_archived: query.include_archived.unwrap_or(false),
            search: query.query.as_deref(),
            page,
            ..Default::default()
        }
        .query();

        let mut tx = self
            .pool
            .begin_read()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;
        ensure_journal_space(&mut tx, journal_space_id).await?;
        let rows = qb
            .build_query_as::<ConversationListRow>()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list journal conversations: {}", e))
            })?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        listing_response(rows)
    }
}
