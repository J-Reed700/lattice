use crate::features::search::dto::SearchResultDto;
use crate::infrastructure::search::query_expansion::dictionaries::select_informative_terms;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct KeywordQueryPlan {
    pub(crate) terms: Vec<String>,
    pub(crate) query: String,
}

pub(crate) fn build_keyword_query_plan(query: &str, hyde_text: Option<&str>) -> KeywordQueryPlan {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let raw_query_tokens = tokenize_keyword_terms(query);
    let query_tokens = {
        let selected = select_informative_terms(raw_query_tokens.clone(), 18);
        if selected.is_empty() {
            raw_query_tokens
        } else {
            selected
        }
    };
    let hyde_tokens = hyde_text
        .map(|hyde| {
            let raw = tokenize_keyword_terms(hyde);
            let selected = select_informative_terms(raw.clone(), 14);
            if selected.is_empty() {
                raw
            } else {
                selected
            }
        })
        .unwrap_or_default();
    let has_hyde = !hyde_tokens.is_empty();
    let acronym_terms = super::extract_acronym_terms(query);
    let acronym_context_terms = super::extract_acronym_context_terms(query, hyde_text);
    let (query_term_weight, query_phrase_weight, hyde_term_weight, hyde_phrase_weight) = if has_hyde
    {
        // When HyDE exists, give lexical expansion priority to HyDE vocabulary.
        (2.8, 4.0, 4.6, 5.8)
    } else {
        (5.0, 7.0, 1.2, 1.6)
    };

    // Query terms provide anchoring.
    for term in &query_tokens {
        *scores.entry(term.clone()).or_insert(0.0) += query_term_weight;
    }
    for term in extract_phrase_terms(&query_tokens, 2, 3, 24) {
        *scores.entry(term).or_insert(0.0) += query_phrase_weight;
    }

    // HyDE text should dominate lexical evidence when available.
    for term in &hyde_tokens {
        *scores.entry(term.clone()).or_insert(0.0) += hyde_term_weight;
    }
    for term in extract_phrase_terms(&hyde_tokens, 2, 2, 18) {
        *scores.entry(term).or_insert(0.0) += hyde_phrase_weight;
    }

    // Stem variants improve lexical recall deterministically.
    for term in query_tokens.iter().chain(hyde_tokens.iter()) {
        let stem = stem_token(term);
        if stem != *term && stem.len() >= 4 {
            *scores.entry(stem).or_insert(0.0) += 1.0;
        }
    }

    if !acronym_terms.is_empty() {
        for term in &acronym_terms {
            *scores.entry(term.clone()).or_insert(0.0) += 4.0;
        }
        for term in &acronym_context_terms {
            *scores.entry(term.clone()).or_insert(0.0) += 2.5;
        }
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut terms: Vec<String> = ranked
        .into_iter()
        .map(|(term, _)| term)
        .filter(|term| is_keyword_term_specific(term, has_hyde) || acronym_terms.contains(term))
        .take(18)
        .collect();
    if terms.is_empty() {
        terms.extend(
            query_tokens
                .into_iter()
                .filter(|term| {
                    is_keyword_term_specific(term, has_hyde) || acronym_terms.contains(term)
                })
                .take(10),
        );
    }

    let query =
        render_bm25_query_from_terms(&terms, 18).unwrap_or_else(|| query.trim().to_string());
    KeywordQueryPlan { terms, query }
}

fn is_keyword_term_specific(term: &str, hyde_present: bool) -> bool {
    let term_tokens = tokenize_keyword_terms(term);
    if term_tokens.is_empty() {
        return false;
    }

    // Always keep multi-token phrases; they encode more structure than single terms.
    if term_tokens.len() >= 2 {
        return true;
    }

    let Some(len) = term_tokens.first().map(String::len) else {
        return false;
    };
    if hyde_present {
        // With HyDE available, suppress short conversational singletons.
        len >= 5
    } else {
        len >= 4
    }
}

pub(super) fn tokenize_keyword_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            if let Some(token) = normalize_keyword_token(&current) {
                tokens.push(token);
            }
            current = String::new();
        }
    }
    if !current.is_empty() {
        if let Some(token) = normalize_keyword_token(&current) {
            tokens.push(token);
        }
    }
    tokens
}

pub(super) fn normalize_keyword_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.len() < 3 {
        return None;
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed.to_string())
}

pub(super) fn stem_token(token: &str) -> String {
    static EN_STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));
    let normalized = token.to_ascii_lowercase();
    EN_STEMMER.stem(&normalized).to_string()
}

pub(super) fn extract_phrase_terms(
    tokens: &[String],
    min_n: usize,
    max_n: usize,
    max_terms: usize,
) -> Vec<String> {
    if tokens.len() < min_n || max_terms == 0 {
        return Vec::new();
    }

    let mut phrases = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for n in min_n..=max_n {
        if tokens.len() < n {
            continue;
        }
        for window in tokens.windows(n) {
            let phrase = window.join(" ");
            if seen.insert(phrase.clone()) {
                phrases.push(phrase);
                if phrases.len() >= max_terms {
                    return phrases;
                }
            }
        }
    }
    phrases
}

fn render_bm25_query_from_terms(terms: &[String], max_terms: usize) -> Option<String> {
    if terms.is_empty() || max_terms == 0 {
        return None;
    }

    let mut rendered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for term in terms.iter().filter(|t| !t.trim().is_empty()) {
        if !seen.insert(term.clone()) {
            continue;
        }
        let escaped = term.replace('"', "");
        let rendered_term = if escaped.contains(' ') {
            format!("\"{}\"", escaped)
        } else {
            escaped
        };
        rendered.push(rendered_term);
        if rendered.len() >= max_terms {
            break;
        }
    }

    if rendered.is_empty() {
        None
    } else if rendered.len() == 1 {
        rendered.into_iter().next()
    } else {
        Some(rendered.join(" OR "))
    }
}

pub(super) fn expand_keyword_plan_with_rm3(
    base_plan: KeywordQueryPlan,
    feedback_results: &[SearchResultDto],
    query: &str,
    hyde_text: Option<&str>,
) -> KeywordQueryPlan {
    if feedback_results.is_empty() {
        return base_plan;
    }

    let feedback_limit = feedback_results.len().min(12);
    let mut docs_by_term: HashMap<String, HashSet<String>> = HashMap::new();
    let mut feedback_doc_ids: HashSet<String> = HashSet::new();
    let mut feedback_scores: HashMap<String, f32> = HashMap::new();

    for (rank, result) in feedback_results.iter().take(feedback_limit).enumerate() {
        let doc_id = result
            .document_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| result.id.clone());
        feedback_doc_ids.insert(doc_id.clone());

        let rank_weight = 1.0 / ((rank + 1) as f32);
        let score_weight = result.score.max(0.05);
        let doc_weight = rank_weight * score_weight;

        let source_text = format!("{} {}", result.title, result.content);
        let tokens = tokenize_keyword_terms(&source_text);
        if tokens.is_empty() {
            continue;
        }

        let mut tf_counts: HashMap<String, usize> = HashMap::new();
        for term in tokens.iter() {
            *tf_counts.entry(term.clone()).or_insert(0) += 1;
        }
        let token_count = tokens.len() as f32;
        for (term, count) in tf_counts {
            docs_by_term
                .entry(term.clone())
                .or_default()
                .insert(doc_id.clone());
            let tf = count as f32 / token_count;
            *feedback_scores.entry(term).or_insert(0.0) += tf * doc_weight;
        }

        let doc_phrases = extract_phrase_terms(&tokens, 2, 2, 10);
        if !doc_phrases.is_empty() {
            let phrase_weight = doc_weight / (doc_phrases.len() as f32);
            for phrase in doc_phrases {
                docs_by_term
                    .entry(phrase.clone())
                    .or_default()
                    .insert(doc_id.clone());
                *feedback_scores.entry(phrase).or_insert(0.0) += phrase_weight * 0.35;
            }
        }
    }

    let mut final_scores: HashMap<String, f32> = HashMap::new();
    for (rank, term) in base_plan.terms.iter().enumerate() {
        let prior = 5.0 / (1.0 + (rank as f32 * 0.15));
        *final_scores.entry(term.clone()).or_insert(0.0) += prior;
    }
    let raw_query_terms = tokenize_keyword_terms(query);
    let query_terms: HashSet<String> = raw_query_terms.iter().cloned().collect();
    let informative_query_terms = {
        let selected = select_informative_terms(raw_query_terms.clone(), 16);
        if selected.is_empty() {
            raw_query_terms
        } else {
            selected
        }
    };
    let informative_query_stems: HashSet<String> = informative_query_terms
        .iter()
        .map(|term| stem_token(term))
        .collect();
    let hyde_stems: HashSet<String> = hyde_text
        .map(tokenize_keyword_terms)
        .unwrap_or_default()
        .into_iter()
        .map(|term| stem_token(&term))
        .collect();
    for term in query_terms.iter() {
        *final_scores.entry(term.clone()).or_insert(0.0) += 2.0;
    }
    if let Some(hyde) = hyde_text {
        for term in tokenize_keyword_terms(hyde) {
            *final_scores.entry(term).or_insert(0.0) += 0.6;
        }
    }

    let feedback_document_count = feedback_doc_ids.len().max(1);
    let feedback_document_count_f32 = feedback_document_count as f32;
    for (term, feedback_score) in feedback_scores {
        let document_frequency = docs_by_term.get(&term).map(|docs| docs.len()).unwrap_or(0);
        if !is_rm3_feedback_term_allowed(
            &term,
            document_frequency,
            feedback_document_count,
            &informative_query_stems,
            &hyde_stems,
        ) {
            continue;
        }
        let document_frequency = document_frequency as f32;
        let idf = ((feedback_document_count_f32 + 1.0) / (document_frequency + 1.0)).ln() + 1.0;
        *final_scores.entry(term).or_insert(0.0) += feedback_score * idf * 4.0;
    }

    let mut ranked: Vec<(String, f32)> = final_scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let terms: Vec<String> = ranked
        .into_iter()
        .map(|(term, _)| term)
        .filter(|term| is_keyword_term_specific(term, !hyde_stems.is_empty()))
        .take(24)
        .collect();
    let query_text = render_bm25_query_from_terms(&terms, 24).unwrap_or(base_plan.query);
    KeywordQueryPlan {
        terms,
        query: query_text,
    }
}

fn is_rm3_feedback_term_allowed(
    term: &str,
    document_frequency: usize,
    feedback_document_count: usize,
    informative_query_stems: &std::collections::HashSet<String>,
    hyde_stems: &std::collections::HashSet<String>,
) -> bool {
    let term_tokens = tokenize_keyword_terms(term);
    if term_tokens.is_empty() {
        return false;
    }

    let has_query_anchor = term_tokens.iter().any(|token| {
        let token_stem = stem_token(token);
        informative_query_stems.contains(&token_stem) || hyde_stems.contains(&token_stem)
    });
    let is_phrase = term_tokens.len() >= 2;
    let has_long_token = term_tokens.iter().any(|token| token.len() >= 5);
    if has_query_anchor {
        return is_phrase || has_long_token;
    }

    // Non-anchored feedback terms must be corroborated by multiple feedback docs.
    let coverage = if feedback_document_count == 0 {
        0.0
    } else {
        document_frequency as f32 / feedback_document_count as f32
    };
    document_frequency >= 2 && coverage <= 0.72 && (is_phrase || has_long_token)
}
