use std::collections::{HashMap, HashSet};

use crate::features::search::engine::query_expansion::dictionaries::select_informative_terms;
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::shared::text::safe_truncate;

use super::{
    extract_acronym_context_terms, extract_acronym_terms, extract_phrase_terms,
    normalize_keyword_token, tokenize_keyword_terms,
};

pub(super) fn select_wiki_search_query_with_tuning(
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> String {
    let terms = build_external_search_terms(
        validated_message,
        interpretation,
        followup_anchor_terms,
        tuning.external_search_max_wiki_terms as usize,
    );
    if terms.is_empty() {
        let candidate = validated_message.trim();
        let max_chars = tuning.external_search_query_max_chars as usize;
        if candidate.chars().count() <= max_chars {
            candidate.to_string()
        } else {
            candidate.chars().take(max_chars).collect()
        }
    } else {
        render_web_search_query(&terms, tuning.external_search_query_max_chars as usize)
    }
}

/// Default-tuning entry point retained for the retrieval unit tests, which
/// pin web-query selection behaviour without threading settings through.
#[cfg(test)]
pub(super) fn select_web_search_query<'a>(
    validated_message: &'a str,
    interpretation: &'a crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
) -> String {
    select_web_search_query_with_tuning(
        validated_message,
        interpretation,
        followup_anchor_terms,
        &RetrievalTuningSettingsDto::default(),
    )
}

pub(super) fn select_web_search_query_with_tuning<'a>(
    validated_message: &'a str,
    interpretation: &'a crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> String {
    let cleaned = search_text_without_url_tracking(validated_message);
    let raw_query = cleaned.trim();
    if should_use_raw_web_query(raw_query) {
        return safe_truncate(raw_query, tuning.external_search_query_max_chars as usize);
    }

    let terms = build_external_search_terms(
        validated_message,
        interpretation,
        followup_anchor_terms,
        tuning.external_search_max_web_terms as usize,
    );
    if terms.is_empty() {
        return safe_truncate(raw_query, tuning.external_search_query_max_chars as usize);
    }

    render_web_search_query(&terms, tuning.external_search_query_max_chars as usize)
}

fn build_external_search_terms(
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
    max_terms: usize,
) -> Vec<String> {
    if max_terms == 0 {
        return Vec::new();
    }

    let linked_subjects: Vec<String> = validated_message
        .split_whitespace()
        .filter(|word| word.contains("https://") || word.contains("http://"))
        .map(search_text_without_url_tracking)
        .filter(|subject| subject.split_whitespace().count() >= 2)
        .collect();
    let cleaned = search_text_without_url_tracking(validated_message);
    let validated_message = cleaned.as_str();
    let hyde_text = interpretation.hyde_text.as_deref();
    let mut scores: HashMap<String, f32> = HashMap::new();
    // A supplied descriptive URL is an explicit subject. Keep its product or
    // article phrase together, including short model names and units.
    for subject in linked_subjects {
        scores.insert(subject, 12.0);
    }

    if let Some(anchor_terms) = followup_anchor_terms {
        for term in anchor_terms.iter() {
            if let Some(normalized) = normalize_keyword_token(term) {
                *scores.entry(normalized).or_insert(0.0) += 2.0;
            }
        }
    }

    if let Some(hyde) = hyde_text {
        let hyde_terms = select_informative_terms(tokenize_keyword_terms(hyde), max_terms * 2);
        for term in hyde_terms {
            *scores.entry(term).or_insert(0.0) += 5.0;
        }
        for phrase in extract_phrase_terms(&tokenize_keyword_terms(hyde), 2, 2, max_terms) {
            *scores.entry(phrase).or_insert(0.0) += 3.0;
        }
    }

    // The current question owns the subject. Earlier anchors only resolve
    // missing context; they must not outrank a newly named product or topic.
    let raw_weight = if hyde_text.is_some() { 6.0 } else { 8.0 };
    let query_terms =
        select_informative_terms(tokenize_keyword_terms(validated_message), max_terms * 2);
    for (rank, term) in query_terms.into_iter().enumerate() {
        *scores.entry(term).or_insert(0.0) += raw_weight - rank as f32 * 0.05;
    }

    for acronym in extract_acronym_terms(validated_message) {
        *scores.entry(acronym).or_insert(0.0) += 4.0;
    }
    for context_term in extract_acronym_context_terms(validated_message, hyde_text) {
        *scores.entry(context_term).or_insert(0.0) += 2.0;
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    ranked
        .into_iter()
        .map(|(term, _)| term)
        .take(max_terms)
        .collect()
}

/// Keep descriptive URL paths but never feed query parameters, fragments or
/// advertising identifiers into lexical term ranking. Their length/entropy
/// otherwise makes them beat the actual subject when the rewrite times out.
pub(in crate::features::conversation::chat) fn search_text_without_url_tracking(
    text: &str,
) -> String {
    text.split_whitespace()
        .map(|word| {
            let start = word.find("https://").or_else(|| word.find("http://"));
            let Some(start) = start else {
                return word.to_string();
            };
            let candidate = word[start..].trim_end_matches([')', ']', '>', ',', '.']);
            match url::Url::parse(candidate) {
                Ok(url) => {
                    let path = url
                        .path()
                        .rsplit('/')
                        .find(|part| !part.is_empty())
                        .unwrap_or("");
                    let path = urlencoding::decode(path).unwrap_or_else(|_| path.into());
                    // Keep natural-language slugs, not opaque identifiers.
                    path.split(['-', '_', '.', '/'])
                        .filter(|part| part.len() <= 32 && part.chars().any(char::is_alphabetic))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                Err(_) => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Above this many content terms a message has stopped being query-shaped.
/// Search engines weigh every word they are given, so a rambling paragraph
/// buries its own subject under filler ("really", "need", "watched") and the
/// deep-research branch queries then append yet more terms to that paragraph.
/// Past the ceiling the keyword builder earns its keep: it ranks the terms and
/// keeps the top handful.
const MAX_RAW_WEB_QUERY_TERMS: usize = 12;

fn should_use_raw_web_query(query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    if is_generic_web_search_command(query) {
        return false;
    }

    let terms = tokenize_keyword_terms(query);
    if terms.is_empty() {
        return false;
    }

    // Very short single-token queries are often referential ("it", "that").
    if terms.len() == 1 {
        return terms.first().is_some_and(|term| term.len() >= 6);
    }

    terms.len() <= MAX_RAW_WEB_QUERY_TERMS
}

fn is_generic_web_search_command(query: &str) -> bool {
    let normalized: String = query
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 7 {
        return false;
    }

    let action_terms = [
        "search", "find", "lookup", "look", "google", "browse", "check",
    ];
    let target_terms = ["web", "internet", "online"];
    let neutral_terms = [
        "please", "can", "you", "for", "the", "on", "up", "me", "this", "that", "it", "to",
    ];

    let has_action = tokens
        .iter()
        .any(|token| action_terms.contains(token) || (*token == "look"));
    let has_target = tokens.iter().any(|token| target_terms.contains(token));
    let allowed = tokens.iter().all(|token| {
        action_terms.contains(token)
            || target_terms.contains(token)
            || neutral_terms.contains(token)
    });

    has_action && has_target && allowed
}

fn render_web_search_query(terms: &[String], max_chars: usize) -> String {
    if terms.is_empty() || max_chars == 0 {
        return String::new();
    }

    let mut query = String::new();
    for term in terms.iter().filter(|term| !term.trim().is_empty()) {
        if query.is_empty() {
            query.push_str(term);
            if query.len() >= max_chars {
                return query.chars().take(max_chars).collect();
            }
            continue;
        }

        let projected_len = query.len() + 1 + term.len();
        if projected_len > max_chars {
            break;
        }

        query.push(' ');
        query.push_str(term);
    }

    if query.is_empty() {
        terms
            .first()
            .map(|term| term.chars().take(max_chars).collect())
            .unwrap_or_default()
    } else {
        query
    }
}
