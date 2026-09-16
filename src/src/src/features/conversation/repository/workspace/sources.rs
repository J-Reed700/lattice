use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationLinkedDocumentRow {
    document_id: String,
    file_name: String,
    file_path: String,
    file_type: Option<String>,
    category: String,
    indexed_at: String,
    last_referenced_at: String,
    reference_count: i64,
}
#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationWebSourceRow {
    id: String,
    url: String,
    normalized_url: String,
    title: Option<String>,
    excerpt: Option<String>,
    relevance_score: Option<f32>,
    added_at: String,
}
fn normalize_web_source_url(url: &str) -> String {
    url.trim().to_ascii_lowercase()
}

impl ConversationRepository {
    pub async fn list_conversation_linked_documents(
        &self,
        conversation_id: String,
    ) -> Result<Vec<ConversationLinkedDocumentDto>, AppError> {
        let linked_docs = sqlx::query_as::<_, ConversationLinkedDocumentRow>(
            r#"
        SELECT
            d.id AS document_id,
            d.file_name AS file_name,
            d.file_path AS file_path,
            d.file_type AS file_type,
            d.category AS category,
            d.indexed_at AS indexed_at,
            MAX(cd.added_at) AS last_referenced_at,
            COUNT(*) AS reference_count
        FROM conversation_documents cd
        INNER JOIN documents d ON d.id = cd.document_id
        WHERE cd.conversation_id = ?
        GROUP BY d.id, d.file_name, d.file_path, d.file_type, d.category, d.indexed_at
        ORDER BY last_referenced_at DESC
        "#,
        )
        .bind(&conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to list conversation linked documents: {}",
                e
            ))
        })?;

        Ok(linked_docs
            .into_iter()
            .map(|row| ConversationLinkedDocumentDto {
                document_id: row.document_id,
                file_name: row.file_name,
                file_path: row.file_path,
                file_type: row.file_type.unwrap_or_default(),
                category: row.category,
                indexed_at: row.indexed_at,
                last_referenced_at: row.last_referenced_at,
                reference_count: row.reference_count,
            })
            .collect())
    }

    pub async fn remove_conversation_linked_document(
        &self,
        conversation_id: String,
        document_id: String,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let conversation_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
                .bind(&conversation_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify conversation: {}", e)))?;

        if conversation_exists == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }

        let deleted = sqlx::query(
            r#"
        DELETE FROM conversation_documents
        WHERE conversation_id = ? AND document_id = ?
        "#,
        )
        .bind(&conversation_id)
        .bind(&document_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to remove linked document from conversation: {}",
                e
            ))
        })?;

        if deleted.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Document {} is not linked to conversation {}",
                document_id, conversation_id
            )));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn add_conversation_web_source(
        &self,
        conversation_id: String,
        url: String,
        title: Option<String>,
        excerpt: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let conversation_id = conversation_id.trim().to_string();
        if conversation_id.is_empty() {
            return Err(AppError::InvalidInput(
                "conversationId is required".to_string(),
            ));
        }

        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(AppError::InvalidInput("url is required".to_string()));
        }

        let normalized_url = normalize_web_source_url(&url);
        let now = Utc::now().to_rfc3339();
        let source_id = format!("cws_{}", uuid::Uuid::new_v4().simple());

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let conversation_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
                .bind(&conversation_id)
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
        INSERT INTO conversation_web_sources (
            id,
            conversation_id,
            url,
            normalized_url,
            title,
            excerpt,
            relevance_score,
            added_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(conversation_id, normalized_url) DO UPDATE SET
            url = excluded.url,
            title = COALESCE(excluded.title, conversation_web_sources.title),
            excerpt = COALESCE(excluded.excerpt, conversation_web_sources.excerpt),
            relevance_score = COALESCE(excluded.relevance_score, conversation_web_sources.relevance_score),
            added_at = excluded.added_at
        "#,
    )
    .bind(&source_id)
    .bind(&conversation_id)
    .bind(&url)
    .bind(&normalized_url)
    .bind(title.as_ref().map(|value| value.trim().to_string()))
    .bind(excerpt.as_ref().map(|value| value.trim().to_string()))
    .bind(relevance_score)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to upsert conversation web source: {}",
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

    pub async fn list_conversation_web_sources(
        &self,
        conversation_id: String,
    ) -> Result<Vec<ConversationWebSourceDto>, AppError> {
        let rows = sqlx::query_as::<_, ConversationWebSourceRow>(
            r#"
        SELECT
            id,
            url,
            normalized_url,
            title,
            excerpt,
            relevance_score,
            added_at
        FROM conversation_web_sources
        WHERE conversation_id = ?
        ORDER BY added_at DESC
        "#,
        )
        .bind(&conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to list conversation web sources: {}", e))
        })?;

        Ok(rows
            .into_iter()
            .map(|row| ConversationWebSourceDto {
                id: row.id,
                url: row.url,
                normalized_url: row.normalized_url,
                title: row.title,
                excerpt: row.excerpt,
                relevance_score: row.relevance_score,
                added_at: row.added_at,
            })
            .collect())
    }

    pub async fn remove_conversation_web_source(
        &self,
        conversation_id: String,
        source_id: String,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let deleted = sqlx::query(
            r#"
        DELETE FROM conversation_web_sources
        WHERE conversation_id = ? AND id = ?
        "#,
        )
        .bind(&conversation_id)
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to remove conversation web source: {}", e))
        })?;

        if deleted.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Web source {} not found in conversation {}",
                source_id, conversation_id
            )));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }
}
