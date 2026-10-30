//! Cleaning and guarding model rewrites: compaction, answer detection and web-query normalisation.

use std::collections::HashSet;

use lazy_regex::regex;

/// The follow-up searches in a model's reply to [`RESEARCH_FOLLOWUPS_TEMPLATE`]:
/// one per line, cleaned the way the main query is, anything that is not a
/// search-bar query dropped, and none repeating `root` or each other.
pub(super) fn parse_research_followups(raw: &str, root: &str, count: usize) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::from([root.trim().to_lowercase()]);
    raw.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|ch: char| {
                    ch.is_ascii_digit() || matches!(ch, '-' | '*' | '•' | '.' | ')' | ' ')
                })
                .to_string()
        })
        .map(|line| normalize_web_search_query(&line))
        .filter(|line| {
            let words = line.split_whitespace().count();
            (2..=MAX_WEB_SEARCH_QUERY_WORDS).contains(&words)
        })
        .filter(|line| seen.insert(line.to_lowercase()))
        .take(count)
        .collect()
}

/// The verdict in a model's reply to [`FOLLOWUP_CLASSIFIER_TEMPLATE`]: the
/// first word of its last line, so a model that reasons first is read by its
/// conclusion and not by a digit in its reasoning.
pub(super) fn is_followup_verdict(raw: &str) -> bool {
    raw.lines()
        .rev()
        .find_map(|line| line.split_whitespace().next())
        .is_some_and(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()) == "1")
}

pub(super) fn compact_hyde_text(raw: &str) -> String {
    const MAX_WORDS: usize = 90;
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() <= MAX_WORDS {
        return raw.trim().to_string();
    }
    words
        .iter()
        .take(MAX_WORDS)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize_guard_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn term_information_score(term: &str) -> f32 {
    if term.is_empty() {
        return 0.0;
    }

    let len = term.len() as f32;
    let alpha_count = term.chars().filter(|c| c.is_ascii_alphabetic()).count() as f32;
    if alpha_count <= 0.0 {
        return 0.0;
    }

    let unique_ratio = (term.chars().collect::<HashSet<char>>().len() as f32 / len).min(1.0);
    let alpha_ratio = (alpha_count / len).min(1.0);
    let length_boost = (len + 1.0).ln();

    alpha_ratio * (0.62 + 0.38 * unique_ratio) * length_boost
}

fn is_informative_term(term: &str) -> bool {
    term.len() >= 4 && term_information_score(term) >= 0.9
}

pub(super) fn build_retrieval_fallback(query: &str) -> String {
    let question = query.trim().trim_end_matches(&['?', '.', '!'][..]).trim();
    let mut seen = HashSet::new();
    let keywords: Vec<String> = tokenize_guard_terms(question)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .filter(|token| seen.insert(token.clone()))
        .take(8)
        .collect();

    if keywords.is_empty() {
        return question.to_string();
    }

    format!(
        "{question}. Retrieval focus terms: {}.",
        keywords.join(", ")
    )
}

pub(super) fn has_placeholder_artifacts(text: &str) -> bool {
    regex!(
        r"(?ix)
        \[
            \s*
            (the\s+)?(subject|topic|entity|item|thing)
            \s*
        \]
        |
        <\s*(subject|topic|entity|item|thing)\s*>
        "
    )
    .is_match(text)
}

pub(super) fn looks_like_direct_answer(query: &str, hyde_text: &str) -> bool {
    let hyde_terms: Vec<String> = tokenize_guard_terms(hyde_text)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .collect();
    if hyde_terms.is_empty() {
        return false;
    }

    let query_terms: HashSet<String> = tokenize_guard_terms(query)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .collect();
    let shared_count = hyde_terms
        .iter()
        .filter(|term| query_terms.contains(*term))
        .count();
    let novelty_count = hyde_terms
        .iter()
        .filter(|term| !query_terms.contains(*term))
        .count();
    let shared_ratio = shared_count as f32 / hyde_terms.len() as f32;
    let sentence_count = hyde_text
        .split(&['.', '!', '?'][..])
        .filter(|segment| !segment.trim().is_empty())
        .count();
    let hyde_word_count = hyde_text.split_whitespace().count();
    let is_question = query.trim_end().ends_with('?');

    is_question
        && hyde_word_count <= 28
        && sentence_count <= 2
        && novelty_count <= 2
        && shared_ratio >= 0.55
}

/// The most words a rewrite can have and still be a search query.
///
/// The prompt asks for three to eight; this is the net under it, not the
/// target. Past it the model has retold the question rather than rewritten it,
/// and an engine given the retelling matches nothing specific: it falls back
/// on whatever page shares one common word, which is how a search for what to
/// grow indoors came back with the dictionary definition of "indoor".
///
/// A rewrite over the line is discarded, not cut down to it. The first twelve
/// words of a retelling are no more a search query than the whole of it, and
/// cutting one to fit is how the failure stayed invisible: the query went out
/// as its first twenty-four words, ending mid-phrase on a dangling adjective,
/// looking deliberate.
pub(super) const MAX_WEB_SEARCH_QUERY_WORDS: usize = 12;

pub(super) fn normalize_web_search_query(raw: &str) -> String {
    let mut line = raw
        .lines()
        .find(|candidate| !candidate.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string();

    for prefix in ["search query:", "query:"] {
        if line.to_ascii_lowercase().starts_with(prefix) {
            line = line[prefix.len()..].trim().to_string();
            break;
        }
    }

    if line.starts_with('"') && line.ends_with('"') && line.len() >= 2 {
        line = line[1..line.len() - 1].trim().to_string();
    }
    if line.starts_with('\'') && line.ends_with('\'') && line.len() >= 2 {
        line = line[1..line.len() - 1].trim().to_string();
    }

    let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");

    compact.trim_matches(&['.', ';'][..]).trim().to_string()
}
