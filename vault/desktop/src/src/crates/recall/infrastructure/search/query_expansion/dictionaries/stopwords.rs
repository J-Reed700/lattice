//! Query-term salience utilities.
//!
//! Provides corpus-agnostic heuristics for selecting informative terms while
//! avoiding static stopword string lists.

use std::collections::HashMap;

const MIN_TERM_LEN: usize = 2;

/// Select a high-signal subset of terms using statistical salience.
///
/// The scoring blends character entropy and token length. This avoids static
/// language-specific stopword lists while still reducing low-information terms.
pub fn select_informative_terms(terms: Vec<String>, max_terms: usize) -> Vec<String> {
    if max_terms == 0 {
        return Vec::new();
    }

    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for term in terms {
        if let Some(normalized) = normalize_term(&term) {
            if seen.insert(normalized.clone()) {
                deduped.push(normalized);
            }
        }
    }
    if deduped.is_empty() {
        return Vec::new();
    }

    let mut ranked: Vec<(String, f32)> = deduped
        .into_iter()
        .map(|term| {
            let score = term_salience(&term);
            (term, score)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let best_score = ranked.first().map(|(_, score)| *score).unwrap_or(0.0);
    if best_score <= f32::EPSILON {
        return ranked
            .into_iter()
            .take(max_terms)
            .map(|(term, _)| term)
            .collect();
    }

    let mut selected: Vec<String> = ranked
        .iter()
        .filter_map(|(term, score)| (*score >= best_score * 0.42).then_some(term.clone()))
        .take(max_terms)
        .collect();

    if selected.is_empty() {
        selected = ranked
            .into_iter()
            .take(max_terms)
            .map(|(term, _)| term)
            .collect();
    }

    selected
}

fn normalize_term(term: &str) -> Option<String> {
    let trimmed = term.trim().to_ascii_lowercase();
    if trimmed.len() < MIN_TERM_LEN {
        return None;
    }
    if !trimmed.chars().any(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed)
}

fn term_entropy(term: &str) -> f32 {
    let len = term.len();
    if len == 0 {
        return 0.0;
    }

    let mut counts: HashMap<char, usize> = HashMap::new();
    for ch in term.chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }

    let denom = len as f32;
    counts.values().fold(0.0_f32, |acc, count| {
        let p = (*count as f32) / denom;
        if p <= f32::EPSILON {
            acc
        } else {
            acc - p * p.log2()
        }
    })
}

fn term_salience(term: &str) -> f32 {
    let entropy = term_entropy(term);
    let length_factor = ((term.len() as f32) + 1.0).ln();
    entropy * (0.65 + 0.35 * length_factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_informative_terms_prefers_specific_terms() {
        let terms = vec![
            "the".to_string(),
            "and".to_string(),
            "blueberries".to_string(),
            "anthocyanin".to_string(),
        ];

        let selected = select_informative_terms(terms, 8);
        assert!(selected.contains(&"blueberries".to_string()));
        assert!(selected.contains(&"anthocyanin".to_string()));
        assert!(!selected.contains(&"the".to_string()));
    }

    #[test]
    fn test_select_informative_terms_limits_results() {
        let terms = vec![
            "blueberries".to_string(),
            "anthocyanin".to_string(),
            "correlation".to_string(),
        ];
        let selected = select_informative_terms(terms, 2);
        assert!(selected.len() <= 2);
    }

    #[test]
    fn test_select_informative_terms_dedupes_case_insensitively() {
        let terms = vec!["Blueberries".to_string(), "blueberries".to_string()];
        let selected = select_informative_terms(terms, 8);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], "blueberries");
    }
}
