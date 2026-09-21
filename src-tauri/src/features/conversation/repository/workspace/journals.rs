use super::*;

pub(super) async fn ensure_journal_space(
    pool: &SqlitePool,
    journal_space_id: &str,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM journals WHERE id = ?")
        .bind(journal_space_id)
        .fetch_one(pool)
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
            default_model_name,
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

        sqlx::query(
        r#"
        INSERT INTO journals (
            id, name, description, icon, accent_color, space_prompt,
            default_model_name, tool_preferences_json, is_archived, sort_order, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(icon)
    .bind(accent_color)
    .bind(space_prompt)
    .bind(default_model_name)
    .bind(tool_preferences_json)
    .bind(&now)
    .bind(&now)
    .execute(&self.pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to create journal: {}", e)))?;

        let created = sqlx::query_as::<_, ConversationJournalDto>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch created journal: {}", e)))?;

        Ok(created)
    }

    pub async fn list_journals(&self) -> Result<Vec<ConversationJournalDto>, AppError> {
        let journals = sqlx::query_as::<_, ConversationJournalDto>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        ORDER BY is_archived ASC, sort_order ASC, updated_at DESC
        "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list journals: {}", e)))?;

        Ok(journals)
    }

    pub async fn update_journal(
        &self,
        request: UpdateConversationJournalRequestDto,
    ) -> Result<ConversationJournalDto, AppError> {
        let now = Utc::now().to_rfc3339();
        let is_archived: Option<i64> = request.is_archived.map(|v| if v { 1 } else { 0 });

        let result = sqlx::query(
            r#"
        UPDATE journals
        SET
            name = COALESCE(?, name),
            description = COALESCE(?, description),
            icon = COALESCE(?, icon),
            accent_color = COALESCE(?, accent_color),
            space_prompt = COALESCE(?, space_prompt),
            default_model_name = COALESCE(?, default_model_name),
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
        .bind(request.default_model_name)
        .bind(request.tool_preferences_json)
        .bind(is_archived)
        .bind(request.sort_order)
        .bind(&now)
        .bind(&request.journal_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update journal: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Journal not found: {}",
                request.journal_id
            )));
        }

        let updated = sqlx::query_as::<_, ConversationJournalDto>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
        )
        .bind(&request.journal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch updated journal: {}", e)))?;

        Ok(updated)
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

        ensure_journal_space(&self.pool, journal_space_id).await?;

        let conversation_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_one(&self.pool)
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
    .bind(crate::shared::time::now_db_timestamp())
    .execute(&self.pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to add conversation to journal: {}",
            e
        ))
    })?;

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

        ensure_journal_space(&self.pool, journal_space_id).await?;

        sqlx::query(
            r#"
        DELETE FROM journal_conversation_entries
        WHERE journal_space_id = ? AND conversation_id = ?
        "#,
        )
        .bind(journal_space_id)
        .bind(conversation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to remove conversation from journal: {}", e))
        })?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
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

        ensure_journal_space(&self.pool, journal_space_id).await?;

        let limit = query.limit.unwrap_or(120).clamp(1, 200);
        let offset = query.offset.unwrap_or(0).max(0);
        let include_archived = query.include_archived.unwrap_or(false);
        let search_query = query
            .query
            .as_ref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty());
        let fts_query = search_query.and_then(build_fts_query);

        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
        SELECT
            c.id,
            c.title,
            c.model_name,
            c.system_prompt,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.total_tokens,
            c.space_id,
            c.is_saved,
            c.is_bookmarked,
            c.is_pinned,
            c.is_archived,
            c.saved_at,
            c.bookmarked_at,
            c.pinned_at,
            c.archived_at,
            c.forked_from_conversation_id,
            c.forked_from_message_id,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE EXISTS (
                SELECT 1
                FROM journal_conversation_entries jce
                WHERE jce.journal_space_id =
        "#,
        );
        qb.push_bind(journal_space_id)
            .push(" AND jce.conversation_id = c.id")
            .push(")");

        if !include_archived {
            qb.push(" AND c.is_archived = 0");
        }

        if let Some(fts) = &fts_query {
            qb.push(
                " AND EXISTS (
                SELECT 1
                FROM conversation_search_fts fts
                WHERE fts.conversation_id = c.id
                  AND fts.content MATCH ",
            )
            .push_bind(fts.clone())
            .push(")");
        }

        if let Some(fts) = &fts_query {
            qb.push(
                " ORDER BY
                c.is_pinned DESC,
                COALESCE((
                    SELECT MIN(bm25(conversation_search_fts))
                    FROM conversation_search_fts
                    WHERE conversation_id = c.id
                      AND content MATCH ",
            )
            .push_bind(fts.clone())
            .push(
                "
                ), 999999.0),
                c.updated_at DESC",
            );
        } else {
            qb.push(" ORDER BY c.is_pinned DESC, c.updated_at DESC");
        }

        qb.push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = qb
            .build_query_as::<ConversationExplorerRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list journal conversations: {}", e))
            })?;

        let conversations = rows
            .into_iter()
            .map(|row| crate::features::conversation::dto::ConversationDto {
                id: row.id,
                title: row.title,
                model_name: row.model_name,
                system_prompt: row.system_prompt,
                created_at: row.created_at,
                updated_at: row.updated_at,
                message_count: row.message_count,
                total_tokens: row.total_tokens,
                space_id: Some(row.space_id),
                is_saved: Some(row.is_saved != 0),
                is_bookmarked: Some(row.is_bookmarked != 0),
                is_pinned: Some(row.is_pinned != 0),
                is_archived: Some(row.is_archived != 0),
                saved_at: row.saved_at,
                bookmarked_at: row.bookmarked_at,
                pinned_at: row.pinned_at,
                archived_at: row.archived_at,
                last_message_preview: row.last_message_preview,
                compaction: None,
                forked_from_conversation_id: row.forked_from_conversation_id,
                forked_from_message_id: row.forked_from_message_id,
            })
            .collect::<Vec<_>>();

        Ok(ListConversationsResponseDto {
            total: conversations.len(),
            conversations,
        })
    }
}
