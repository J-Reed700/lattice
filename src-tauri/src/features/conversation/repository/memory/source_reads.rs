//! Reads of the source messages and spans that memory items cite.

use super::super::ConversationRepository;
use super::{SourceMessageRow, SOURCE_COLUMNS};
use crate::application::ports::conversation_memory::{
    ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
};
use crate::domain::conversation::memory::{
    EvidencePurpose, EvidenceSpan, SourceMessage, SourceRole, SuppressedSpan,
};
use crate::shared::error::{AppError, Result};

impl ConversationRepository {
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

    /// The newest messages in `(after_sequence, through_sequence]`, read from
    /// the top down so a limit drops the oldest candidates, not the newest.
    /// Returned oldest first; `has_more` means older rows were left unread.
    pub async fn page_recent_memory_source_messages(
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
        let fetch = limits.max_messages.saturating_add(1).min(4096) as i64;
        let rows = sqlx::query_as::<_, SourceMessageRow>(&format!(
            "SELECT {SOURCE_COLUMNS} FROM conversation_messages \
             WHERE conversation_id = ? AND sequence > ? AND sequence <= ? \
             ORDER BY sequence DESC LIMIT ?"
        ))
        .bind(conversation_id)
        .bind(after_sequence)
        .bind(through_sequence)
        .bind(fetch)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to page recent source messages: {}", e)))?;

        // The same limits as an ascending page, applied from the newest row.
        // Only an observed extra row sets `has_more`, so an exhausted range
        // reports no unread source.
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
        messages.reverse();
        Ok(SourcePage {
            messages,
            has_more: truncated,
            next_after_sequence: after_sequence,
        })
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

    /// Forgotten spans in `message_ids`, in one query however many there are.
    ///
    /// Scoped through the message's conversation, since a suppression row
    /// names only a message. Returned as stored: whether one still applies is
    /// the redaction's digest check, not this read's.
    pub async fn memory_suppressed_spans(
        &self,
        conversation_id: &str,
        message_ids: &[String],
    ) -> Result<Vec<SuppressedSpan>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: String,
            start_byte: i64,
            end_byte: i64,
            content_digest: String,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT s.message_id, s.start_byte, s.end_byte, s.content_digest \
             FROM conversation_memory_suppressions s \
             JOIN conversation_messages m ON m.id = s.message_id \
             WHERE m.conversation_id = ? \
               AND s.message_id IN (SELECT value FROM json_each(?)) \
             ORDER BY s.message_id, s.start_byte",
        )
        .bind(conversation_id)
        .bind(serde_json::to_string(message_ids)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read memory suppressions: {}", e)))?;
        Ok(rows
            .into_iter()
            .map(|row| SuppressedSpan {
                message_id: row.message_id,
                start_byte: row.start_byte,
                end_byte: row.end_byte,
                content_digest: row.content_digest,
            })
            .collect())
    }

    /// Labels of the conversation's forgotten items, distinct, most recently
    /// changed first, at most `limit`.
    pub async fn memory_forgotten_labels(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<String>> {
        sqlx::query_scalar(
            "SELECT i.label \
             FROM conversation_memory_items i \
             JOIN conversation_memory_attributes a ON a.item_id = i.id \
             WHERE i.conversation_id = ? AND a.forgotten = 1 \
             GROUP BY i.label \
             ORDER BY MAX(i.updated_at) DESC, i.label \
             LIMIT ?",
        )
        .bind(conversation_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read forgotten memory: {}", e)))
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
}
