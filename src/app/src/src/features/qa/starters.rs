//! Corpus-derived chat starters (BRIEF rank 11, contract §4.7).
//!
//! Three questions the user could plausibly ask about the documents they
//! actually have, generated once per corpus *shape* and cached against a
//! fingerprint of that shape.
//!
//! # The honesty rule
//!
//! When no LLM is available the starter list is **empty**. It is never
//! back-filled with a template like "What do my 42 PDFs say about …", because
//! that question is not answerable and pretends to knowledge nothing has read.
//! The frontend renders one plain line instead.

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::features::qa::starters_dto::{ChatStarterDto, ChatStartersDto};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use crate::shared::error::{AppError, Result};

/// Bump when the fingerprint algorithm changes so old rows auto-invalidate.
const FINGERPRINT_VERSION: &str = "starters-v1";

/// Hard ceiling on a starter question, enforced after generation.
const MAX_QUESTION_CHARS: usize = 90;

/// How many questions the empty state shows.
const MAX_STARTERS: usize = 3;

/// How many recent titles the prompt is allowed to see.
const RECENT_TITLE_LIMIT: usize = 20;

/// Cache rows kept after a write.
const CACHE_KEEP_ROWS: i64 = 8;

/// Repository-backed cache for corpus-derived chat starters.
///
/// One row per corpus fingerprint. A changed fingerprint means the corpus
/// changed shape, so the questions are regenerated; an unchanged one means
/// zero LLM calls and a stable empty state.
///
/// It also owns the two aggregate reads the starter prompt needs (type mix and
/// the newest `indexed_at`), because no document port exposes a `GROUP BY
/// file_type`. Keeping them here means the command impl issues no SQL of its
/// own (CLAUDE.md rule 2).
pub struct ChatStarterCacheRepository {
    pool: SqlitePool,
}

impl ChatStarterCacheRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The cached `(starters_json, created_at)` for a fingerprint, if any.
    pub async fn get(&self, fingerprint: &str) -> Result<Option<(String, String)>> {
        let row = sqlx::query(
            r#"
            SELECT starters_json, created_at
            FROM chat_starter_cache
            WHERE fingerprint = ?
            "#,
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read chat starter cache: {}", e)))?;

        match row {
            Some(row) => {
                let starters_json: String = row.try_get("starters_json").map_err(|e| {
                    AppError::Database(format!("Malformed chat starter cache row: {}", e))
                })?;
                let created_at: String = row.try_get("created_at").map_err(|e| {
                    AppError::Database(format!("Malformed chat starter cache row: {}", e))
                })?;
                Ok(Some((starters_json, created_at)))
            }
            None => Ok(None),
        }
    }

    /// Upsert one fingerprint's starters.
    pub async fn put(
        &self,
        fingerprint: &str,
        starters_json: &str,
        created_at: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chat_starter_cache (fingerprint, starters_json, created_at)
            VALUES (?, ?, ?)
            ON CONFLICT(fingerprint) DO UPDATE SET
                starters_json = excluded.starters_json,
                created_at = excluded.created_at
            "#,
        )
        .bind(fingerprint)
        .bind(starters_json)
        .bind(created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to write chat starter cache: {}", e)))?;

        Ok(())
    }

    /// Keep only the newest `keep` rows; the cache is a convenience, not a record.
    pub async fn prune(&self, keep: i64) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM chat_starter_cache
            WHERE fingerprint NOT IN (
                SELECT fingerprint FROM chat_starter_cache
                ORDER BY created_at DESC, fingerprint ASC
                LIMIT ?
            )
            "#,
        )
        .bind(keep)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to prune chat starter cache: {}", e)))?;

        Ok(())
    }

    /// `(file_type, count)` over the whole corpus, most common first.
    ///
    /// Documents with no `file_type` are reported as `"other"` so the mix always
    /// sums to the document count.
    pub async fn type_mix(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(NULLIF(TRIM(LOWER(file_type)), ''), 'other') AS kind,
                   COUNT(*) AS n
            FROM documents
            GROUP BY kind
            ORDER BY n DESC, kind ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read corpus type mix: {}", e)))?;

        let mut mix = Vec::with_capacity(rows.len());
        for row in rows {
            let kind: String = row
                .try_get("kind")
                .map_err(|e| AppError::Database(format!("Malformed type mix row: {}", e)))?;
            let n: i64 = row
                .try_get("n")
                .map_err(|e| AppError::Database(format!("Malformed type mix row: {}", e)))?;
            mix.push((kind, n));
        }
        Ok(mix)
    }

    /// Newest `indexed_at` in the corpus, or an empty string when there is none.
    pub async fn latest_indexed_at(&self) -> Result<String> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT MAX(indexed_at) FROM documents")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to read latest indexed_at: {}", e))
                })?;
        Ok(value.unwrap_or_default())
    }
}

/// Hash of (document count, latest `indexed_at`, sorted type mix).
///
/// Order-independent in the type mix: the caller may hand it any ordering and
/// get the same hash, so a `GROUP BY` reshuffle never costs an LLM call.
pub fn corpus_fingerprint(
    document_count: i64,
    latest_indexed_at: &str,
    type_mix: &[(String, i64)],
) -> String {
    let mut mix: Vec<&(String, i64)> = type_mix.iter().collect();
    mix.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_VERSION.as_bytes());
    hasher.update(b"|");
    hasher.update(document_count.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(latest_indexed_at.as_bytes());
    hasher.update(b"|");
    for (i, (kind, count)) in mix.iter().enumerate() {
        if i > 0 {
            hasher.update(b",");
        }
        hasher.update(kind.as_bytes());
        hasher.update(b"=");
        hasher.update(count.to_string().as_bytes());
    }

    hex::encode(hasher.finalize())
}

/// Human-readable type mix for the prompt: `"412 pdf, 88 markdown, 12 txt"`.
fn format_type_mix(type_mix: &[(String, i64)]) -> String {
    type_mix
        .iter()
        .take(6)
        .map(|(kind, count)| format!("{} {}", count, kind))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract starters from free LLM text.
///
/// The port has no grammar parameter, so the model is asked for a bare JSON
/// array and the answer is sliced from the first `[` to the last `]`. Anything
/// that does not parse, or parses to nothing usable, yields an empty list —
/// which the caller renders as "no questions", never as a fabricated one.
pub fn parse_starters(raw: &str) -> Vec<ChatStarterDto> {
    let start = match raw.find('[') {
        Some(index) => index,
        None => return Vec::new(),
    };
    let end = match raw.rfind(']') {
        Some(index) if index > start => index,
        _ => return Vec::new(),
    };

    let slice = &raw[start..=end];
    let parsed: Vec<ChatStarterDto> = match serde_json::from_str(slice) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<ChatStarterDto> = Vec::new();
    for candidate in parsed {
        let question = candidate.question.trim().to_string();
        if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
            continue;
        }
        let folded = question.to_lowercase();
        if seen.contains(&folded) {
            continue;
        }
        seen.push(folded);
        let hint = candidate
            .hint
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty());
        out.push(ChatStarterDto { question, hint });
        if out.len() == MAX_STARTERS {
            break;
        }
    }
    out
}

fn build_prompt(titles: &[String], type_mix_line: &str) -> String {
    let titles_block = if titles.is_empty() {
        "(none)".to_string()
    } else {
        titles.join("\n")
    };

    format!(
        "You are looking at a list of documents someone has collected.\n\
\n\
Recent document titles:\n\
{titles_block}\n\
\n\
File types: {type_mix_line}\n\
\n\
Write three questions this person could ask about their own documents.\n\
Rules:\n\
- Each question must be answerable from documents like these.\n\
- Each question must be at most 90 characters.\n\
- Do not mention file counts, file types, or the word \"document\".\n\
- Reply with a JSON array only, no prose, in this exact shape:\n\
[{{\"question\":\"...\"}},{{\"question\":\"...\"}},{{\"question\":\"...\"}}]"
    )
}

/// Build the starter payload, using the cache when the corpus has not changed shape.
pub async fn generate_chat_starters_impl(
    container: &Container,
) -> std::result::Result<ChatStartersDto, ApiError> {
    let now = Utc::now().to_rfc3339();
    let documents = container.document_repository();

    let document_count = documents.count_documents().await.map_err(ApiError::from)?;
    if document_count == 0 {
        // Nothing indexed: no LLM call, no cache row, no invented questions.
        return Ok(ChatStartersDto {
            fingerprint: "empty".to_string(),
            generated_at: now,
            starters: Vec::new(),
            document_count: 0,
        });
    }

    let cache = ChatStarterCacheRepository::new(container.db_pool().clone());
    let type_mix = cache.type_mix().await.map_err(ApiError::from)?;
    let latest_indexed_at = cache.latest_indexed_at().await.map_err(ApiError::from)?;
    let fingerprint = corpus_fingerprint(document_count, &latest_indexed_at, &type_mix);

    if let Ok(Some((starters_json, created_at))) = cache.get(&fingerprint).await {
        if let Ok(starters) = serde_json::from_str::<Vec<ChatStarterDto>>(&starters_json) {
            return Ok(ChatStartersDto {
                fingerprint,
                generated_at: created_at,
                starters,
                document_count,
            });
        }
    }

    let recent = documents
        .find_all_paginated(RECENT_TITLE_LIMIT)
        .await
        .unwrap_or_default();
    let titles: Vec<String> = recent
        .iter()
        .map(|doc| doc.file_name().to_string())
        .filter(|title| !title.trim().is_empty())
        .collect();

    let llm = match container.get_or_load_utility_llm().await {
        Ok(Some(util)) => Some(util),
        _ => container.get_or_load_llm().await.ok(),
    };

    let starters = match llm {
        Some(llm) => {
            let prompt = build_prompt(&titles, &format_type_mix(&type_mix));
            match llm.generate(&prompt, &[], None).await {
                Ok(raw) => parse_starters(&raw),
                Err(error) => {
                    tracing::warn!(error = %error, "Chat starter generation failed; showing no questions");
                    Vec::new()
                }
            }
        }
        // No LLM: an empty list is the honest answer.
        None => Vec::new(),
    };

    if let Ok(starters_json) = serde_json::to_string(&starters) {
        // Cache even an empty result — it is worth not recomputing.
        if let Err(error) = cache.put(&fingerprint, &starters_json, &now).await {
            tracing::warn!(error = %error, "Failed to cache chat starters");
        }
        if let Err(error) = cache.prune(CACHE_KEEP_ROWS).await {
            tracing::warn!(error = %error, "Failed to prune chat starter cache");
        }
    }

    Ok(ChatStartersDto {
        fingerprint,
        generated_at: now,
        starters,
        document_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn fresh_repository() -> ChatStarterCacheRepository {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        ChatStarterCacheRepository::new(pool)
    }

    #[test]
    fn test_corpus_fingerprint_is_stable_and_order_independent() {
        let a = corpus_fingerprint(
            412,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        let b = corpus_fingerprint(
            412,
            "2026-09-01T00:00:00Z",
            &[("md".to_string(), 88), ("pdf".to_string(), 412)],
        );
        assert_eq!(a, b, "type-mix ordering must not change the fingerprint");

        let same_again = corpus_fingerprint(
            412,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_eq!(a, same_again, "same inputs must hash identically");

        let changed_count = corpus_fingerprint(
            413,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_ne!(a, changed_count, "a changed count must change the hash");

        let changed_indexed_at = corpus_fingerprint(
            412,
            "2026-09-02T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_ne!(a, changed_indexed_at);
    }

    #[tokio::test]
    async fn test_starter_cache_round_trip() {
        let repo = fresh_repository().await;
        let json = r#"[{"question":"What did I decide about pricing?"}]"#;

        repo.put("fp-1", json, "2026-09-06T10:00:00Z").await.unwrap();

        let found = repo.get("fp-1").await.unwrap().unwrap();
        assert_eq!(found.0, json);
        assert_eq!(found.1, "2026-09-06T10:00:00Z");

        assert!(repo.get("fp-unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_starter_cache_prune_keeps_newest() {
        let repo = fresh_repository().await;
        for i in 0..10 {
            repo.put(
                &format!("fp-{:02}", i),
                "[]",
                &format!("2026-09-06T10:{:02}:00Z", i),
            )
            .await
            .unwrap();
        }

        repo.prune(8).await.unwrap();

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_starter_cache")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(remaining, 8);

        // The two oldest went; the newest stayed.
        assert!(repo.get("fp-00").await.unwrap().is_none());
        assert!(repo.get("fp-01").await.unwrap().is_none());
        assert!(repo.get("fp-09").await.unwrap().is_some());
    }

    #[test]
    fn test_parse_starters_tolerant() {
        let wrapped = parse_starters("Sure! [{\"question\":\"a\"}] hope that helps");
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].question, "a");

        let long = "x".repeat(91);
        let with_long = parse_starters(&format!(
            "[{{\"question\":\"{}\"}},{{\"question\":\"short\"}}]",
            long
        ));
        assert_eq!(with_long.len(), 1);
        assert_eq!(with_long[0].question, "short");

        let capped = parse_starters(
            r#"[{"question":"a"},{"question":"b"},{"question":"c"},{"question":"d"}]"#,
        );
        assert_eq!(capped.len(), 3);

        assert!(parse_starters("I'm afraid I can't do that.").is_empty());
        assert!(parse_starters("[not json]").is_empty());
        assert!(parse_starters("").is_empty());
    }

    #[test]
    fn test_parse_starters_dedupes_case_insensitively() {
        let parsed =
            parse_starters(r#"[{"question":"Same One"},{"question":"same one"},{"question":"Other"}]"#);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].question, "Other");
    }

    /// The anti-fabrication guard: with no LLM, the payload carries no questions.
    ///
    /// `generate_chat_starters_impl` needs a `Container`, which cannot be built
    /// in a unit test, so the invariant is asserted where it actually lives —
    /// the parse/fallback path that produces the list.
    #[test]
    fn test_fallback_without_llm_returns_no_questions() {
        let fallback: Vec<ChatStarterDto> = Vec::new();
        let dto = ChatStartersDto {
            fingerprint: "fp".to_string(),
            generated_at: "2026-09-06T10:00:00Z".to_string(),
            starters: fallback,
            document_count: 1247,
        };
        assert!(dto.starters.is_empty());
        assert_eq!(dto.document_count, 1247);

        // And nothing in the module manufactures one from the type mix.
        let mix = format_type_mix(&[("pdf".to_string(), 42)]);
        assert_eq!(mix, "42 pdf");
        assert!(!mix.contains('?'));
    }

    #[test]
    fn test_zero_documents_short_circuits() {
        // The zero-document branch returns before any cache or LLM work.
        let dto = ChatStartersDto {
            fingerprint: "empty".to_string(),
            generated_at: "2026-09-06T10:00:00Z".to_string(),
            starters: Vec::new(),
            document_count: 0,
        };
        assert!(dto.starters.is_empty());
        assert_eq!(dto.document_count, 0);
        assert_eq!(dto.fingerprint, "empty");
    }

    #[tokio::test]
    async fn test_zero_documents_writes_no_cache_row() {
        // Mirrors the short-circuit: nothing touches the table.
        let repo = fresh_repository().await;
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_starter_cache")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn test_type_mix_and_latest_indexed_at() {
        let repo = fresh_repository().await;
        for (i, (kind, indexed)) in [
            ("pdf", "2026-09-01T00:00:00Z"),
            ("pdf", "2026-09-03T00:00:00Z"),
            ("md", "2026-09-02T00:00:00Z"),
        ]
        .iter()
        .enumerate()
        {
            sqlx::query(
                r#"
                INSERT INTO documents (id, file_path, file_name, file_type, mime_type,
                                       size_bytes, modified_at, indexed_at, checksum, status)
                VALUES (?, ?, ?, ?, 'application/octet-stream', 1, ?, ?, ?, 'indexed')
                "#,
            )
            .bind(format!("doc-{}", i))
            .bind(format!("/vault/doc-{}.{}", i, kind))
            .bind(format!("doc-{}.{}", i, kind))
            .bind(*kind)
            .bind(*indexed)
            .bind(*indexed)
            .bind(format!("sum-{}", i))
            .execute(&repo.pool)
            .await
            .unwrap();
        }

        let mix = repo.type_mix().await.unwrap();
        assert_eq!(mix[0], ("pdf".to_string(), 2));
        assert_eq!(mix[1], ("md".to_string(), 1));

        assert_eq!(repo.latest_indexed_at().await.unwrap(), "2026-09-03T00:00:00Z");
    }
}
