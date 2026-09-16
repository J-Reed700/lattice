//! Lexical grounding pre-filter.
//!
//! Stemmed token overlap between each claim sentence and the passages it cites.
//! Cheap and deterministic, so it runs first on every turn and decides which
//! sentences are worth an LLM call. It cannot read meaning: a sentence that
//! reuses the source's vocabulary while inverting its meaning still scores
//! high, which is why numbers, dates and negations are escalated to the judge
//! regardless of overlap.

use crate::features::qa::dto::SourceDto;
use crate::shared::text_utils::normalize_whitespace;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashSet;

const MIN_CLAIM_CHARS: usize = 24;
pub(super) const MIN_CLAIM_TOKENS: usize = 5;
pub(super) const MIN_OVERLAP_RATIO: f32 = 0.32;
pub(super) const MIN_MATCHING_TOKENS: usize = 2;

/// Overlap at or above this ratio, with enough matching tokens, is treated as
/// settled by the lexical pass alone. Well clear of `MIN_OVERLAP_RATIO` so the
/// judge still sees everything in the ambiguous band just above threshold.
pub(super) const STRONG_OVERLAP_RATIO: f32 = 0.55;
pub(super) const STRONG_MATCHING_TOKENS: usize = 4;

/// One sentence the lexical pass considered a claim.
#[derive(Debug, Clone)]
pub(super) struct LexicalClaim {
    /// Sentence as written, citation markers included.
    pub(super) sentence: String,
    /// Sentence with `[n]` markers removed — the text the judge reads.
    pub(super) claim_text: String,
    /// Citation numbers exactly as the model wrote them (1-based).
    pub(super) citation_ids: Vec<u32>,
    /// Zero-based source indices those citation numbers resolve to.
    pub(super) cited_source_indices: Vec<usize>,
    pub(super) overlap_ratio: f32,
    pub(super) matching_tokens: usize,
    /// The lexical verdict: enough overlap to call this grounded.
    pub(super) supported: bool,
    /// Carries a number, date or negation, so vocabulary overlap cannot settle it.
    pub(super) contradiction_prone: bool,
}

/// Score every claim sentence in `response` against `sources`.
///
/// Returns one entry per sentence that cleared the claim filters; sentences
/// that are too short, are questions, or are boilerplate never appear.
pub(super) fn lexical_pass(response: &str, sources: &[SourceDto]) -> Vec<LexicalClaim> {
    if response.trim().is_empty() || sources.is_empty() {
        return Vec::new();
    }

    let source_token_sets = collect_source_token_sets(sources);
    if source_token_sets.is_empty() {
        return Vec::new();
    }

    let mut claims = Vec::new();
    for sentence in split_sentences(response) {
        if !is_claim_candidate(&sentence) {
            continue;
        }

        let citation_ids = extract_sentence_citation_ids(&sentence, sources.len());
        let cited_source_indices: Vec<usize> = citation_ids
            .iter()
            .filter_map(|id| (*id as usize).checked_sub(1))
            .filter(|idx| *idx < source_token_sets.len())
            .collect();
        let claim_text = strip_citation_markers(&sentence);
        let claim_tokens = extract_normalized_tokens(&claim_text);
        if claim_tokens.len() < MIN_CLAIM_TOKENS {
            continue;
        }

        let (overlap_ratio, matching_tokens) = if cited_source_indices.is_empty() {
            compute_best_overlap_ratio(&claim_tokens, source_token_sets.iter())
        } else {
            compute_best_overlap_ratio(
                &claim_tokens,
                cited_source_indices
                    .iter()
                    .filter_map(|idx| source_token_sets.get(*idx)),
            )
        };
        let supported =
            matching_tokens >= MIN_MATCHING_TOKENS && overlap_ratio >= MIN_OVERLAP_RATIO;

        claims.push(LexicalClaim {
            sentence: claim_for_note(&sentence),
            contradiction_prone: is_contradiction_prone(&claim_text),
            claim_text: claim_for_note(&claim_text),
            citation_ids,
            cited_source_indices,
            overlap_ratio,
            matching_tokens,
            supported,
        });
    }

    claims
}

/// Whether this claim is worth an LLM call.
///
/// Strong overlap settles a plain sentence. It never settles one carrying a
/// number, date or negation: those are exactly the claims that echo a source's
/// wording while stating the opposite of it.
pub(super) fn needs_judge(claim: &LexicalClaim) -> bool {
    let strongly_supported = claim.supported
        && claim.overlap_ratio >= STRONG_OVERLAP_RATIO
        && claim.matching_tokens >= STRONG_MATCHING_TOKENS;
    !strongly_supported || claim.contradiction_prone
}

/// Tokens that flip a sentence's meaning without changing its vocabulary.
const NEGATION_MARKERS: &[&str] = &[
    " not ",
    "n't ",
    " no ",
    " never ",
    " without ",
    " cannot ",
    " none ",
    " neither ",
    " nor ",
    " unlike ",
    " instead of ",
    " rather than ",
    " fails to ",
    " failed to ",
    " unable to ",
    " only ",
    " except ",
];

const MONTH_MARKERS: &[&str] = &[
    " january ",
    " february ",
    " march ",
    " april ",
    " june ",
    " july ",
    " august ",
    " september ",
    " october ",
    " november ",
    " december ",
];

fn is_contradiction_prone(claim: &str) -> bool {
    if claim.chars().any(|ch| ch.is_ascii_digit()) {
        return true;
    }

    // Pad and flatten punctuation so " not " matches "did not." as well.
    let mut padded = String::with_capacity(claim.len() + 2);
    padded.push(' ');
    for ch in claim.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}' {
            padded.extend(ch.to_lowercase());
        } else {
            padded.push(' ');
        }
    }
    padded.push(' ');

    NEGATION_MARKERS
        .iter()
        .chain(MONTH_MARKERS.iter())
        .any(|marker| padded.contains(marker))
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let candidate = current.trim();
            if !candidate.is_empty() {
                sentences.push(candidate.to_string());
            }
            current.clear();
        }
    }

    let trailing = current.trim();
    if !trailing.is_empty() {
        sentences.push(trailing.to_string());
    }

    sentences
}

fn is_claim_candidate(sentence: &str) -> bool {
    let s = sentence.trim();
    if s.len() < MIN_CLAIM_CHARS {
        return false;
    }
    if s.ends_with('?') {
        return false;
    }

    let lowercase = s.to_lowercase();
    if lowercase.starts_with("verification note:")
        || lowercase.starts_with("sources:")
        || lowercase.starts_with("reference:")
        || lowercase.starts_with("relevance")
    {
        return false;
    }

    true
}

fn claim_for_note(claim: &str) -> String {
    normalize_whitespace(claim).trim().to_string()
}

fn strip_citation_markers(sentence: &str) -> String {
    let mut out = String::with_capacity(sentence.len());
    let chars: Vec<char> = sentence.chars().collect();
    let mut idx = 0usize;

    while idx < chars.len() {
        let Some(&current) = chars.get(idx) else {
            break;
        };
        if current == '[' {
            let mut j = idx + 1;
            let mut saw_digit = false;
            while chars.get(j).is_some_and(|ch| ch.is_ascii_digit()) {
                saw_digit = true;
                j += 1;
            }
            if saw_digit && chars.get(j) == Some(&']') {
                idx = j + 1;
                continue;
            }
        }

        out.push(current);
        idx += 1;
    }

    out
}

/// Citation numbers in the sentence, as written, restricted to real sources.
fn extract_sentence_citation_ids(sentence: &str, source_count: usize) -> Vec<u32> {
    if source_count == 0 {
        return Vec::new();
    }

    let chars: Vec<char> = sentence.chars().collect();
    let mut idx = 0usize;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    while idx < chars.len() {
        if chars.get(idx) != Some(&'[') {
            idx += 1;
            continue;
        }

        let mut j = idx + 1;
        let mut digits = String::new();
        while let Some(ch) = chars.get(j).filter(|ch| ch.is_ascii_digit()) {
            digits.push(*ch);
            j += 1;
        }

        if digits.is_empty() || chars.get(j) != Some(&']') {
            idx += 1;
            continue;
        }

        if let Ok(raw_index) = digits.parse::<usize>() {
            if (1..=source_count).contains(&raw_index) && seen.insert(raw_index) {
                out.push(raw_index as u32);
            }
        }

        idx = j + 1;
    }

    out
}

fn collect_source_token_sets(sources: &[SourceDto]) -> Vec<HashSet<String>> {
    let mut sets = Vec::new();
    for source in sources {
        let mut text = String::new();
        text.push_str(&source.content);
        text.push(' ');
        text.push_str(&source.file_name);
        text.push(' ');
        text.push_str(&source.file_path);
        text.push(' ');
        if let Some(path) = source.path.as_deref() {
            text.push_str(path);
            text.push(' ');
        }
        if let Some(excerpt) = source.excerpt.as_deref() {
            text.push_str(excerpt);
            text.push(' ');
        }
        if let Some(excerpts) = source.chunk_excerpts.as_ref() {
            for excerpt in excerpts {
                text.push_str(&excerpt.excerpt);
                text.push(' ');
            }
        }

        let tokens = extract_normalized_tokens(&text);
        if !tokens.is_empty() {
            sets.push(tokens);
        }
    }
    sets
}

fn compute_best_overlap_ratio<'a>(
    claim_tokens: &HashSet<String>,
    source_token_sets: impl IntoIterator<Item = &'a HashSet<String>>,
) -> (f32, usize) {
    let mut best_ratio = 0.0f32;
    let mut best_match_count = 0usize;

    for source_tokens in source_token_sets {
        let match_count = claim_tokens.intersection(source_tokens).count();
        if claim_tokens.is_empty() {
            continue;
        }
        let ratio = match_count as f32 / claim_tokens.len() as f32;
        if ratio > best_ratio || (ratio == best_ratio && match_count > best_match_count) {
            best_ratio = ratio;
            best_match_count = match_count;
        }
    }

    (best_ratio, best_match_count)
}

fn extract_normalized_tokens(text: &str) -> HashSet<String> {
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
mod tests {
    use super::super::test_support::source;
    use super::*;

    #[test]
    fn grounded_claim_scores_above_threshold() {
        let claims = lexical_pass(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [1].",
            &[source(
                "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
            )],
        );

        assert_eq!(claims.len(), 1);
        assert!(claims[0].supported);
        assert_eq!(claims[0].citation_ids, vec![1]);
    }

    #[test]
    fn unsupported_claim_scores_below_threshold() {
        let claims = lexical_pass(
            "Blueberry plants tripled yield after lunar-cycle irrigation [1].",
            &[source(
                "The study measured antioxidant enzyme response under low-temperature stress.",
            )],
        );

        assert_eq!(claims.len(), 1);
        assert!(!claims[0].supported);
    }

    #[test]
    fn short_or_question_sentences_are_skipped() {
        let claims = lexical_pass("Any update?\nThanks.", &[source("Random source text.")]);
        assert!(claims.is_empty());
    }

    #[test]
    fn citation_index_selects_the_referenced_source() {
        let claims = lexical_pass(
            "Blueberry anthocyanins support vascular function and endothelial health [2].",
            &[
                source("Irrigation protocol and frost timing notes for greenhouse control."),
                source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
            ],
        );

        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].citation_ids, vec![2]);
        assert_eq!(claims[0].cited_source_indices, vec![1]);
        assert!(claims[0].supported);
    }

    #[test]
    fn mismatched_citation_index_is_flagged_even_if_another_source_matches() {
        let claims = lexical_pass(
            "Blueberry anthocyanins support vascular function and endothelial health [1].",
            &[
                source("Irrigation protocol and frost timing notes for greenhouse control."),
                source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
            ],
        );

        assert_eq!(claims.len(), 1);
        assert!(!claims[0].supported);
    }

    #[test]
    fn claim_text_keeps_the_full_sentence() {
        let claims = lexical_pass(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity, suggesting a mechanistic link between methylation dynamics and developmental transitions [1][2].",
            &[source("The study discusses WOX family structure and stress response without this meristem claim.")],
        );

        assert_eq!(claims.len(), 1);
        assert!(claims[0].claim_text.contains(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity"
        ));
        assert!(claims[0].claim_text.contains(
            "mechanistic link between methylation dynamics and developmental transitions"
        ));
        assert!(!claims[0].claim_text.contains("developmental tr..."));
    }

    #[test]
    fn numbers_dates_and_negations_are_contradiction_prone() {
        assert!(is_contradiction_prone("Yields rose by 42 percent."));
        assert!(is_contradiction_prone("The trial began in March 2021."));
        assert!(is_contradiction_prone(
            "The treatment did not improve tolerance."
        ));
        assert!(is_contradiction_prone(
            "The sample didn't survive the freeze."
        ));
        assert!(is_contradiction_prone("Only the treated group recovered."));
        assert!(!is_contradiction_prone(
            "Salicylic acid treatment improved cold tolerance in blueberry plants."
        ));
    }

    #[test]
    fn strong_overlap_skips_the_judge_but_a_number_does_not() {
        let plain = LexicalClaim {
            sentence: "s".into(),
            claim_text: "s".into(),
            citation_ids: vec![1],
            cited_source_indices: vec![0],
            overlap_ratio: 0.8,
            matching_tokens: 6,
            supported: true,
            contradiction_prone: false,
        };
        assert!(!needs_judge(&plain));

        let with_number = LexicalClaim {
            contradiction_prone: true,
            ..plain.clone()
        };
        assert!(needs_judge(&with_number));

        let ambiguous = LexicalClaim {
            overlap_ratio: 0.40,
            matching_tokens: 3,
            ..plain.clone()
        };
        assert!(needs_judge(&ambiguous));

        let weak = LexicalClaim {
            overlap_ratio: 0.10,
            matching_tokens: 1,
            supported: false,
            ..plain
        };
        assert!(needs_judge(&weak));
    }
}
