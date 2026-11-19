//! The names a conversation has given the things it showed its model.

use super::ConversationRepository;
use async_trait::async_trait;
use lattice_core::application::ports::conversation_names::ConversationNames;
use lattice_core::application::services::model_contract::shown::Record;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;
use sqlx::Row;

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidData(format!("Invalid name: {error}"))
}

#[async_trait]
impl ConversationNames for ConversationRepository {
    async fn names(&self, conversation_id: &str) -> Result<Vec<Record>> {
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query(
            "SELECT prefix, ordinal, under_prefix, under_ordinal, reference \
             FROM conversation_names WHERE conversation_id=? ORDER BY position",
        )
        .bind(conversation_id)
        .fetch_all(&mut *read)
        .await?;
        read.commit().await?;
        rows.iter()
            .map(|row| {
                let ordinal: i64 = row.get("ordinal");
                let under_prefix: Option<String> = row.get("under_prefix");
                let under_ordinal: Option<i64> = row.get("under_ordinal");
                let under = match (under_prefix, under_ordinal) {
                    (Some(prefix), Some(ordinal)) => {
                        Some((prefix, u32::try_from(ordinal).map_err(invalid)?))
                    }
                    _ => None,
                };
                Ok(Record {
                    prefix: row.get("prefix"),
                    ordinal: u32::try_from(ordinal).map_err(invalid)?,
                    under,
                    reference: serde_json::from_str(row.get("reference")).map_err(invalid)?,
                })
            })
            .collect()
    }

    async fn keep(&self, conversation_id: &str, first: usize, fresh: &[Record]) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        for (offset, record) in fresh.iter().enumerate() {
            let position = i64::try_from(first + offset).map_err(invalid)?;
            sqlx::query(
                "INSERT INTO conversation_names \
                 (conversation_id, position, prefix, ordinal, under_prefix, under_ordinal, reference) \
                 VALUES (?,?,?,?,?,?,?)",
            )
            .bind(conversation_id)
            .bind(position)
            .bind(&record.prefix)
            .bind(i64::from(record.ordinal))
            .bind(record.under.as_ref().map(|(prefix, _)| prefix.as_str()))
            .bind(record.under.as_ref().map(|(_, ordinal)| i64::from(*ordinal)))
            .bind(record.reference.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.map_err(AppError::from)
    }
}

impl ConversationRepository {
    /// Gives a conversation made from another the names of its parent, so a
    /// handle in the copied messages means what it meant there.
    pub(super) async fn copy_names(
        tx: &mut WriteTx,
        conversation_id: &str,
        new_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO conversation_names \
             (conversation_id, position, prefix, ordinal, under_prefix, under_ordinal, reference) \
             SELECT ?, position, prefix, ordinal, under_prefix, under_ordinal, reference \
             FROM conversation_names WHERE conversation_id = ?",
        )
        .bind(new_id)
        .bind(conversation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to copy names: {}", e)))?;
        Ok(())
    }
}
