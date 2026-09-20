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
use crate::application::ports::conversation_memory::{
    CommittedMemorySnapshot, MemoryCommitCandidate, MemoryCommitError, MemoryCommitPreconditions,
    ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
};
use crate::domain::conversation_memory::{
    compute_digest, ConversationMemoryState, EvidencePurpose, EvidenceSpan, MemoryId, MemoryItem,
    MemoryKind, MemoryReview, MemorySnapshot, MemoryState, SourceMessage, SourceRole,
    MAX_ACTIVE_ITEMS, MAX_RELATED_ITEMS, MEMORY_SCHEMA_VERSION,
};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use std::str::FromStr;

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
        Ok(SourceMessage {
            id: self.id,
            conversation_id: self.conversation_id,
            sequence: self.sequence,
            role: SourceRole::from_str(&self.role)?,
            content: self.content,
            content_digest: digest,
            status: self.status,
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

const SOURCE_COLUMNS: &str = "id, conversation_id, sequence, role, content, content_digest, status";

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

        let summary: Option<String> = sqlx::query_scalar(
            "SELECT summary_text FROM conversation_summaries WHERE conversation_id = ?",
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
            summary,
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

    /// Original messages in `(after_sequence, through_sequence]`, oldest first.
    pub async fn page_memory_source_messages(
        &self,
        conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        if through_sequence <= after_sequence {
            return Ok(SourcePage {
                messages: Vec::new(),
                has_more: false,
                next_after_sequence: after_sequence,
            });
        }
        // One row more than asked for, so `has_more` is an observation rather
        // than a guess from a full page.
        let fetch = limits.max_messages.saturating_add(1).min(4096) as i64;
        let rows = sqlx::query_as::<_, SourceMessageRow>(&format!(
            "SELECT {SOURCE_COLUMNS} FROM conversation_messages \
             WHERE conversation_id = ? AND sequence > ? AND sequence <= ? \
             ORDER BY sequence ASC LIMIT ?"
        ))
        .bind(conversation_id)
        .bind(after_sequence)
        .bind(through_sequence)
        .bind(fetch)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to page source messages: {}", e)))?;

        Self::assemble_page(rows, after_sequence, through_sequence, limits)
    }

    /// Apply message and byte limits to a fetched run of rows.
    ///
    /// A message limit alone does not bound anything: one pasted log file can be
    /// larger than the whole model input. At least one message is always
    /// returned when any exist, so a single oversized message produces a
    /// page the caller can reason about rather than an empty page forever.
    fn assemble_page(
        rows: Vec<SourceMessageRow>,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        let mut messages = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        for row in rows {
            if messages.len() >= limits.max_messages {
                truncated = true;
                break;
            }
            let size = row.content.len();
            if !messages.is_empty() && bytes.saturating_add(size) > limits.max_bytes {
                truncated = true;
                break;
            }
            bytes = bytes.saturating_add(size);
            messages.push(row.into_domain()?);
        }
        let next_after_sequence = messages
            .last()
            .map(|message| message.sequence)
            .unwrap_or(after_sequence);
        Ok(SourcePage {
            has_more: truncated || next_after_sequence < through_sequence,
            next_after_sequence,
            messages,
        })
    }

    /// Resolve stored spans back to source text.
    pub async fn read_memory_source_spans(
        &self,
        conversation_id: &str,
        spans: &[SourceSpanRef],
        limits: SourceReadLimits,
    ) -> Result<Vec<ResolvedSpan>> {
        if spans.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = spans
            .iter()
            .map(|span| span.message_id.clone())
            .take(limits.max_messages)
            .collect();
        let messages = self
            .read_memory_messages(conversation_id, &ids, limits)
            .await?;
        let by_id: std::collections::HashMap<&str, &SourceMessage> = messages
            .iter()
            .map(|message| (message.id.as_str(), message))
            .collect();

        Ok(spans
            .iter()
            .map(|span| {
                let message = by_id.get(span.message_id.as_str());
                // Resolved through the domain rule, digest check included. A
                // span whose source moved yields `None`; there is deliberately
                // no cached copy of the text to fall back to.
                let text = message.and_then(|message| {
                    let probe = EvidenceSpan {
                        message_id: message.id.clone(),
                        sequence: message.sequence,
                        role: message.role,
                        start_byte: span.start_byte,
                        end_byte: span.end_byte,
                        content_digest: message.content_digest.clone(),
                        purpose: EvidencePurpose::Assertion,
                    };
                    probe.resolve(&message.content).map(str::to_owned)
                });
                ResolvedSpan {
                    message_id: span.message_id.clone(),
                    sequence: message.map(|message| message.sequence).unwrap_or(0),
                    role: message
                        .map(|message| message.role)
                        .unwrap_or(SourceRole::User),
                    text,
                }
            })
            .collect())
    }

    /// Read specific messages, scoped to one conversation. Foreign ids are
    /// dropped by the `WHERE`, never returned.
    pub async fn read_memory_messages(
        &self,
        conversation_id: &str,
        message_ids: &[String],
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: Vec<&String> = message_ids.iter().take(limits.max_messages).collect();
        let placeholders = vec!["?"; wanted.len()].join(",");
        let sql = format!(
            "SELECT {SOURCE_COLUMNS} FROM conversation_messages \
             WHERE conversation_id = ? AND id IN ({placeholders}) ORDER BY sequence ASC"
        );
        let mut query = sqlx::query_as::<_, SourceMessageRow>(&sql).bind(conversation_id);
        for id in wanted {
            query = query.bind(id);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to read source messages: {}", e)))?;

        let mut messages = Vec::new();
        let mut bytes = 0usize;
        for row in rows {
            let size = row.content.len();
            if !messages.is_empty() && bytes.saturating_add(size) > limits.max_bytes {
                break;
            }
            bytes = bytes.saturating_add(size);
            messages.push(row.into_domain()?);
        }
        Ok(messages)
    }

    /// Messages around `sequence`, for explaining a short reply in context.
    pub async fn read_memory_adjacent_turns(
        &self,
        conversation_id: &str,
        sequence: i64,
        before: usize,
        after: usize,
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>> {
        let from = sequence.saturating_sub(before as i64);
        let to = sequence.saturating_add(after as i64);
        let page = self
            .read_memory_sequence_range(conversation_id, from, to, limits)
            .await?;
        Ok(page.messages)
    }

    /// A contiguous sequence interval, inclusive at both ends.
    pub async fn read_memory_sequence_range(
        &self,
        conversation_id: &str,
        from_sequence: i64,
        to_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        if to_sequence < from_sequence {
            return Ok(SourcePage {
                messages: Vec::new(),
                has_more: false,
                next_after_sequence: from_sequence,
            });
        }
        let fetch = limits.max_messages.saturating_add(1).min(4096) as i64;
        let rows = sqlx::query_as::<_, SourceMessageRow>(&format!(
            "SELECT {SOURCE_COLUMNS} FROM conversation_messages \
             WHERE conversation_id = ? AND sequence >= ? AND sequence <= ? \
             ORDER BY sequence ASC LIMIT ?"
        ))
        .bind(conversation_id)
        .bind(from_sequence)
        .bind(to_sequence)
        .bind(fetch)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read sequence range: {}", e)))?;

        Self::assemble_page(rows, from_sequence.saturating_sub(1), to_sequence, limits)
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

        let mut tx = self.pool.begin().await.map_err(db)?;

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
