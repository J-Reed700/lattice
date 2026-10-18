//! SQL for the bounded conversation-memory ledger.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §5.4, §11.
//!
//! Two things here are load-bearing and easy to lose in a refactor:
//!
//! 1. **One read.** [`ConversationRepository::load_memory_snapshot`] reads state,
//!    items, evidence and summary inside a single transaction. Several
//!    independent reads can interleave with a commit and return the summary from
//!    revision N beside the ledger from N+1 — a pairing that was never true.
//!
//! 2. **One write, with preconditions.** [`ConversationRepository::commit_memory`]
//!    checks both revisions, re-resolves every evidence span against live
//!    content, and applies items, evidence, summary, watermark, revision and the
//!    event row together. No model call happens inside it: a SQLite write lock
//!    is never held across inference.
//!
//! Invalidation on a source rewrite is *not* here. It lives in SQL triggers
//! (`trg_conversation_memory_invalidate_*`), so it participates in whatever
//! transaction did the rewrite and no new call site can forget it.

use super::ConversationRepository;
use chrono::{DateTime, Utc};
use lattice_core::domain::conversation::memory::{
    compute_digest, ConversationMemoryState, EvidencePurpose, EvidenceSpan, MemoryId, MemoryItem,
    MemoryKind, MemoryReview, MemorySnapshot, MemoryState, SourceMessage, SourceRole,
    MAX_ACTIVE_ITEMS, MAX_RELATED_ITEMS,
};
use lattice_core::shared::error::{AppError, Result};
use std::str::FromStr;

mod source_reads;
mod writes;

/// Row shape for a source message read by the memory layer.
#[derive(sqlx::FromRow)]
struct SourceMessageRow {
    id: String,
    conversation_id: String,
    sequence: i64,
    role: String,
    content: String,
    content_digest: String,
    status: String,
    metadata: Option<String>,
}

impl SourceMessageRow {
    /// Convert to the domain value.
    ///
    /// An empty `content_digest` means the row was written by a path that
    /// predates memory — a repository test fixture, or the conversation-export
    /// scratch database. Those rows get their digest computed here rather than
    /// being treated as corrupt, so a fixture-built conversation is still a
    /// usable source. A *non-empty* digest is left exactly as stored: if it no
    /// longer matches the content, that is a real mismatch and the memory layer
    /// must see it.
    fn into_domain(self) -> Result<SourceMessage> {
        let digest = if self.content_digest.is_empty() {
            compute_digest(&self.content)
        } else {
            self.content_digest
        };
        let role = SourceRole::from_str(&self.role)?;
        let pictures = if role == SourceRole::User {
            lattice_core::shared::markdown_media::attached_picture_names(self.metadata.as_deref())
        } else {
            Vec::new()
        };
        Ok(SourceMessage {
            id: self.id,
            conversation_id: self.conversation_id,
            sequence: self.sequence,
            role,
            content: self.content,
            content_digest: digest,
            status: self.status,
            pictures,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MemoryStateRow {
    conversation_id: String,
    schema_version: i64,
    memory_revision: i64,
    source_transcript_revision: i64,
    processed_through_sequence: i64,
    validity: String,
    last_error_code: Option<String>,
    extractor_model_identity: Option<String>,
    extractor_prompt_version: Option<String>,
    validator_version: Option<String>,
    updated_at: String,
}

impl MemoryStateRow {
    fn into_domain(self) -> Result<ConversationMemoryState> {
        Ok(ConversationMemoryState {
            conversation_id: self.conversation_id,
            schema_version: self.schema_version,
            memory_revision: self.memory_revision,
            source_transcript_revision: self.source_transcript_revision,
            processed_through_sequence: self.processed_through_sequence,
            validity: self.validity.parse()?,
            last_error_code: self.last_error_code,
            extractor_model_identity: self.extractor_model_identity,
            extractor_prompt_version: self.extractor_prompt_version,
            validator_version: self.validator_version,
            updated_at: parse_timestamp(&self.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct MemoryItemRow {
    id: String,
    conversation_id: String,
    kind: String,
    state: String,
    label: String,
    created_at_sequence: i64,
    changed_at_sequence: i64,
    superseded_by: Option<String>,
    revision: i64,
    review: String,
    related_item_ids: String,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct EvidenceRow {
    item_id: String,
    message_id: String,
    sequence: i64,
    role: String,
    start_byte: i64,
    end_byte: i64,
    content_digest: String,
    purpose: String,
}

/// Lenient timestamp parse. A stored value that is not RFC 3339 (SQLite's
/// `CURRENT_TIMESTAMP` writes `YYYY-MM-DD HH:MM:SS`) still yields a usable
/// instant rather than failing a whole snapshot read over a display field.
fn parse_timestamp(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
        .unwrap_or_else(|_| Utc::now())
}

fn related_ids_from_json(raw: &str) -> Vec<MemoryId> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| MemoryId::from_string(id).ok())
        .take(MAX_RELATED_ITEMS)
        .collect()
}

fn related_ids_to_json(ids: &[MemoryId]) -> String {
    let ids: Vec<&str> = ids
        .iter()
        .take(MAX_RELATED_ITEMS)
        .map(MemoryId::as_str)
        .collect();
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
}

/// Assemble items from their rows plus a flat evidence list.
fn assemble_items(
    item_rows: Vec<MemoryItemRow>,
    evidence_rows: Vec<EvidenceRow>,
) -> Result<Vec<MemoryItem>> {
    let mut by_item: std::collections::HashMap<String, Vec<EvidenceSpan>> =
        std::collections::HashMap::new();
    for row in evidence_rows {
        // Offsets are stored as INTEGER; the domain uses u32. A negative or
        // oversized value cannot come from this crate's writes, and clamping it
        // would invent a span, so the row is skipped and the item ends up
        // unsupported — which the caller already has to handle.
        let (Ok(start_byte), Ok(end_byte)) =
            (u32::try_from(row.start_byte), u32::try_from(row.end_byte))
        else {
            continue;
        };
        by_item.entry(row.item_id).or_default().push(EvidenceSpan {
            message_id: row.message_id,
            sequence: row.sequence,
            role: SourceRole::from_str(&row.role)?,
            start_byte,
            end_byte,
            content_digest: row.content_digest,
            purpose: EvidencePurpose::from_str(&row.purpose)?,
        });
    }

    item_rows
        .into_iter()
        .map(|row| {
            let evidence = by_item.remove(&row.id).unwrap_or_default();
            Ok(MemoryItem {
                id: MemoryId::from_string(row.id)?,
                conversation_id: row.conversation_id,
                kind: MemoryKind::from_str(&row.kind)?,
                state: MemoryState::from_str(&row.state)?,
                label: row.label,
                evidence,
                created_at_sequence: row.created_at_sequence,
                changed_at_sequence: row.changed_at_sequence,
                superseded_by: row
                    .superseded_by
                    .and_then(|id| MemoryId::from_string(id).ok()),
                revision: row.revision,
                review: MemoryReview::from_str(&row.review)?,
                related_item_ids: related_ids_from_json(&row.related_item_ids),
                created_at: parse_timestamp(&row.created_at),
                updated_at: parse_timestamp(&row.updated_at),
            })
        })
        .collect()
}

const SOURCE_COLUMNS: &str =
    "id, conversation_id, sequence, role, content, content_digest, status, metadata";

impl ConversationRepository {
    /// One consistent read of everything the memory layer needs.
    pub async fn load_memory_snapshot(&self, conversation_id: &str) -> Result<MemorySnapshot> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!("Failed to begin memory snapshot read: {}", e))
        })?;

        #[derive(sqlx::FromRow)]
        struct ConversationRow {
            transcript_revision: i64,
        }
        let conversation = sqlx::query_as::<_, ConversationRow>(
            "SELECT transcript_revision FROM conversations WHERE id = ?",
        )
        .bind(conversation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read transcript revision: {}", e)))?
        .ok_or_else(|| {
            AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let state = Self::load_memory_state_tx(&mut tx, conversation_id).await?;

        // Count before materializing. A conversation that somehow accumulated
        // more active items than the host will work with is an explicit
        // overflow, not a silent `LIMIT` that drops constraints off the end.
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_memory_items \
             WHERE conversation_id = ? AND state = 'active'",
        )
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count memory items: {}", e)))?;
        if active_count as usize > MAX_ACTIVE_ITEMS {
            return Err(AppError::InvalidState(format!(
                "Conversation {} has {} active memory items, above the {} the host will load; \
                 a rebuild or review is required",
                conversation_id, active_count, MAX_ACTIVE_ITEMS
            )));
        }

        let item_rows = sqlx::query_as::<_, MemoryItemRow>(
            "SELECT id, conversation_id, kind, state, label, created_at_sequence, \
                    changed_at_sequence, superseded_by, revision, review, related_item_ids, \
                    created_at, updated_at \
             FROM conversation_memory_items \
             WHERE conversation_id = ? AND state = 'active' \
             ORDER BY created_at_sequence ASC, id ASC",
        )
        .bind(conversation_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load memory items: {}", e)))?;

        let evidence_rows = sqlx::query_as::<_, EvidenceRow>(
            "SELECT e.item_id, e.message_id, e.sequence, e.role, e.start_byte, e.end_byte, \
                    e.content_digest, e.purpose \
             FROM conversation_memory_evidence e \
             JOIN conversation_memory_items i ON i.id = e.item_id \
             WHERE i.conversation_id = ? AND i.state = 'active' \
             ORDER BY e.item_id ASC, e.ordinal ASC",
        )
        .bind(conversation_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load memory evidence: {}", e)))?;

        // The summary's boundary comes back with it, in this same read: the
        // compaction job compares it with the watermark to find source the
        // ledger covers but no summary does. Sequence zero is the placeholder of
        // paths that predate memory, not a position, so it reads as unknown.
        #[derive(sqlx::FromRow)]
        struct SummaryRow {
            summary_text: String,
            through_sequence: Option<i64>,
        }
        let summary = sqlx::query_as::<_, SummaryRow>(
            "SELECT s.summary_text, NULLIF(m.sequence, 0) AS through_sequence \
             FROM conversation_summaries s \
             LEFT JOIN conversation_messages m \
               ON m.id = s.up_to_message_id AND m.conversation_id = s.conversation_id \
             WHERE s.conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load working summary: {}", e)))?;

        let latest_sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM conversation_messages WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read latest sequence: {}", e)))?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to finish memory snapshot read: {}", e))
        })?;

        Ok(MemorySnapshot {
            state,
            transcript_revision: conversation.transcript_revision,
            active_items: assemble_items(item_rows, evidence_rows)?,
            summary_through_sequence: summary.as_ref().and_then(|row| row.through_sequence),
            summary: summary.map(|row| row.summary_text),
            latest_sequence,
        })
    }

    /// State row, or the empty state when nothing has been extracted yet.
    pub async fn load_memory_state(
        &self,
        conversation_id: &str,
    ) -> Result<ConversationMemoryState> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin state read: {}", e)))?;
        let state = Self::load_memory_state_tx(&mut tx, conversation_id).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to finish state read: {}", e)))?;
        Ok(state)
    }

    async fn load_memory_state_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
    ) -> Result<ConversationMemoryState> {
        let row = sqlx::query_as::<_, MemoryStateRow>(
            "SELECT conversation_id, schema_version, memory_revision, source_transcript_revision, \
                    processed_through_sequence, validity, last_error_code, \
                    extractor_model_identity, extractor_prompt_version, validator_version, \
                    updated_at \
             FROM conversation_memory_state WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load memory state: {}", e)))?;

        match row {
            Some(row) => row.into_domain(),
            None => Ok(ConversationMemoryState::empty(conversation_id)),
        }
    }

    /// Inactive items, newest change first: the correction history behind the
    /// active ledger.
    pub async fn page_inactive_memory_items(
        &self,
        conversation_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        let limit = limit.clamp(1, MAX_ACTIVE_ITEMS as i64);
        let item_rows = sqlx::query_as::<_, MemoryItemRow>(
            "SELECT id, conversation_id, kind, state, label, created_at_sequence, \
                    changed_at_sequence, superseded_by, revision, review, related_item_ids, \
                    created_at, updated_at \
             FROM conversation_memory_items \
             WHERE conversation_id = ? AND state != 'active' \
             ORDER BY changed_at_sequence DESC, id ASC LIMIT ? OFFSET ?",
        )
        .bind(conversation_id)
        .bind(limit)
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to page inactive items: {}", e)))?;

        if item_rows.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; item_rows.len()].join(",");
        let sql = format!(
            "SELECT item_id, message_id, sequence, role, start_byte, end_byte, content_digest, \
                    purpose FROM conversation_memory_evidence \
             WHERE item_id IN ({placeholders}) ORDER BY item_id ASC, ordinal ASC"
        );
        let mut query = sqlx::query_as::<_, EvidenceRow>(&sql);
        for row in &item_rows {
            query = query.bind(&row.id);
        }
        let evidence_rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to load item evidence: {}", e)))?;

        assemble_items(item_rows, evidence_rows)
    }
}
