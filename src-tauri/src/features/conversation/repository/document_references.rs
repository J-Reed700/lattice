//! Documents linked to a conversation, and the space memberships they imply.

use super::ConversationRepository;
use crate::domain::conversation::DocumentReference;
use crate::features::conversation::persistence_mapper::{
    DocumentReferenceMapper, DocumentReferenceModel,
};
use crate::shared::error::{AppError, Result};
use chrono::Utc;

impl ConversationRepository {
    pub async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&str>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        // Both inserts must commit together. If the membership insert
        // fails after the conversation_documents insert, we'd otherwise
        // have a doc linked to the conversation that is invisible in
        // the conversation's space — breaks the invariant that linked
        // chat docs are also space members. Drop-on-error rolls back.
        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!(
                "Failed to open tx for add_document_reference: {}",
                e
            ))
        })?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_documents (conversation_id, document_id, chunk_id, relevance_score, added_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(conversation_id)
        .bind(document_id)
        .bind(chunk_id)
        .bind(relevance_score)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to add document reference: {}", e)))?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
            SELECT ?, c.space_id, ?
            FROM conversations c
            WHERE c.id = ?
              AND c.space_id IS NOT NULL
              AND TRIM(c.space_id) <> ''
            "#,
        )
        .bind(document_id)
        .bind(&now)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to add document-space membership for reference: {}",
                e
            ))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit add_document_reference: {}", e))
        })?;

        Ok(())
    }

    pub async fn get_document_references(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<DocumentReference>> {
        let db_models = sqlx::query_as::<_, DocumentReferenceModel>(
            r#"
            SELECT document_id, chunk_id, relevance_score, added_at
            FROM conversation_documents
            WHERE conversation_id = ?
            ORDER BY added_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get document references: {}", e)))?;

        Ok(DocumentReferenceMapper::to_entities(&db_models))
    }
}
