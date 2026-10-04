//! Shared passage ranking. Token overlap only selects evidence; it never approves a claim.
use crate::shared::text::normalize_whitespace;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashSet;
pub(crate) const WINDOW_CHARS: usize = 1200;
/// Windows overlap by a third so a sentence that straddles a boundary still
/// lands whole in one of them.
const WINDOW_STEP_CHARS: usize = 800;

/// The passage cut into overlapping windows; a short passage is one window.
pub(crate) fn passage_windows(text: &str) -> Vec<String> {
    exact_windows(&normalize_whitespace(text))
}

fn exact_windows(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= WINDOW_CHARS {
        return vec![text.to_owned()];
    }
    let mut windows = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + WINDOW_CHARS).min(chars.len());
        windows.push(chars.iter().skip(start).take(end - start).collect());
        if end == chars.len() {
            break;
        }
        start += WINDOW_STEP_CHARS;
    }
    windows
}

/// One stretch of a passage, scored against a claim.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScoredWindow {
    /// Position among the passage's windows, so picks can be put back in
    /// reading order and neighbours told apart.
    pub(crate) index: usize,
    /// Claim tokens this window shares.
    pub(crate) score: usize,
    pub(crate) text: String,
}

/// Up to `max` windows of `text` that share the most vocabulary with `claim`,
/// best first.
///
/// An answer synthesises: one sentence can join a figure from the middle of a
/// page to a condition near its end, and no single window holds both. So the
/// judge is shown several. A window next to one already taken is skipped —
/// the two share a third of their text, and the overlap exists so a sentence
/// lands whole in one of them, not so it is shown twice. A window sharing no
/// vocabulary at all is dropped unless it is the passage's only one.
pub(crate) fn best_windows(claim: &str, text: &str, max: usize) -> Vec<ScoredWindow> {
    rank_windows(claim, passage_windows(text), max)
}

fn rank_windows(claim: &str, windows: Vec<String>, max: usize) -> Vec<ScoredWindow> {
    let claim_tokens = extract_normalized_tokens(claim);
    let only_one = windows.len() == 1;
    let mut scored: Vec<ScoredWindow> = windows
        .into_iter()
        .enumerate()
        .map(|(index, window)| ScoredWindow {
            index,
            score: claim_tokens
                .intersection(&extract_normalized_tokens(&window))
                .count(),
            text: window,
        })
        .filter(|window| only_one || window.score > 0)
        .collect();
    // Stable: equal scores keep page order.
    scored.sort_by_key(|window| std::cmp::Reverse(window.score));

    let mut picked: Vec<ScoredWindow> = Vec::new();
    for window in scored {
        if picked.len() >= max {
            break;
        }
        if picked
            .iter()
            .any(|taken| taken.index.abs_diff(window.index) <= 1)
        {
            continue;
        }
        picked.push(window);
    }
    picked
}

pub(crate) fn extract_normalized_tokens(text: &str) -> HashSet<String> {
    static STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));

    let mut tokens = HashSet::new();
    let mut current = String::new();

    let flush = |current: &mut String, tokens: &mut HashSet<String>| {
        if current.is_empty() {
            return;
        }
        let lowered = current.to_lowercase();
        current.clear();

        if lowered.len() < 3 || STOPWORDS.contains(&lowered.as_str()) {
            return;
        }

        let stemmed = STEMMER.stem(&lowered).to_string();
        if stemmed.len() >= 3 && !STOPWORDS.contains(&stemmed.as_str()) {
            tokens.insert(stemmed);
        }
    };

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else {
            flush(&mut current, &mut tokens);
        }
    }
    flush(&mut current, &mut tokens);

    tokens
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "about", "which", "were", "have",
    "has", "had", "been", "being", "their", "there", "would", "could", "should", "your", "than",
    "then", "also", "only", "more", "most", "such", "some", "very", "over", "under", "between",
    "while", "where", "when", "what", "who", "whom", "into", "onto", "using", "used", "through",
    "across", "each", "other", "these", "those", "because", "after", "before", "during", "within",
    "without", "them", "they", "its", "it's", "was", "are", "is", "you", "our", "out", "all",
    "any", "can", "may", "not", "but", "per",
];
