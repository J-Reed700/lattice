//! Query-term salience utilities.
//!
//! Selecting informative query terms is done in two passes: a closed-class
//! stoplist removes function words and contentless modifiers outright, then a
//! statistical salience score ranks whatever survives.
//!
//! The stoplist is not optional. An earlier version of this module scored terms
//! *purely* by character entropy and length, on the theory that this avoided
//! maintaining a language-specific word list. It does — but length is
//! anti-correlated with informativeness in English questions: in "What is
//! specifically going on with NASA right now?", `specifically` scores 4.64 and
//! `nasa` scores 1.82, so the relative-score cutoff dropped the actual subject
//! of the query and kept the adverb. Closed-class words are a closed set; they
//! are cheaper and far more accurate to enumerate than to infer.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const MIN_TERM_LEN: usize = 2;

/// Function words and contentless modifiers, which carry no retrieval signal
/// regardless of how long or high-entropy they happen to be.
///
/// Deliberately conservative: only words that are near-never the subject of a
/// question. Domain nouns are never listed here, even common ones — dropping a
/// term the user actually asked about is far worse than keeping a weak one.
const STOPWORDS: &[&str] = &[
    // Articles, determiners, quantifiers
    "a",
    "an",
    "the",
    "this",
    "that",
    "these",
    "those",
    "each",
    "every",
    "any",
    "all",
    "both",
    "some",
    "no",
    "other",
    "another",
    "such",
    "same",
    "own",
    "few",
    "many",
    "much",
    "most",
    "several",
    "enough",
    // Pronouns
    "i",
    "me",
    "my",
    "mine",
    "myself",
    "we",
    "us",
    "our",
    "ours",
    "ourselves",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
    "he",
    "him",
    "his",
    "himself",
    "she",
    "her",
    "hers",
    "herself",
    "it",
    "its",
    "itself",
    "they",
    "them",
    "their",
    "theirs",
    "themselves",
    "who",
    "whom",
    "whose",
    "which",
    "what",
    "whatever",
    "whoever",
    "whichever",
    // Auxiliaries, copulas, modals
    "am",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "having",
    "do",
    "does",
    "did",
    "doing",
    "done",
    "will",
    "would",
    "shall",
    "should",
    "can",
    "could",
    "may",
    "might",
    "must",
    "ought",
    "let",
    // Prepositions and conjunctions
    "about",
    "above",
    "across",
    "after",
    "against",
    "along",
    "among",
    "around",
    "as",
    "at",
    "before",
    "behind",
    "below",
    "beneath",
    "beside",
    "between",
    "beyond",
    "but",
    "by",
    "despite",
    "down",
    "during",
    "except",
    "for",
    "from",
    "in",
    "inside",
    "into",
    "like",
    "near",
    "of",
    "off",
    "on",
    "onto",
    "or",
    "out",
    "outside",
    "over",
    "past",
    "per",
    "since",
    "than",
    "through",
    "throughout",
    "to",
    "toward",
    "towards",
    "under",
    "until",
    "up",
    "upon",
    "via",
    "with",
    "within",
    "without",
    "and",
    "nor",
    "so",
    "yet",
    "if",
    "because",
    "although",
    "though",
    "unless",
    "while",
    "whereas",
    "whether",
    // Interrogatives and discourse glue
    "how",
    "when",
    "where",
    "why",
    "there",
    "here",
    "then",
    "thus",
    "hence",
    "therefore",
    "however",
    "moreover",
    "also",
    "too",
    "either",
    "neither",
    // Contentless modifiers — the class that motivated this list
    "very",
    "really",
    "quite",
    "rather",
    "somewhat",
    "fairly",
    "pretty",
    "just",
    "only",
    "even",
    "still",
    "already",
    "yet",
    "again",
    "ever",
    "never",
    "always",
    "often",
    "sometimes",
    "usually",
    "generally",
    "typically",
    "basically",
    "essentially",
    "actually",
    "literally",
    "simply",
    "merely",
    "specifically",
    "particularly",
    "especially",
    "exactly",
    "precisely",
    "approximately",
    "roughly",
    "nearly",
    "almost",
    "currently",
    "recently",
    "now",
    "today",
    "lately",
    "soon",
    "right",
    "well",
    "back",
    "far",
    "long",
    "new",
    "old",
    "good",
    "great",
    "best",
    // Light verbs — carry no topical content on their own
    "go",
    "goes",
    "going",
    "gone",
    "went",
    "get",
    "gets",
    "getting",
    "got",
    "make",
    "makes",
    "making",
    "made",
    "take",
    "takes",
    "taking",
    "took",
    "come",
    "comes",
    "coming",
    "came",
    "give",
    "gives",
    "giving",
    "gave",
    "put",
    "puts",
    "putting",
    "say",
    "says",
    "saying",
    "said",
    "tell",
    "tells",
    "telling",
    "told",
    "know",
    "knows",
    "knowing",
    "knew",
    "known",
    "think",
    "thinks",
    "thinking",
    "thought",
    "want",
    "wants",
    "wanting",
    "need",
    "needs",
    "needing",
    "use",
    "uses",
    "using",
    "used",
];

fn stopword_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| STOPWORDS.iter().copied().collect())
}

/// True when `term` is a function word or contentless modifier.
///
/// `term` is expected to be lowercase and trimmed.
pub fn is_stopword(term: &str) -> bool {
    stopword_set().contains(term)
}

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
    if is_stopword(&trimmed) {
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
