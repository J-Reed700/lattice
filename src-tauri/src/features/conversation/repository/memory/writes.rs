//! Writes to the memory ledger: committing a compaction, recording an error, marking a rebuild.

use super::super::ConversationRepository;
use super::related_ids_to_json;
use crate::application::ports::conversation_memory::{
    CommittedMemorySnapshot, MemoryCommitCandidate, MemoryCommitError, MemoryCommitPreconditions,
};
use crate::domain::conversation::memory::{
    compute_digest, MemoryItem, MemoryKind, MEMORY_SCHEMA_VERSION,
};
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use std::str::FromStr;

impl ConversationRepository {
    /// Record a failed attempt without touching items, summary or watermark.
    ///
    /// Separate from a commit on purpose: a memory-maintenance failure must not
    /// be able to disturb an already-completed chat turn (invariant 16).
    pub async fn record_memory_error(&self, conversation_id: &str, code: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO conversation_memory_state \
                (conversation_id, schema_version, last_error_code, updated_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(conversation_id) DO UPDATE SET \
                last_error_code = excluded.last_error_code, \
                updated_at = excluded.updated_at",
        )
        .bind(conversation_id)
        .bind(MEMORY_SCHEMA_VERSION)
        .bind(code)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to record memory error: {}", e)))?;
        Ok(())
    }

    /// Mark memory as needing a rebuild and clear what can no longer be trusted.
    pub async fn mark_memory_rebuild_required(
        &self,
        conversation_id: &str,
        code: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin invalidation: {}", e)))?;
        for statement in [
            "DELETE FROM conversation_memory_items WHERE conversation_id = ?",
            "DELETE FROM conversation_memory_events WHERE conversation_id = ?",
            "DELETE FROM conversation_summaries WHERE conversation_id = ?",
        ] {
            sqlx::query(statement)
                .bind(conversation_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("Failed to clear memory: {}", e)))?;
        }
        sqlx::query(
            "INSERT INTO conversation_memory_state \
                (conversation_id, schema_version, memory_revision, source_transcript_revision, \
                 processed_through_sequence, validity, last_error_code, updated_at) \
             VALUES (?, ?, 1, 0, 0, 'rebuild_required', ?, ?) \
             ON CONFLICT(conversation_id) DO UPDATE SET \
                -- Brought back to this build's layout on purpose. A rebuild
                -- that left an unreadable schema_version in place could never
                -- make memory usable again: the content is already discarded,
                -- so the version has to follow it.
                schema_version = excluded.schema_version, \
                memory_revision = memory_revision + 1, \
                source_transcript_revision = 0, \
                processed_through_sequence = 0, \
                validity = 'rebuild_required', \
                last_error_code = excluded.last_error_code, \
                updated_at = excluded.updated_at",
        )
        .bind(conversation_id)
        .bind(MEMORY_SCHEMA_VERSION)
        .bind(code)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to mark rebuild required: {}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit invalidation: {}", e)))?;
        Ok(())
    }

    /// Apply a validated candidate under its preconditions, or write nothing.
    pub async fn commit_memory(
        &self,
        preconditions: &MemoryCommitPreconditions,
        candidate: &MemoryCommitCandidate,
    ) -> std::result::Result<CommittedMemorySnapshot, MemoryCommitError> {
        let conversation_id = preconditions.conversation_id.as_str();
        let db = |error: sqlx::Error| MemoryCommitError::Database(error.to_string());
        let now = Utc::now().to_rfc3339();

        // This path reads the idempotency row and revision preconditions
        // before it writes. Acquire the SQLite writer reservation up front so
        // concurrent commits serialize and the loser can observe the winner
        // (or return a typed revision conflict) instead of failing with
        // SQLITE_BUSY_SNAPSHOT after upgrading a stale read transaction.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;

        // --- idempotency -------------------------------------------------
        // Checked first: a crash between commit and response leaves the caller
        // retrying, and applying the same patch twice would duplicate items.
        #[derive(sqlx::FromRow)]
        struct ExistingEvent {
            memory_revision: i64,
        }
        if let Some(existing) = sqlx::query_as::<_, ExistingEvent>(
            "SELECT memory_revision FROM conversation_memory_events \
             WHERE conversation_id = ? AND operation_id = ?",
        )
        .bind(conversation_id)
        .bind(&preconditions.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            let counts = Self::count_active(&mut tx, conversation_id).await?;
            let state = Self::state_revisions(&mut tx, conversation_id).await?;
            tx.commit().await.map_err(db)?;
            return Ok(CommittedMemorySnapshot {
                memory_revision: existing.memory_revision,
                transcript_revision: state.1,
                processed_through_sequence: state.2,
                active_mandatory_count: counts.0,
                active_optional_count: counts.1,
                was_already_committed: true,
            });
        }

        // --- preconditions -----------------------------------------------
        let (memory_revision, transcript_revision, _) =
            Self::state_revisions(&mut tx, conversation_id).await?;
        if transcript_revision != preconditions.expected_transcript_revision {
            return Err(MemoryCommitError::TranscriptConflict {
                expected: preconditions.expected_transcript_revision,
                actual: transcript_revision,
            });
        }
        if memory_revision != preconditions.expected_memory_revision {
            return Err(MemoryCommitError::MemoryConflict {
                expected: preconditions.expected_memory_revision,
                actual: memory_revision,
            });
        }
        let schema_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT schema_version FROM conversation_memory_state \
              WHERE conversation_id = ?), ?)",
        )
        .bind(conversation_id)
        .bind(MEMORY_SCHEMA_VERSION)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if schema_version != MEMORY_SCHEMA_VERSION {
            return Err(MemoryCommitError::UnsupportedSchema(schema_version));
        }

        let next_revision = memory_revision + 1;

        // --- revalidate every span against live content ------------------
        // The model call happened outside this transaction, so the source may
        // have moved since. Trusting the candidate's own digests here would let
        // a span that stopped resolving be committed as authoritative.
        for item in candidate
            .commit
            .inserts
            .iter()
            .chain(&candidate.commit.updates)
        {
            for span in &item.evidence {
                #[derive(sqlx::FromRow)]
                struct Row {
                    content: String,
                    content_digest: String,
                }
                let row = sqlx::query_as::<_, Row>(
                    "SELECT content, content_digest FROM conversation_messages \
                     WHERE id = ? AND conversation_id = ?",
                )
                .bind(&span.message_id)
                .bind(conversation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| MemoryCommitError::UnresolvableEvidence(item.id.to_string()))?;
                // Ownership is enforced by the `WHERE` above: a span naming
                // another conversation's message resolves to no row at all.
                let live_digest = if row.content_digest.is_empty() {
                    compute_digest(&row.content)
                } else {
                    row.content_digest
                };
                if live_digest != span.content_digest || span.resolve(&row.content).is_none() {
                    return Err(MemoryCommitError::UnresolvableEvidence(item.id.to_string()));
                }
            }
        }

        // --- apply -------------------------------------------------------
        for item in &candidate.commit.inserts {
            Self::insert_memory_item(&mut tx, item, next_revision, &now)
                .await
                .map_err(db)?;
        }
        for item in &candidate.commit.updates {
            Self::update_memory_item(&mut tx, item, next_revision, &now)
                .await
                .map_err(db)?;
        }
        // Superseded-by links are set in a second pass: a replacement inserted
        // in this same patch does not exist when the item it retires is written,
        // and the foreign key would reject the forward reference.
        for item in &candidate.commit.updates {
            if let Some(replacement) = &item.superseded_by {
                sqlx::query(
                    "UPDATE conversation_memory_items SET superseded_by = ? \
                     WHERE id = ? AND conversation_id = ?",
                )
                .bind(replacement.as_str())
                .bind(item.id.as_str())
                .bind(conversation_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            }
        }

        if let Some(summary) = &candidate.summary {
            let ratio = if summary.original_tokens > 0 {
                (summary.summary_tokens as f64 / summary.original_tokens as f64).min(1.0)
            } else {
                1.0
            };
            sqlx::query(
                "INSERT INTO conversation_summaries \
                    (id, conversation_id, summary_text, up_to_message_id, \
                     original_message_count, original_tokens, summary_tokens, \
                     compression_ratio, created_at, memory_revision) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(conversation_id) DO UPDATE SET \
                    summary_text = excluded.summary_text, \
                    up_to_message_id = excluded.up_to_message_id, \
                    original_message_count = excluded.original_message_count, \
                    original_tokens = excluded.original_tokens, \
                    summary_tokens = excluded.summary_tokens, \
                    compression_ratio = excluded.compression_ratio, \
                    created_at = excluded.created_at, \
                    memory_revision = excluded.memory_revision",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(conversation_id)
            .bind(&summary.summary_text)
            .bind(&summary.up_to_message_id)
            .bind(summary.original_message_count.max(1))
            .bind(summary.original_tokens.max(1))
            .bind(summary.summary_tokens.max(1))
            .bind(ratio)
            .bind(&now)
            .bind(next_revision)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        sqlx::query(
            "INSERT INTO conversation_memory_state \
                (conversation_id, schema_version, memory_revision, source_transcript_revision, \
                 processed_through_sequence, validity, last_error_code, \
                 extractor_model_identity, extractor_prompt_version, validator_version, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'ready', NULL, ?, ?, ?, ?) \
             ON CONFLICT(conversation_id) DO UPDATE SET \
                memory_revision = excluded.memory_revision, \
                source_transcript_revision = excluded.source_transcript_revision, \
                processed_through_sequence = excluded.processed_through_sequence, \
                validity = 'ready', \
                last_error_code = NULL, \
                extractor_model_identity = excluded.extractor_model_identity, \
                extractor_prompt_version = excluded.extractor_prompt_version, \
                validator_version = excluded.validator_version, \
                updated_at = excluded.updated_at",
        )
        .bind(conversation_id)
        .bind(MEMORY_SCHEMA_VERSION)
        .bind(next_revision)
        .bind(candidate.transcript_revision)
        .bind(candidate.commit.processed_through_sequence)
        .bind(&candidate.extractor_model_identity)
        .bind(&candidate.extractor_prompt_version)
        .bind(&candidate.validator_version)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let touched: Vec<&str> = candidate
            .commit
            .inserts
            .iter()
            .chain(&candidate.commit.updates)
            .map(|item| item.id.as_str())
            .collect();
        sqlx::query(
            "INSERT INTO conversation_memory_events \
                (id, conversation_id, operation_id, memory_revision, operation, item_ids, \
                 source_message_ids, extractor_model_identity, extractor_prompt_version, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(conversation_id)
        .bind(&preconditions.operation_id)
        .bind(next_revision)
        .bind(&candidate.operation)
        .bind(serde_json::to_string(&touched).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&candidate.source_message_ids).unwrap_or_else(|_| "[]".into()))
        .bind(&candidate.extractor_model_identity)
        .bind(&candidate.extractor_prompt_version)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        // Bounded event retention. Deleting an event never touches item
        // evidence: they are separate tables with no cascade between them.
        sqlx::query(
            "DELETE FROM conversation_memory_events \
             WHERE conversation_id = ? AND id NOT IN ( \
                SELECT id FROM conversation_memory_events WHERE conversation_id = ? \
                ORDER BY memory_revision DESC, created_at DESC LIMIT 100)",
        )
        .bind(conversation_id)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let (mandatory, optional) = Self::count_active(&mut tx, conversation_id).await?;
        tx.commit().await.map_err(db)?;

        Ok(CommittedMemorySnapshot {
            memory_revision: next_revision,
            transcript_revision,
            processed_through_sequence: candidate.commit.processed_through_sequence,
            active_mandatory_count: mandatory,
            active_optional_count: optional,
            was_already_committed: false,
        })
    }

    /// `(memory_revision, transcript_revision, processed_through_sequence)`.
    async fn state_revisions(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
    ) -> std::result::Result<(i64, i64, i64), MemoryCommitError> {
        let db = |error: sqlx::Error| MemoryCommitError::Database(error.to_string());
        let transcript_revision: Option<i64> =
            sqlx::query_scalar("SELECT transcript_revision FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
        let transcript_revision = transcript_revision
            .ok_or_else(|| MemoryCommitError::NotFound(conversation_id.to_string()))?;

        #[derive(sqlx::FromRow)]
        struct Row {
            memory_revision: i64,
            processed_through_sequence: i64,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT memory_revision, processed_through_sequence \
             FROM conversation_memory_state WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;

        Ok(match row {
            Some(row) => (
                row.memory_revision,
                transcript_revision,
                row.processed_through_sequence,
            ),
            None => (0, transcript_revision, 0),
        })
    }

    /// `(mandatory, optional)` active item counts.
    async fn count_active(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
    ) -> std::result::Result<(usize, usize), MemoryCommitError> {
        let db = |error: sqlx::Error| MemoryCommitError::Database(error.to_string());
        #[derive(sqlx::FromRow)]
        struct Row {
            kind: String,
            count: i64,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT kind, COUNT(*) as count FROM conversation_memory_items \
             WHERE conversation_id = ? AND state = 'active' GROUP BY kind",
        )
        .bind(conversation_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db)?;

        let mut mandatory = 0usize;
        let mut optional = 0usize;
        for row in rows {
            let count = row.count.max(0) as usize;
            match MemoryKind::from_str(&row.kind) {
                Ok(kind) if kind.is_mandatory() => mandatory += count,
                Ok(_) => optional += count,
                // An unparseable kind cannot happen through this crate's writes
                // (the column has a CHECK), but counting it as mandatory is the
                // safe direction: it is reported, not silently dropped.
                Err(_) => mandatory += count,
            }
        }
        Ok((mandatory, optional))
    }

    async fn insert_memory_item(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        item: &MemoryItem,
        revision: i64,
        now: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO conversation_memory_items \
                (id, conversation_id, kind, state, label, created_at_sequence, \
                 changed_at_sequence, superseded_by, revision, review, related_item_ids, \
                 created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(item.id.as_str())
        .bind(&item.conversation_id)
        .bind(item.kind.as_str())
        .bind(item.state.as_str())
        .bind(&item.label)
        .bind(item.created_at_sequence)
        .bind(item.changed_at_sequence)
        .bind(revision)
        .bind(item.review.as_str())
        .bind(related_ids_to_json(&item.related_item_ids))
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
        Self::replace_evidence(tx, item).await
    }

    async fn update_memory_item(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        item: &MemoryItem,
        revision: i64,
        now: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE conversation_memory_items SET \
                kind = ?, state = ?, label = ?, changed_at_sequence = ?, revision = ?, \
                review = ?, related_item_ids = ?, updated_at = ? \
             WHERE id = ? AND conversation_id = ?",
        )
        .bind(item.kind.as_str())
        .bind(item.state.as_str())
        .bind(&item.label)
        .bind(item.changed_at_sequence)
        .bind(revision)
        .bind(item.review.as_str())
        .bind(related_ids_to_json(&item.related_item_ids))
        .bind(now)
        .bind(item.id.as_str())
        .bind(&item.conversation_id)
        .execute(&mut **tx)
        .await?;
        Self::replace_evidence(tx, item).await
    }

    /// Rewrite an item's evidence rows.
    ///
    /// Delete-then-insert keeps `(item_id, ordinal)` contiguous. The spans an
    /// update writes are the spans the domain produced, which always include
    /// the item's original assertions — an update never narrows the evidence a
    /// requirement rests on.
    async fn replace_evidence(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        item: &MemoryItem,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM conversation_memory_evidence WHERE item_id = ?")
            .bind(item.id.as_str())
            .execute(&mut **tx)
            .await?;
        for (ordinal, span) in item.evidence.iter().enumerate() {
            sqlx::query(
                "INSERT INTO conversation_memory_evidence \
                    (item_id, ordinal, message_id, sequence, role, start_byte, end_byte, \
                     content_digest, purpose) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.id.as_str())
            .bind(ordinal as i64)
            .bind(&span.message_id)
            .bind(span.sequence)
            .bind(span.role.as_str())
            .bind(span.start_byte as i64)
            .bind(span.end_byte as i64)
            .bind(&span.content_digest)
            .bind(span.purpose.as_str())
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}
