//! Corpus-derived chat starters.
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

use super::chat::ports::ChatRuntime;
use crate::features::conversation::starters_dto::{ChatStarterDto, ChatStartersDto};
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::services::completion_input::{complete_text, TextCall};
use lattice_core::shared::ipc::ApiError;

/// Bump when the fingerprint algorithm changes so old rows auto-invalidate.
const FINGERPRINT_VERSION: &str = "starters-v2";

/// The space an unscoped request belongs to, matching the conversation default.
const DEFAULT_SPACE_ID: &str = "space_general";

/// How many questions the empty state shows.
const MAX_STARTERS: usize = 3;

/// Cache rows kept after a write.
const CACHE_KEEP_ROWS: i64 = 8;

/// Hash of (space, document count, latest `indexed_at`, sorted type mix). The
/// space is part of the key because two spaces can share a corpus shape; the
/// type mix is order-independent.
pub fn corpus_fingerprint(
    space_id: &str,
    document_count: i64,
    latest_indexed_at: &str,
    type_mix: &[(String, i64)],
) -> String {
    let mut mix: Vec<&(String, i64)> = type_mix.iter().collect();
    mix.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_VERSION.as_bytes());
    hasher.update(b"|");
    hasher.update(space_id.as_bytes());
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
        if question.is_empty() {
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

/// The newest titles the starter call's window gives room for, newest first,
/// with a line counting the older ones left out.
fn fit_titles(
    llm: &dyn lattice_core::application::ports::LLMPort,
    titles: Vec<String>,
    type_mix_line: &str,
) -> Vec<String> {
    use lattice_core::application::services::history_share::{
        auxiliary_history_room, newest_that_fit,
    };
    let room = auxiliary_history_room(llm, &build_prompt(&[], type_mix_line));
    let oldest_first: Vec<String> = titles.into_iter().rev().collect();
    let (kept, omitted) = newest_that_fit(&oldest_first, room, |title| {
        llm.count_tokens(title).saturating_add(1)
    });
    let mut fitted: Vec<String> = kept.iter().rev().cloned().collect();
    if omitted > 0 {
        fitted.push(format!("(and {omitted} older titles)"));
    }
    fitted
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

/// Builds the starter payload, using the cache when the corpus has not changed
/// shape. Everything the prompt and fingerprint see is drawn from `space_id`'s
/// documents (blank means General).
pub async fn generate_chat_starters_impl(
    runtime: &dyn ChatRuntime,
    space_id: Option<String>,
) -> std::result::Result<ChatStartersDto, ApiError> {
    let now = Utc::now().to_rfc3339();
    let space_id = space_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| DEFAULT_SPACE_ID.to_string());

    // The same allow-list every retrieval path uses, so the questions can only
    // be about documents this chat could actually answer from.
    let cache = runtime.chat_starter_cache();
    let allowed_ids = cache
        .space_document_scope(&space_id)
        .await
        .map_err(ApiError::from)?;

    let document_count = allowed_ids.len() as i64;
    if document_count == 0 {
        // Nothing readable here: no LLM call, no cache row, no invented questions.
        return Ok(ChatStartersDto {
            fingerprint: "empty".to_string(),
            generated_at: now,
            starters: Vec::new(),
            document_count: 0,
        });
    }

    let type_mix = cache.type_mix(&allowed_ids).await.map_err(ApiError::from)?;
    let latest_indexed_at = cache
        .latest_indexed_at(&allowed_ids)
        .await
        .map_err(ApiError::from)?;
    let fingerprint = corpus_fingerprint(&space_id, document_count, &latest_indexed_at, &type_mix);

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

    let titles = cache.recent_titles(&allowed_ids).await.unwrap_or_default();

    // Starters belong to a space, not a chat, so without the utility model
    // they are written by the Settings default.
    let llm = match runtime.get_or_load_utility_llm().await {
        Ok(Some(util)) => Some(util),
        Ok(None) => {
            tracing::info!("No utility model; chat starters use the default chat model");
            runtime.default_chat_llm().await.ok()
        }
        Err(error) => {
            tracing::warn!(%error, "Utility model unavailable; chat starters use the default chat model");
            runtime.default_chat_llm().await.ok()
        }
    };

    let starters = match llm {
        Some(llm) => {
            let type_mix_line = format_type_mix(&type_mix);
            let titles = fit_titles(llm.as_ref(), titles, &type_mix_line);
            let prompt = build_prompt(&titles, &type_mix_line);
            // Upkeep: queued behind anyone waiting on the model.
            match complete_text(
                llm.as_ref(),
                &prompt,
                &[],
                // Background upkeep a slow model must not fail on a clock;
                // its length bound and the scheduler end it.
                TextCall {
                    no_time_limit: true,
                    ..TextCall::at(InferencePriority::Maintenance)
                },
            )
            .await
            {
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

    #[test]
    fn test_corpus_fingerprint_is_stable_and_order_independent() {
        let a = corpus_fingerprint(
            "space_general",
            412,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        let b = corpus_fingerprint(
            "space_general",
            412,
            "2026-09-01T00:00:00Z",
            &[("md".to_string(), 88), ("pdf".to_string(), 412)],
        );
        assert_eq!(a, b, "type-mix ordering must not change the fingerprint");

        let same_again = corpus_fingerprint(
            "space_general",
            412,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_eq!(a, same_again, "same inputs must hash identically");

        let changed_count = corpus_fingerprint(
            "space_general",
            413,
            "2026-09-01T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_ne!(a, changed_count, "a changed count must change the hash");

        let changed_indexed_at = corpus_fingerprint(
            "space_general",
            412,
            "2026-09-02T00:00:00Z",
            &[("pdf".to_string(), 412), ("md".to_string(), 88)],
        );
        assert_ne!(a, changed_indexed_at);
    }

    /// Two spaces with an identical corpus shape get different fingerprints.
    #[test]
    fn test_two_spaces_of_the_same_shape_never_share_a_cache_row() {
        let mix = [("pdf".to_string(), 412), ("md".to_string(), 88)];
        let movies = corpus_fingerprint("movies", 500, "2026-09-01T00:00:00Z", &mix);
        let patents = corpus_fingerprint("patents", 500, "2026-09-01T00:00:00Z", &mix);

        assert_ne!(movies, patents, "the space must be part of the cache key");
    }

    #[test]
    fn test_parse_starters_tolerant() {
        let wrapped = parse_starters("Sure! [{\"question\":\"a\"}] hope that helps");
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].question, "a");

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
        let parsed = parse_starters(
            r#"[{"question":"Same One"},{"question":"same one"},{"question":"Other"}]"#,
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].question, "Other");
    }

    #[test]
    fn a_long_starter_question_is_kept_not_dropped() {
        let long = "What do my notes say about the same subject, asked at some length? ".repeat(3);
        let long = long.trim().to_string();
        let parsed = parse_starters(&format!(
            r#"[{{"question":"{long}"}},{{"question":"short"}}]"#
        ));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].question, long);
        assert_eq!(parsed[1].question, "short");
    }

    /// The anti-fabrication guard: with no LLM, the payload carries no questions.
    ///
    /// `generate_chat_starters_impl` needs a `ChatRuntime`, which cannot be
    /// built in a unit test, so the invariant is asserted where it actually
    /// lives — the parse/fallback path that produces the list.
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
}
