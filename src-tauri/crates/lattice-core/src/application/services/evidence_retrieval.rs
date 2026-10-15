//! Shared passage ranking. Token overlap only selects evidence; it never approves a claim.
use crate::shared::text::normalize_whitespace;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashSet;
pub(crate) const WINDOW_CHARS: usize = 1200;
/// Windows overlap by a third so a sentence that straddles a boundary still
/// lands whole in one of them.
const WINDOW_STEP_CHARS: usize = 800;
/// Text a window shares with each neighbour; joined neighbours show it once.
pub const WINDOW_OVERLAP_CHARS: usize = WINDOW_CHARS - WINDOW_STEP_CHARS;

/// The passage cut into overlapping windows; a short passage is one window.
pub fn passage_windows(text: &str) -> Vec<String> {
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
pub struct ScoredWindow {
    /// Position among the passage's windows, so picks can be put back in
    /// reading order and neighbours told apart.
    pub index: usize,
    /// Claim tokens this window shares.
    pub score: usize,
    pub text: String,
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
pub fn best_windows(claim: &str, text: &str, max: usize) -> Vec<ScoredWindow> {
    rank_windows(claim, passage_windows(text), max)
}

/// Every window of `text`, scored against `claim`, in page order.
///
/// Unlike [`best_windows`] nothing is left out — windows sharing no
/// vocabulary score zero and neighbours are all kept — for a caller that
/// covers a whole passage, in parts if it must.
pub fn scored_windows(claim: &str, text: &str) -> Vec<ScoredWindow> {
    let claim_tokens = extract_normalized_tokens(claim);
    passage_windows(text)
        .into_iter()
        .enumerate()
        .map(|(index, window)| ScoredWindow {
            index,
            score: claim_tokens
                .intersection(&extract_normalized_tokens(&window))
                .count(),
            text: window,
        })
        .collect()
}

/// Windows of one passage as continuous text, in page order. Neighbouring
/// windows are joined where they overlap, so the text they share is shown
/// once; a stretch left out between two windows is marked with `gap`.
pub fn join_windows(windows: &[&ScoredWindow], gap: &str) -> String {
    let mut ordered = windows.to_vec();
    ordered.sort_by_key(|window| window.index);
    ordered.dedup_by_key(|window| window.index);
    let overlap = WINDOW_OVERLAP_CHARS;
    let mut joined = String::new();
    let mut previous: Option<usize> = None;
    for window in ordered {
        match previous {
            Some(index) if window.index == index + 1 => {
                joined.extend(window.text.chars().skip(overlap));
            }
            Some(_) => {
                joined.push_str(gap);
                joined.push_str(&window.text);
            }
            None => joined.push_str(&window.text),
        }
        previous = Some(window.index);
    }
    joined
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

pub fn extract_normalized_tokens(text: &str) -> HashSet<String> {
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

#[cfg(test)]
mod window_tests {
    use super::*;

    /// Joining every window of a passage gives the passage back: the overlap
    /// is shown once, and nothing is lost between windows.
    #[test]
    fn every_window_joined_is_the_whole_passage() {
        let text: String = (0..900).map(|i| format!("w{i} ")).collect();
        let normalized = normalize_whitespace(&text);
        let windows = scored_windows("w5", &text);
        assert!(windows.len() > 3);
        let all: Vec<&ScoredWindow> = windows.iter().collect();
        assert_eq!(join_windows(&all, " […] "), normalized);
    }

    #[test]
    fn a_gap_between_windows_is_marked_and_order_is_restored() {
        let text: String = (0..900).map(|i| format!("w{i} ")).collect();
        let windows = scored_windows("w5", &text);
        let picked = [&windows[3], &windows[0]];
        let joined = join_windows(&picked, "|GAP|");
        assert!(joined.starts_with(windows[0].text.as_str()));
        assert!(joined.ends_with(windows[3].text.as_str()));
        assert!(joined.contains("|GAP|"));
    }

    #[test]
    fn scoring_keeps_windows_that_share_nothing() {
        let text = format!("{} needle", "hay ".repeat(1_000));
        let windows = scored_windows("needle", &text);
        assert_eq!(windows.len(), passage_windows(&text).len());
        assert!(windows.iter().any(|window| window.score == 0));
        assert!(windows.last().is_some_and(|window| window.score == 1));
    }
}
