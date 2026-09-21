use super::*;

fn preferences_declares_journal(raw: Option<&str>) -> bool {
    let Some(raw_json) = raw else {
        return false;
    };

    if raw_json.trim().is_empty() {
        return false;
    }

    let parsed: Result<Value, _> = serde_json::from_str(raw_json);
    let Ok(value) = parsed else {
        return false;
    };

    let kind = value
        .get("spaceType")
        .and_then(Value::as_str)
        .or_else(|| value.get("space_type").and_then(Value::as_str))
        .unwrap_or("standard");

    kind.eq_ignore_ascii_case("journal")
}

fn validate_space_preferences_not_journal(
    raw: Option<&str>,
    context: &str,
) -> Result<(), AppError> {
    if preferences_declares_journal(raw) {
        return Err(AppError::InvalidInput(format!(
            "{} cannot declare `spaceType=journal`; journals are a separate entity",
            context
        )));
    }
    Ok(())
}

pub(super) async fn ensure_standard_space(
    pool: &SqlitePool,
    space_id: &str,
) -> Result<(), AppError> {
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
            .bind(space_id)
            .fetch_one(pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to verify space: {}", e)))?;

    if exists == 0 {
        return Err(AppError::NotFound(format!("Space not found: {}", space_id)));
    }

    Ok(())
}

impl ConversationRepository {
    pub async fn list_conversation_spaces(&self) -> Result<Vec<ConversationSpaceDto>, AppError> {
        let spaces = sqlx::query_as::<_, ConversationSpaceDto>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM conversation_spaces
        ORDER BY is_archived ASC, sort_order ASC, updated_at DESC
        "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list spaces: {}", e)))?;

        Ok(spaces)
    }

    pub async fn update_conversation_space(
        &self,
        request: UpdateConversationSpaceRequestDto,
    ) -> Result<ConversationSpaceDto, AppError> {
        validate_space_preferences_not_journal(
            request.tool_preferences_json.as_deref(),
            "Conversation space",
        )?;

        let now = Utc::now().to_rfc3339();
        let is_archived: Option<i64> = request.is_archived.map(|v| if v { 1 } else { 0 });

        let result = sqlx::query(
            r#"
        UPDATE conversation_spaces
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
        .bind(&request.space_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update space: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Space not found: {}",
                request.space_id
            )));
        }

        let updated = sqlx::query_as::<_, ConversationSpaceDto>(
            r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM conversation_spaces
        WHERE id = ?
        "#,
        )
        .bind(&request.space_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch updated space: {}", e)))?;

        Ok(updated)
    }

    pub async fn archive_conversation_space(
        &self,
        request: ArchiveConversationSpaceRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        if request.space_id == DEFAULT_SPACE_ID && request.archived {
            return Err(AppError::InvalidInput(
                "The default space cannot be archived".to_string(),
            ));
        }

        let now = Utc::now().to_rfc3339();
        let archived = if request.archived { 1 } else { 0 };

        let result = sqlx::query(
            r#"
        UPDATE conversation_spaces
        SET is_archived = ?, updated_at = ?
        WHERE id = ?
        "#,
        )
        .bind(archived)
        .bind(&now)
        .bind(&request.space_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to archive space: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Space not found: {}",
                request.space_id
            )));
        }

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn move_conversation_to_space(
        &self,
        request: MoveConversationToSpaceRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        ensure_standard_space(&self.pool, &request.space_id).await?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let _current_space_id = sqlx::query_scalar::<_, String>(
            "SELECT space_id FROM conversations WHERE id = ? LIMIT 1",
        )
        .bind(&request.conversation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to resolve current conversation space: {}",
                e
            ))
        })?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Conversation not found: {}",
                request.conversation_id
            ))
        })?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
        UPDATE conversations
        SET space_id = ?, updated_at = ?
        WHERE id = ?
        "#,
        )
        .bind(&request.space_id)
        .bind(&now)
        .bind(&request.conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to move conversation: {}", e)))?;

        // Only the conversation moves. Filing the documents it once cited into
        // the destination as well made every other chat there search them too:
        // moving one chat out of a patent space put the patent library in front
        // of the whole space it landed in. What a space holds is decided by
        // filing documents, never as a side effect of moving a chat.

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }
}
