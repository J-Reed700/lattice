use crate::application::dtos::qa_dto::SourceDto;
use crate::shared::text_utils::normalize_whitespace;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashSet;

const MIN_CLAIM_CHARS: usize = 24;
const MIN_CLAIM_TOKENS: usize = 5;
const MIN_OVERLAP_RATIO: f32 = 0.32;
const MIN_MATCHING_TOKENS: usize = 2;

#[derive(Debug, Clone, Default)]
pub(super) struct GroundingReport {
    pub(super) claims_evaluated: usize,
    pub(super) supported_claims: usize,
    pub(super) supported_claim_notes: Vec<String>,
    pub(super) unsupported_claims: Vec<String>,
}

impl GroundingReport {
    pub(super) fn unsupported_count(&self) -> usize {
        self.unsupported_claims.len()
    }

    pub(super) fn grounded_ratio(&self) -> f32 {
        if self.claims_evaluated == 0 {
            return 1.0;
        }
        self.supported_claims as f32 / self.claims_evaluated as f32
    }
}

pub(super) fn verify_response_grounding_summary(
    response: &str,
    sources: &[SourceDto],
) -> GroundingReport {
    verify_response_grounding(response, sources)
}

fn verify_response_grounding(response: &str, sources: &[SourceDto]) -> GroundingReport {
    if response.trim().is_empty() || sources.is_empty() {
        return GroundingReport::default();
    }

    let source_token_sets = collect_source_token_sets(sources);
    if source_token_sets.is_empty() {
        return GroundingReport::default();
    }

    let mut report = GroundingReport::default();
    for sentence in split_sentences(response) {
        if !is_claim_candidate(&sentence) {
            continue;
        }

        let cited_source_indices =
            extract_sentence_citation_indices(&sentence, source_token_sets.len());
        let claim_text = strip_citation_markers(&sentence);
        let claim_tokens = extract_normalized_tokens(&claim_text);
        if claim_tokens.len() < MIN_CLAIM_TOKENS {
            continue;
        }

        report.claims_evaluated += 1;
        let (best_ratio, best_match_count) = if cited_source_indices.is_empty() {
            compute_best_overlap_ratio(&claim_tokens, source_token_sets.iter())
        } else {
            compute_best_overlap_ratio(
                &claim_tokens,
                cited_source_indices
                    .iter()
                    .filter_map(|idx| source_token_sets.get(*idx)),
            )
        };
        let supported = best_match_count >= MIN_MATCHING_TOKENS && best_ratio >= MIN_OVERLAP_RATIO;

        if supported {
            report.supported_claims += 1;
            report
                .supported_claim_notes
                .push(claim_for_note(&claim_text));
        } else {
            report.unsupported_claims.push(claim_for_note(&claim_text));
        }
    }

    report
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
        if chars[idx] == '[' {
            let mut j = idx + 1;
            let mut saw_digit = false;
            while j < chars.len() && chars[j].is_ascii_digit() {
                saw_digit = true;
                j += 1;
            }
            if saw_digit && j < chars.len() && chars[j] == ']' {
                idx = j + 1;
                continue;
            }
        }

        out.push(chars[idx]);
        idx += 1;
    }

    out
}

fn extract_sentence_citation_indices(sentence: &str, source_count: usize) -> Vec<usize> {
    if source_count == 0 {
        return Vec::new();
    }

    let chars: Vec<char> = sentence.chars().collect();
    let mut idx = 0usize;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    while idx < chars.len() {
        if chars[idx] != '[' {
            idx += 1;
            continue;
        }

        let mut j = idx + 1;
        let mut digits = String::new();
        while j < chars.len() && chars[j].is_ascii_digit() {
            digits.push(chars[j]);
            j += 1;
        }

        if digits.is_empty() || j >= chars.len() || chars[j] != ']' {
            idx += 1;
            continue;
        }

        if let Ok(raw_index) = digits.parse::<usize>() {
            if (1..=source_count).contains(&raw_index) {
                let zero_based = raw_index - 1;
                if seen.insert(zero_based) {
                    out.push(zero_based);
                }
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
    use super::*;

    fn source(content: &str) -> SourceDto {
        SourceDto {
            document_id: "doc-1".to_string(),
            chunk_id: "chunk-1".to_string(),
            content: content.to_string(),
            score: 1.0,
            path: None,
            position: None,
            file_name: "doc.txt".to_string(),
            file_path: "/tmp/doc.txt".to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text File".to_string(),
            file_size_bytes: 100,
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
        }
    }

    #[test]
    fn grounded_claim_is_not_flagged() {
        let response =
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [1].";
        let sources = vec![source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
        )];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 0);
        assert_eq!(report.supported_claim_notes.len(), 1);
    }

    #[test]
    fn unsupported_claim_is_reported() {
        let response = "Blueberry plants tripled yield after lunar-cycle irrigation [1].";
        let sources = vec![source(
            "The study measured antioxidant enzyme response under low-temperature stress.",
        )];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 1);
        assert!(report.supported_claim_notes.is_empty());
    }

    #[test]
    fn short_or_question_sentences_are_skipped() {
        let response = "Any update?\nThanks.";
        let sources = vec![source("Random source text for testing.")];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.claims_evaluated, 0);
        assert_eq!(report.unsupported_count(), 0);
    }

    #[test]
    fn citation_index_checks_the_referenced_source() {
        let response =
            "Blueberry anthocyanins support vascular function and endothelial health [2].";
        let sources = vec![
            source("Irrigation protocol and frost timing notes for greenhouse control."),
            source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
        ];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 0);
    }

    #[test]
    fn mismatched_citation_index_is_flagged_even_if_another_source_matches() {
        let response =
            "Blueberry anthocyanins support vascular function and endothelial health [1].";
        let sources = vec![
            source("Irrigation protocol and frost timing notes for greenhouse control."),
            source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
        ];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 1);
    }

    #[test]
    fn unsupported_claim_note_keeps_full_sentence() {
        let response = "The same epigenetic shifts are linked to altered expression of genes that control meristem activity, suggesting a mechanistic link between methylation dynamics and developmental transitions [1][2].";
        let sources = vec![source(
            "The study discusses WOX family structure and stress response without this meristem claim.",
        )];

        let report = verify_response_grounding_summary(response, &sources);
        assert_eq!(report.unsupported_count(), 1);
        assert!(report.unsupported_claims[0].contains(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity"
        ));
        assert!(report.unsupported_claims[0].contains(
            "mechanistic link between methylation dynamics and developmental transitions"
        ));
        assert!(!report.unsupported_claims[0].contains("developmental tr..."));
    }
}
