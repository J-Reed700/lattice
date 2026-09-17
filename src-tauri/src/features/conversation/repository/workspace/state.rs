use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationStateRow {
    space_id: String,
    is_saved: i64,
    is_bookmarked: i64,
    is_pinned: i64,
    is_archived: i64,
    saved_at: Option<String>,
    bookmarked_at: Option<String>,
    pinned_at: Option<String>,
    archived_at: Option<String>,
    last_message_preview: Option<String>,
}
async fn fetch_conversation_state(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<ConversationStateRow>, AppError> {
    let state = sqlx::query_as::<_, ConversationStateRow>(
        r#"
        SELECT
            c.space_id AS space_id,
            c.is_saved AS is_saved,
            c.is_bookmarked AS is_bookmarked,
            c.is_pinned AS is_pinned,
            c.is_archived AS is_archived,
            c.saved_at AS saved_at,
            c.bookmarked_at AS bookmarked_at,
            c.pinned_at AS pinned_at,
            c.archived_at AS archived_at,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE c.id = ?
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch conversation state: {}", e)))?;

    Ok(state)
}

fn to_conversation_dto(
    c: &crate::domain::conversation::Conversation,
    state: Option<ConversationStateRow>,
    compaction: Option<CompactionRecordDto>,
) -> crate::features::conversation::dto::ConversationDto {
    let (
        space_id,
        is_saved,
        is_bookmarked,
        is_pinned,
        is_archived,
        saved_at,
        bookmarked_at,
        pinned_at,
        archived_at,
        last_message_preview,
    ) = if let Some(s) = state {
        (
            Some(s.space_id),
            Some(s.is_saved != 0),
            Some(s.is_bookmarked != 0),
            Some(s.is_pinned != 0),
            Some(s.is_archived != 0),
            s.saved_at,
            s.bookmarked_at,
            s.pinned_at,
            s.archived_at,
            s.last_message_preview,
        )
    } else {
        (
            Some(DEFAULT_SPACE_ID.to_string()),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            None,
            None,
            None,
            None,
            None,
        )
    };

    crate::features::conversation::dto::ConversationDto {
        id: c.id.to_string(),
        title: c.title.clone(),
        model_name: c.model_name.clone(),
        system_prompt: c.system_prompt.clone(),
        created_at: c.created_at.to_rfc3339(),
        updated_at: c.updated_at.to_rfc3339(),
        message_count: c.message_count,
        total_tokens: c.total_tokens,
        space_id,
        is_saved,
        is_bookmarked,
        is_pinned,
        is_archived,
        saved_at,
        bookmarked_at,
        pinned_at,
        archived_at,
        last_message_preview,
        compaction,
    }
}

async fn set_conversation_state(
    pool: &SqlitePool,
    conversation_id: &str,
    column_name: &str,
    timestamp_column: &str,
    value: bool,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let mut qb = QueryBuilder::<Sqlite>::new("UPDATE conversations SET ");
    qb.push(column_name)
        .push(" = ")
        .push_bind(if value { 1_i64 } else { 0_i64 })
        .push(", ")
        .push(timestamp_column)
        .push(" = ")
        .push_bind(if value {
            Some(now.clone())
        } else {
            None::<String>
        })
        .push(", updated_at = ")
        .push_bind(now)
        .push(" WHERE id = ")
        .push_bind(conversation_id);

    let result =
        qb.build().execute(pool).await.map_err(|e| {
            AppError::Database(format!("Failed to update conversation state: {}", e))
        })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Conversation not found: {}",
            conversation_id
        )));
    }

    Ok(())
}

impl ConversationRepository {
    pub async fn set_conversation_saved(
        &self,
        request: SetConversationStateRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        set_conversation_state(
            &self.pool,
            &request.conversation_id,
            "is_saved",
            "saved_at",
            request.value,
        )
        .await?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn set_conversation_bookmarked(
        &self,
        request: SetConversationStateRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        set_conversation_state(
            &self.pool,
            &request.conversation_id,
            "is_bookmarked",
            "bookmarked_at",
            request.value,
        )
        .await?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn set_conversation_pinned(
        &self,
        request: SetConversationStateRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        set_conversation_state(
            &self.pool,
            &request.conversation_id,
            "is_pinned",
            "pinned_at",
            request.value,
        )
        .await?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn set_conversation_archived(
        &self,
        request: SetConversationStateRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        set_conversation_state(
            &self.pool,
            &request.conversation_id,
            "is_archived",
            "archived_at",
            request.value,
        )
        .await?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn project_conversation(
        &self,
        conversation: &crate::domain::conversation::Conversation,
    ) -> Result<ConversationDto, AppError> {
        let state = fetch_conversation_state(&self.pool, &conversation.id.to_string()).await?;
        let compaction = match self.get_summary(&conversation.id.to_string()).await {
            Ok(Some(record)) => Some(CompactionRecordDto::from_record(&record)),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    conversation_id = %conversation.id,
                    error = %e,
                    "Failed to load conversation compaction summary"
                );
                None
            }
        };

        Ok(to_conversation_dto(conversation, state, compaction))
    }
}
