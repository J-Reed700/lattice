//! LLM-based cluster labeling.
//!
//! Given a handful of representative documents, produce a 3-5 word noun-phrase
//! label and a one-sentence description. Phase 5.3 goal: good-enough labels
//! Josh can eyeball without drowning the LLM in tokens.
//!
//! The prompt asks for strict JSON so we can parse deterministically. If
//! parsing fails (the LLM does what LLMs do) we fall back to a best-effort
//! line-by-line read and, if that fails, a deterministic fallback label.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::application::ports::LLMPort;
use crate::shared::error::Result;

/// The maximum content preview fed per representative doc. Keeps prompts
/// small on cheap utility LLMs without losing topicality.
pub const REP_CONTENT_PREVIEW_CHARS: usize = 500;

/// Upper bound on generated label length — LLMs love to run long.
pub const MAX_LABEL_CHARS: usize = 80;

/// A representative document for a cluster, as used by the labeling prompt.
#[derive(Debug, Clone)]
pub struct RepresentativeDoc {
    pub title: String,
    /// First ~500 chars of the document body (LLM sees a preview, not the whole doc).
    pub content_preview: String,
}

/// A successfully labeled cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterLabel {
    pub label: String,
    pub description: Option<String>,
}

/// Parse the LLM's JSON response. Strict format first, then loose recovery.
#[derive(Debug, Clone, Deserialize)]
struct RawLabelResponse {
    label: Option<String>,
    description: Option<String>,
}

/// Generate a `{label, description}` for a cluster's representative docs.
///
/// - `member_count` is injected into the fallback label when the LLM fails.
/// - Returns a `ClusterLabel` every time — fallback path is deterministic.
pub async fn label_cluster(
    llm: &Arc<dyn LLMPort>,
    representatives: &[RepresentativeDoc],
    member_count: usize,
) -> Result<ClusterLabel> {
    if representatives.is_empty() {
        return Ok(fallback_label(member_count, None));
    }

    let prompt = build_prompt(representatives);
    let system = build_system_prompt();

    // `LLMPort` has no per-call generation overrides — the system prompt does
    // the work instead ("JSON only, no preamble"), the parser is tolerant, and
    // `MAX_LABEL_CHARS` truncates whatever runs long.
    //
    // Ollama-style models want the "system" bit prepended to context, and
    // `generate` takes a plain prompt plus a context array, so the system
    // prompt goes in as context.
    let context = vec![system];
    let response = llm.generate(&prompt, &context, None).await;

    let raw = match response {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "LLM label generation failed; using fallback label"
            );
            let hint = representatives.first().map(|d| d.title.clone());
            return Ok(fallback_label(member_count, hint));
        }
    };

    match parse_label_response(&raw) {
        Some(label) => Ok(sanitize(label, representatives, member_count)),
        None => {
            tracing::warn!(
                raw = %raw,
                "Could not parse label response as JSON; using fallback"
            );
            let hint = representatives.first().map(|d| d.title.clone());
            Ok(fallback_label(member_count, hint))
        }
    }
}

/// Deterministic fallback when the LLM is absent or output is unusable.
///
/// Produces something like "Cluster of 47 (Pad See Ew)" or
/// "Cluster of 47" when no representative title is available.
pub fn fallback_label(member_count: usize, representative_title: Option<String>) -> ClusterLabel {
    let label = match representative_title {
        Some(title) if !title.trim().is_empty() => {
            let truncated = truncate_for_label(&title, 40);
            format!("Theme of {} ({})", member_count, truncated)
        }
        _ => format!("Theme of {}", member_count),
    };
    ClusterLabel {
        label,
        description: None,
    }
}

fn build_system_prompt() -> String {
    [
        "You name clusters of related documents.",
        "Given representative documents, produce a tight 3-5 word noun-phrase label and one sentence describing the theme.",
        "Favor domain nouns over generic filler. Avoid words like 'Miscellaneous', 'Various', 'Notes'.",
        "Respond with a single JSON object: {\"label\": \"...\", \"description\": \"...\"}. No prose before or after.",
        "Answer with JSON only. No preamble.",
    ]
    .join(" ")
}

fn build_prompt(representatives: &[RepresentativeDoc]) -> String {
    let mut out = String::new();
    out.push_str("Representative documents in this cluster:\n\n");
    for (i, doc) in representatives.iter().enumerate() {
        out.push_str(&format!("Document {}:\n", i + 1));
        out.push_str(&format!("Title: {}\n", doc.title));
        let preview: String = doc
            .content_preview
            .chars()
            .take(REP_CONTENT_PREVIEW_CHARS)
            .collect();
        out.push_str(&format!("Content: {}\n\n", preview));
    }
    out.push_str("Respond with JSON only: {\"label\": \"3-5 word noun phrase\", \"description\": \"one sentence\"}.");
    out
}

fn parse_label_response(raw: &str) -> Option<ClusterLabel> {
    // Strict-first: try parsing the whole thing as JSON.
    if let Ok(parsed) = serde_json::from_str::<RawLabelResponse>(raw.trim()) {
        if let Some(label) = parsed.label {
            return Some(ClusterLabel {
                label,
                description: parsed.description,
            });
        }
    }

    // Loose: scan for a {...} substring and try again. LLMs often wrap JSON
    // in markdown fences or preamble.
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    let slice = raw.get(start..=end)?;
    if let Ok(parsed) = serde_json::from_str::<RawLabelResponse>(slice) {
        if let Some(label) = parsed.label {
            return Some(ClusterLabel {
                label,
                description: parsed.description,
            });
        }
    }
    None
}

fn sanitize(
    mut label: ClusterLabel,
    representatives: &[RepresentativeDoc],
    member_count: usize,
) -> ClusterLabel {
    let trimmed = label.label.trim().to_string();
    if trimmed.is_empty() {
        let hint = representatives.first().map(|d| d.title.clone());
        return fallback_label(member_count, hint);
    }

    // Cap overrun.
    label.label = if trimmed.chars().count() > MAX_LABEL_CHARS {
        let boundary = trimmed
            .char_indices()
            .take(MAX_LABEL_CHARS)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        let safe = trimmed
            .get(..boundary)
            .unwrap_or(&trimmed)
            .trim_end()
            .to_string();
        if safe.is_empty() {
            trimmed
        } else {
            safe
        }
    } else {
        trimmed
    };

    if let Some(desc) = label.description.as_mut() {
        *desc = desc.trim().to_string();
        if desc.is_empty() {
            label.description = None;
        }
    }
    label
}

fn truncate_for_label(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let cut = s
        .char_indices()
        .take(max_chars)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    let prefix = s.get(..cut).unwrap_or("");
    format!("{}…", prefix.trim_end())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_json_response() {
        let raw = r#"{"label": "Thai Cuisine", "description": "Dishes from Thailand."}"#;
        let label = parse_label_response(raw).unwrap();
        assert_eq!(label.label, "Thai Cuisine");
        assert_eq!(label.description.as_deref(), Some("Dishes from Thailand."));
    }

    #[test]
    fn parses_json_wrapped_in_markdown_fences() {
        let raw = "```json\n{\"label\": \"Machine Learning Papers\", \"description\": \"ML research papers and notes.\"}\n```";
        let label = parse_label_response(raw).unwrap();
        assert_eq!(label.label, "Machine Learning Papers");
    }

    #[test]
    fn parses_json_with_preamble() {
        let raw = "Sure! Here you go: {\"label\":\"Rust Projects\",\"description\":\"Rust programming projects.\"}";
        let label = parse_label_response(raw).unwrap();
        assert_eq!(label.label, "Rust Projects");
    }

    #[test]
    fn rejects_non_json_garbage() {
        assert!(parse_label_response("just a plain sentence").is_none());
        assert!(parse_label_response("").is_none());
    }

    #[test]
    fn rejects_json_without_label() {
        let raw = r#"{"description": "some text"}"#;
        assert!(parse_label_response(raw).is_none());
    }

    #[test]
    fn fallback_label_uses_count_and_title() {
        let label = fallback_label(47, Some("Pad See Ew".to_string()));
        assert!(label.label.contains("47"));
        assert!(label.label.contains("Pad See Ew"));
        assert!(label.description.is_none());
    }

    #[test]
    fn fallback_label_handles_missing_title() {
        let label = fallback_label(12, None);
        assert!(label.label.contains("12"));
    }

    #[test]
    fn sanitize_empty_label_falls_back() {
        let reps = vec![RepresentativeDoc {
            title: "Hello".to_string(),
            content_preview: String::new(),
        }];
        let label = sanitize(
            ClusterLabel {
                label: "   ".to_string(),
                description: None,
            },
            &reps,
            3,
        );
        assert!(label.label.contains("Hello"));
    }

    #[test]
    fn sanitize_caps_very_long_labels() {
        let reps = vec![RepresentativeDoc {
            title: "t".to_string(),
            content_preview: String::new(),
        }];
        let long = "a".repeat(500);
        let label = sanitize(
            ClusterLabel {
                label: long,
                description: None,
            },
            &reps,
            1,
        );
        assert!(label.label.chars().count() <= MAX_LABEL_CHARS);
    }

    #[test]
    fn build_prompt_truncates_long_previews() {
        let huge = "x".repeat(10_000);
        let reps = vec![RepresentativeDoc {
            title: "Doc".to_string(),
            content_preview: huge,
        }];
        let prompt = build_prompt(&reps);
        // Prompt shouldn't contain the full 10k chars.
        assert!(prompt.len() < REP_CONTENT_PREVIEW_CHARS + 2_000);
    }
}
