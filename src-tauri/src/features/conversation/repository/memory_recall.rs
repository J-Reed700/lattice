//! Recall over one conversation's original transcript.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §9.
//!
//! Reuses the existing `conversation_search_fts` index rather than borrowing
//! document-search permissions or treating the current space as equivalent to
//! this conversation. Three rules shape everything here:
//!
//! * **Ownership is in the `WHERE`, always.** Every query joins back to
//!   `conversation_messages` filtered by `conversation_id`. Another thread can
//!   contain a byte-identical quotation and it will never be returned.
//! * **`source = 'message'` only.** The same index holds conversation titles and
//!   bookmark notes. Those are not things the user said in this transcript, and
//!   returning one as message evidence would attribute a note to the user.
//! * **OR, not AND.** The explorer's all-terms helper is right for finding a
//!   conversation and wrong for finding a passage inside one: a recalled
//!   question rarely repeats every word of the message that answers it.
//!
//! What this is not: semantic retrieval. `conversation_search_fts` is tokenized
//! `unicode61 remove_diacritics 2`, so it matches words, not meanings, and it
//! does not segment CJK. Paraphrase recall is reported as a measured number by
//! the evaluation suite, never claimed here.

use super::ConversationRepository;
use crate::application::ports::conversation_memory::{RecallCandidate, RecallCandidates};
use crate::domain::conversation_memory::{MemoryId, SourceRole};
use crate::shared::error::{AppError, Result};
use std::str::FromStr;

/// Cap on terms in one `MATCH` expression (§9.1 rule 1).
const MAX_FTS_TERMS: usize = 16;
/// Cap on the serialized `MATCH` expression, in UTF-8 bytes.
const MAX_FTS_QUERY_BYTES: usize = 1024;
/// Shorter terms carry almost no signal and widen the OR query for nothing.
const MIN_TERM_CHARS: usize = 3;
/// Exact identifiers to probe, kept separate from the lexical query so
/// truncating that query can never discard one.
const MAX_EXACT_TERMS: usize = 8;
/// Characters allowed to open an excerpt, bounding what a candidate row costs.
const EXCERPT_CHARS: usize = 600;

/// An FTS5 string literal: the one form no user input can escape from.
fn quote_fts(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Build an OR `MATCH` expression from free text, or `None` when nothing in the
/// input is searchable and the lexical branch should simply be skipped.
///
/// Never interpolated into SQL: the result is bound as a parameter, so FTS5
/// operators inside user text stay inside a quoted literal.
pub(super) fn build_recall_match(raw: &str) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    let mut terms: Vec<String> = Vec::new();
    let mut bytes = 0usize;
    for term in raw.split(|c: char| c.is_whitespace()) {
        let term = term.trim_matches(|c: char| !c.is_alphanumeric());
        if term.is_empty() || !term.chars().any(char::is_alphanumeric) {
            continue;
        }
        // Kept for having any alphanumeric character in any script: an
        // ASCII-letter rule silently empties the query for `4012`, `429`, and
        // for anything not written in Latin script.
        let is_short = term.chars().count() < MIN_TERM_CHARS;
        let is_numeric = term.chars().all(|c| c.is_ascii_digit());
        if is_short && !is_numeric {
            continue;
        }
        let lowered = term.to_lowercase();
        if !seen.insert(lowered) {
            continue;
        }
        let quoted = quote_fts(term);
        // ` OR ` between terms, counted so the cap is on what is actually sent.
        let added = quoted.len() + if terms.is_empty() { 0 } else { 4 };
        if bytes + added > MAX_FTS_QUERY_BYTES || terms.len() >= MAX_FTS_TERMS {
            break;
        }
        bytes += added;
        terms.push(quoted);
    }
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

/// Trim an excerpt to a bounded number of characters on a char boundary.
fn excerpt(content: &str) -> String {
    match content.char_indices().nth(EXCERPT_CHARS) {
        Some((cut, _)) => content[..cut].to_string(),
        None => content.to_string(),
    }
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: String,
    sequence: i64,
    role: String,
    content: String,
    score: f64,
}

impl ConversationRepository {
    /// Lexical and exact-identifier candidates for one conversation.
    ///
    /// Reports which modes ran. "No hits" and "the index was unavailable" are
    /// different answers, and neither of them means the user never said it.
    pub async fn search_memory_source_messages(
        &self,
        conversation_id: &str,
        query: &str,
        exact_terms: &[String],
        limit: usize,
    ) -> Result<RecallCandidates> {
        let limit = limit.clamp(1, 64);
        let mut out = RecallCandidates::default();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // --- exact identifiers first ------------------------------------
        // Paths, version numbers, codenames and ids are the cases where lexical
        // ranking is least reliable and an exact answer matters most, so these
        // take their places in the result before ranked candidates compete.
        for term in exact_terms.iter().take(MAX_EXACT_TERMS) {
            let term = term.trim();
            if term.is_empty() {
                continue;
            }
            // `instr` is an exact, case-sensitive substring test. `LIKE` would
            // fold ASCII case and would need its own wildcard escaping, which
            // is precisely the wrong behaviour for `AuthToken` vs `authtoken`.
            let rows = sqlx::query_as::<_, CandidateRow>(
                "SELECT id, sequence, role, content, 0.0 as score \
                 FROM conversation_messages \
                 WHERE conversation_id = ? AND instr(content, ?) > 0 \
                 ORDER BY sequence DESC LIMIT ?",
            )
            .bind(conversation_id)
            .bind(term)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to match exact term: {}", e)))?;

            for row in rows {
                if !seen.insert(row.id.clone()) {
                    continue;
                }
                out.candidates.push(RecallCandidate {
                    message_id: row.id,
                    sequence: row.sequence,
                    role: SourceRole::from_str(&row.role)?,
                    excerpt: excerpt(&row.content),
                    score: 0.0,
                    exact_identifier: true,
                });
            }
        }

        // --- ranked lexical candidates ----------------------------------
        let Some(match_expression) = build_recall_match(query) else {
            // Nothing searchable in the query. The exact hits above still stand.
            return Ok(out);
        };

        let rows = sqlx::query_as::<_, CandidateRow>(
            "SELECT m.id, m.sequence, m.role, m.content, bm25(conversation_search_fts) as score \
             FROM conversation_search_fts \
             JOIN conversation_messages m ON m.id = conversation_search_fts.message_id \
             WHERE conversation_search_fts.content MATCH ? \
               AND conversation_search_fts.source = 'message' \
               AND conversation_search_fts.conversation_id = ? \
               AND m.conversation_id = ? \
             ORDER BY score ASC LIMIT ?",
        )
        .bind(&match_expression)
        .bind(conversation_id)
        .bind(conversation_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await;

        let rows = match rows {
            Ok(rows) => {
                out.lexical_ran = true;
                rows
            }
            Err(error) => {
                // A `MATCH` that FTS5 rejects, or a missing index, degrades
                // recall. It must not fail the turn: the constraints and the
                // recent context are unaffected.
                out.index_error = Some(if error.to_string().contains("fts5") {
                    "fts_syntax".to_string()
                } else {
                    "index_unavailable".to_string()
                });
                Vec::new()
            }
        };

        for row in rows {
            if !seen.insert(row.id.clone()) {
                continue;
            }
            out.candidates.push(RecallCandidate {
                message_id: row.id,
                sequence: row.sequence,
                role: SourceRole::from_str(&row.role)?,
                excerpt: excerpt(&row.content),
                // bm25 returns a negative score where more negative is better.
                score: row.score as f32,
                exact_identifier: false,
            });
        }

        out.candidates.truncate(limit);
        Ok(out)
    }

    /// Generated item labels matching free text, for the details view.
    ///
    /// Labels help someone find a memory they half-remember. They are never a
    /// substitute for evidence, and no mandatory item depends on this search
    /// reaching it — mandatory items are loaded wholesale (§9.4).
    pub async fn search_memory_item_labels(
        &self,
        conversation_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryId>> {
        let needle = query.trim();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM conversation_memory_items \
             WHERE conversation_id = ? AND instr(lower(label), lower(?)) > 0 \
             ORDER BY state = 'active' DESC, changed_at_sequence DESC LIMIT ?",
        )
        .bind(conversation_id)
        .bind(needle)
        .bind(limit.clamp(1, 64) as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to search memory labels: {}", e)))?;

        Ok(ids
            .into_iter()
            .filter_map(|id| MemoryId::from_string(id).ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recall_query_is_or_joined_quoted_and_bounded() {
        let built = build_recall_match("deploy the staging environment").expect("terms");
        assert_eq!(
            built,
            "\"deploy\" OR \"the\" OR \"staging\" OR \"environment\""
        );

        // FTS5 operators inside user text stay inside the literal.
        let built = build_recall_match("rules - The Silo* NEAR").expect("terms");
        assert!(built.contains("\"rules\""));
        assert!(
            !built.contains('*'),
            "a wildcard escaped the literal: {built}"
        );
        // Edge punctuation is trimmed off a term, so a surrounding quote never
        // reaches the expression at all.
        assert_eq!(
            build_recall_match("say \"hello\" twice").as_deref(),
            Some("\"say\" OR \"hello\" OR \"twice\"")
        );
        // An *interior* quote survives trimming, and is doubled rather than
        // closing the literal early.
        assert_eq!(
            build_recall_match("path a\"b end").as_deref(),
            Some("\"path\" OR \"a\"\"b\" OR \"end\"")
        );

        // Numerics survive even though they are short; they are often the
        // whole point of the recall.
        assert_eq!(build_recall_match("429").as_deref(), Some("\"429\""));
        // Nothing searchable yields no lexical branch at all.
        assert_eq!(build_recall_match("  -- ,, "), None);
        assert_eq!(build_recall_match(""), None);
    }

    #[test]
    fn the_term_cap_applies_to_what_is_actually_sent() {
        let many = (0..40)
            .map(|i| format!("term{i:03}"))
            .collect::<Vec<_>>()
            .join(" ");
        let built = build_recall_match(&many).expect("terms");
        assert_eq!(built.matches(" OR ").count(), MAX_FTS_TERMS - 1);
        assert!(built.len() <= MAX_FTS_QUERY_BYTES);

        // One enormous term cannot push the expression past the byte cap.
        let huge = "x".repeat(MAX_FTS_QUERY_BYTES * 2);
        assert_eq!(build_recall_match(&huge), None);
    }

    #[test]
    fn repeated_terms_are_collapsed_case_insensitively() {
        assert_eq!(
            build_recall_match("Deploy deploy DEPLOY staging").as_deref(),
            Some("\"Deploy\" OR \"staging\"")
        );
    }

    #[test]
    fn an_excerpt_is_cut_on_a_character_boundary() {
        let text = "é".repeat(EXCERPT_CHARS + 50);
        let cut = excerpt(&text);
        assert_eq!(cut.chars().count(), EXCERPT_CHARS);
        assert!(text.starts_with(&cut));
        // Short content is returned whole.
        assert_eq!(excerpt("short"), "short");
    }
}
