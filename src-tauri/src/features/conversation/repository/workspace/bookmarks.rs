use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
struct MessageBookmarkRow {
    id: String,
    conversation_id: String,
    conversation_title: String,
    space_id: String,
    message_id: String,
    message_role: String,
    message_preview: String,
    title: Option<String>,
    note: Option<String>,
    created_at: String,
}

impl ConversationRepository {
    pub async fn bookmark_conversation_message(
        &self,
        request: BookmarkConversationMessageRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let exists: i64 = sqlx::query_scalar(
            r#"
        SELECT COUNT(*)
        FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
        )
        .bind(&request.message_id)
        .bind(&request.conversation_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to verify message bookmark target: {}", e))
        })?;

        if exists == 0 {
            return Err(AppError::NotFound(format!(
                "Message {} does not belong to conversation {}",
                request.message_id, request.conversation_id
            )));
        }

        let id = format!("cmb_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
        INSERT INTO conversation_message_bookmarks (
            id, conversation_id, message_id, title, note, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(conversation_id, message_id)
        DO UPDATE SET
            title = excluded.title,
            note = excluded.note,
            created_at = excluded.created_at
        "#,
        )
        .bind(id)
        .bind(&request.conversation_id)
        .bind(&request.message_id)
        .bind(request.title)
        .bind(request.note)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to bookmark conversation message: {}", e))
        })?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn unbookmark_conversation_message(
        &self,
        request: UnbookmarkConversationMessageRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        sqlx::query(
            r#"
        DELETE FROM conversation_message_bookmarks
        WHERE conversation_id = ? AND message_id = ?
        "#,
        )
        .bind(&request.conversation_id)
        .bind(&request.message_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to remove message bookmark: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn delete_conversation_message(
        &self,
        request: DeleteConversationMessageRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!("Failed to start delete message transaction: {}", e))
        })?;

        let exists: i64 = sqlx::query_scalar(
            r#"
        SELECT COUNT(*)
        FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
        )
        .bind(&request.message_id)
        .bind(&request.conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to verify message deletion target: {}", e))
        })?;

        if exists == 0 {
            return Err(AppError::NotFound(format!(
                "Message {} does not belong to conversation {}",
                request.message_id, request.conversation_id
            )));
        }

        sqlx::query(
            r#"
        DELETE FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
        )
        .bind(&request.message_id)
        .bind(&request.conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete message: {}", e)))?;

        #[derive(sqlx::FromRow)]
        struct MessageStatsRow {
            message_count: i64,
            total_tokens: i64,
        }

        let stats = sqlx::query_as::<_, MessageStatsRow>(
            r#"
        SELECT
            COUNT(*) AS message_count,
            COALESCE(SUM(tokens), 0) AS total_tokens
        FROM conversation_messages
        WHERE conversation_id = ?
        "#,
        )
        .bind(&request.conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to recalculate conversation message stats: {}",
                e
            ))
        })?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
        UPDATE conversations
        SET message_count = ?,
            total_tokens = ?,
            updated_at = ?
        WHERE id = ?
        "#,
        )
        .bind(stats.message_count)
        .bind(stats.total_tokens)
        .bind(now)
        .bind(&request.conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to update conversation stats after message deletion: {}",
                e
            ))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!(
                "Failed to commit delete message transaction: {}",
                e
            ))
        })?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn list_message_bookmarks(
        &self,
        query: ListMessageBookmarksQueryDto,
    ) -> Result<ListMessageBookmarksResponseDto, AppError> {
        let limit = query.limit.unwrap_or(100).clamp(1, 200);
        let offset = query.offset.unwrap_or(0).max(0);
        let search_query = query
            .query
            .as_ref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty());
        let fts_query = search_query.and_then(build_fts_query);

        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
        SELECT
            b.id,
            b.conversation_id,
            c.title AS conversation_title,
            c.space_id AS space_id,
            b.message_id,
            m.role AS message_role,
            m.content AS message_preview,
            b.title,
            b.note,
            b.created_at
        FROM conversation_message_bookmarks b
        INNER JOIN conversations c ON c.id = b.conversation_id
        INNER JOIN conversation_messages m ON m.id = b.message_id
        WHERE 1 = 1
        "#,
        );

        if let Some(conversation_id) = &query.conversation_id {
            qb.push(" AND b.conversation_id = ")
                .push_bind(conversation_id);
        }

        if let Some(fts) = &fts_query {
            qb.push(
                " AND EXISTS (
                SELECT 1
                FROM conversation_search_fts fts
                WHERE fts.conversation_id = b.conversation_id
                  AND fts.content MATCH ",
            )
            .push_bind(fts.clone())
            .push(
                "
                  AND (
                    (fts.source = 'bookmark' AND fts.message_id = b.message_id)
                    OR (fts.source = 'message' AND fts.message_id = b.message_id)
                    OR fts.source = 'title'
                  )
            )",
            );
        }

        if let Some(fts) = &fts_query {
            qb.push(
                " ORDER BY
                COALESCE((
                    SELECT MIN(bm25(conversation_search_fts))
                    FROM conversation_search_fts
                    WHERE conversation_id = b.conversation_id
                      AND content MATCH ",
            )
            .push_bind(fts.clone())
            .push(
                "
                ), 999999.0),
                b.created_at DESC",
            );
        } else {
            qb.push(" ORDER BY b.created_at DESC");
        }

        qb.push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = qb
            .build_query_as::<MessageBookmarkRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to list message bookmarks: {}", e)))?;

        let bookmarks = rows
            .into_iter()
            .map(|row| ConversationMessageBookmarkDto {
                id: row.id,
                conversation_id: row.conversation_id,
                conversation_title: row.conversation_title,
                space_id: row.space_id,
                message_id: row.message_id,
                message_role: row.message_role,
                message_preview: row.message_preview,
                title: row.title,
                note: row.note,
                created_at: row.created_at,
            })
            .collect::<Vec<_>>();

        Ok(ListMessageBookmarksResponseDto {
            total: bookmarks.len(),
            bookmarks,
        })
    }
}
