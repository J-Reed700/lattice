use crate::features::search::dto::SearchResultDto;
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::infrastructure::search::query_expansion::dictionaries::select_informative_terms;
use crate::shared::text_utils::safe_truncate;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use super::{
    extract_acronym_context_terms, extract_acronym_terms, stem_token, tokenize_keyword_terms,
};

pub(crate) fn filter_results_by_query_overlap(
    results: Vec<SearchResultDto>,
    query: &str,
    hyde_text: Option<&str>,
    followup_anchor_terms: Option<&HashSet<String>>,
) -> Vec<SearchResultDto> {
    filter_results_by_query_overlap_with_tuning(
        results,
        query,
        hyde_text,
        followup_anchor_terms,
        &RetrievalTuningSettingsDto::default(),
    )
}

pub(super) fn filter_results_by_query_overlap_with_tuning(
    results: Vec<SearchResultDto>,
    query: &str,
    hyde_text: Option<&str>,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> Vec<SearchResultDto> {
    if results.len() <= 1 {
        return results;
    }

    let mut query_terms: Vec<String> = select_informative_overlap_terms(
        extract_overlap_query_terms(query),
        query,
        hyde_text,
        &results,
    )
    .into_iter()
    .collect();
    if query_terms.is_empty() {
        return results;
    }
    let acronym_terms = extract_acronym_terms(query);
    let acronym_context_terms = extract_acronym_context_terms(query, hyde_text);
    for acronym in &acronym_terms {
        if !query_terms.contains(acronym) {
            query_terms.push(acronym.clone());
        }
    }
    let query_anchor_terms = filter_anchor_terms_by_candidate_coverage(
        extract_query_anchor_terms(query),
        &results,
        0.70,
    );
    for anchor in &query_anchor_terms {
        if !query_terms.contains(anchor) {
            query_terms.push(anchor.clone());
        }
    }
    let hyde_anchor_terms = filter_anchor_terms_by_candidate_coverage(
        extract_hyde_overlap_anchor_terms(query, hyde_text),
        &results,
        0.55,
    );
    let followup_anchor_terms = filter_followup_anchor_terms_by_candidate_coverage(
        followup_anchor_terms.cloned().unwrap_or_default(),
        &results,
    );
    let short_followup_query = !followup_anchor_terms.is_empty() && query_terms.len() <= 1;
    let require_followup_anchor = !followup_anchor_terms.is_empty() && !short_followup_query;
    let require_hyde_anchor = !hyde_anchor_terms.is_empty() && query_terms.len() >= 6;
    let require_query_anchor = false;
    let require_any_anchor = require_followup_anchor || require_hyde_anchor || require_query_anchor;
    let min_hyde_anchor_hits = if require_hyde_anchor { 2 } else { 1 };
    // Require stronger lexical agreement for multi-term queries to avoid
    // rescuing unrelated results based on a single weak token match.
    let min_hits = if !acronym_terms.is_empty() {
        1
    } else if query_terms.len() >= 2 {
        (tuning.overlap_min_hits_for_multi_term as usize).max(1)
    } else {
        1
    };
    debug!(
        query = %query,
        query_terms = ?query_terms,
        acronym_terms = ?acronym_terms,
        acronym_context_terms = ?acronym_context_terms,
        query_anchor_terms = ?query_anchor_terms,
        hyde_anchor_terms = ?hyde_anchor_terms,
        followup_anchor_terms = ?followup_anchor_terms,
        require_query_anchor = require_query_anchor,
        require_hyde_anchor = require_hyde_anchor,
        require_followup_anchor = require_followup_anchor,
        short_followup_query = short_followup_query,
        min_hyde_anchor_hits = min_hyde_anchor_hits,
        min_hits = min_hits,
        "Overlap filter configuration"
    );

    let mut filtered: Vec<SearchResultDto> = Vec::new();
    let mut fallback_results: Vec<(usize, usize, usize, usize, SearchResultDto)> = Vec::new();
    let mut best_fallback_hits = 0usize;
    for result in results {
        let mut haystack = String::new();
        haystack.push_str(&result.title);
        haystack.push(' ');
        haystack.push_str(&result.content);
        if let Some(path) = &result.path {
            haystack.push(' ');
            haystack.push_str(path);
        }
        let haystack_terms = tokenize_overlap_terms(&haystack);
        let acronym_context_hits = if acronym_context_terms.is_empty() {
            0
        } else {
            acronym_context_terms
                .iter()
                .filter(|term| haystack_terms.contains(term.as_str()))
                .count()
        };
        let hyde_anchor_hits = if hyde_anchor_terms.is_empty() {
            0
        } else {
            hyde_anchor_terms
                .iter()
                .filter(|term| haystack_terms.contains(term.as_str()))
                .count()
        };
        let query_anchor_hits = if query_anchor_terms.is_empty() {
            0
        } else {
            query_anchor_terms
                .iter()
                .filter(|term| haystack_terms.contains(term.as_str()))
                .count()
        };
        let followup_anchor_hits = if followup_anchor_terms.is_empty() {
            0
        } else {
            followup_anchor_terms
                .iter()
                .filter(|term| haystack_terms.contains(term.as_str()))
                .count()
        };
        let mut matched_terms: Vec<&str> = Vec::new();
        let mut hits: usize = 0;

        for term in &query_terms {
            let matched = if acronym_terms.contains(term.as_str()) {
                if haystack_terms.contains(term.as_str()) {
                    true
                } else {
                    acronym_context_hits >= 2
                }
            } else {
                haystack_terms.contains(term.as_str())
            };

            if matched {
                hits += 1;
                matched_terms.push(term.as_str());
            }
        }

        let anchor_gate_pass = !require_any_anchor
            || (require_followup_anchor && followup_anchor_hits >= 1)
            || (require_hyde_anchor && hyde_anchor_hits >= min_hyde_anchor_hits)
            || (require_query_anchor && query_anchor_hits >= 1);
        let keep = hits >= min_hits && anchor_gate_pass;
        debug!(
            result_id = %result.id,
            score = result.score,
            hits = hits,
            min_hits = min_hits,
            acronym_context_hits = acronym_context_hits,
            hyde_anchor_hits = hyde_anchor_hits,
            query_anchor_hits = query_anchor_hits,
            followup_anchor_hits = followup_anchor_hits,
            matched_terms = ?matched_terms,
            title = %safe_truncate(&result.title, 80),
            keep = keep,
            "Overlap filter decision"
        );

        if keep {
            filtered.push(result);
        } else {
            best_fallback_hits = best_fallback_hits.max(hits);
            fallback_results.push((
                hits,
                hyde_anchor_hits,
                query_anchor_hits,
                followup_anchor_hits,
                result,
            ));
        }
    }

    if filtered.is_empty() {
        // For follow-ups, preserve best context-anchored candidates before failing closed.
        if !followup_anchor_terms.is_empty() {
            let best_followup_anchor_hits = fallback_results
                .iter()
                .map(|(_, _, _, followup_anchor_hits, _)| *followup_anchor_hits)
                .max()
                .unwrap_or(0);
            if best_followup_anchor_hits > 0 {
                let rescued: Vec<SearchResultDto> = fallback_results
                    .iter()
                    .filter_map(
                        |(hits, hyde_anchor_hits, _, followup_anchor_hits, result)| {
                            ((*followup_anchor_hits == best_followup_anchor_hits)
                                && (*hits >= 1 || *hyde_anchor_hits >= 1))
                                .then_some(result.clone())
                        },
                    )
                    .collect();
                if !rescued.is_empty() {
                    debug!(
                        rescued_count = rescued.len(),
                        best_followup_anchor_hits = best_followup_anchor_hits,
                        short_followup_query = short_followup_query,
                        "Overlap filter rescued follow-up anchor matches"
                    );
                    return rescued;
                }
            }
        }

        let best_fallback_hyde_anchor_hits = fallback_results
            .iter()
            .filter(|(hits, _, _, _, _)| *hits == best_fallback_hits)
            .map(|(_, anchor_hits, _, _, _)| *anchor_hits)
            .max()
            .unwrap_or(0);
        let best_fallback_query_anchor_hits = fallback_results
            .iter()
            .filter(|(hits, _, _, _, _)| *hits == best_fallback_hits)
            .map(|(_, _, query_anchor_hits, _, _)| *query_anchor_hits)
            .max()
            .unwrap_or(0);
        let best_fallback_followup_anchor_hits = fallback_results
            .iter()
            .filter(|(hits, _, _, _, _)| *hits == best_fallback_hits)
            .map(|(_, _, _, followup_anchor_hits, _)| *followup_anchor_hits)
            .max()
            .unwrap_or(0);
        // Only rescue fallback matches when they satisfy the same lexical bar.
        let fallback_anchor_gate_pass = !require_any_anchor
            || (require_followup_anchor && best_fallback_followup_anchor_hits >= 1)
            || (require_hyde_anchor && best_fallback_hyde_anchor_hits >= min_hyde_anchor_hits)
            || (require_query_anchor && best_fallback_query_anchor_hits >= 1);
        if best_fallback_hits >= min_hits && fallback_anchor_gate_pass {
            let rescued: Vec<SearchResultDto> = fallback_results
                .into_iter()
                .filter_map(
                    |(hits, hyde_anchor_hits, query_anchor_hits, followup_anchor_hits, result)| {
                        let anchor_gate_pass = !require_any_anchor
                            || (require_followup_anchor
                                && followup_anchor_hits == best_fallback_followup_anchor_hits)
                            || (require_hyde_anchor
                                && hyde_anchor_hits == best_fallback_hyde_anchor_hits
                                && hyde_anchor_hits >= min_hyde_anchor_hits)
                            || (require_query_anchor
                                && query_anchor_hits == best_fallback_query_anchor_hits);
                        let keep = hits == best_fallback_hits && anchor_gate_pass;
                        keep.then_some(result)
                    },
                )
                .collect();
            debug!(
                rescued_count = rescued.len(),
                best_fallback_hits = best_fallback_hits,
                best_fallback_hyde_anchor_hits = best_fallback_hyde_anchor_hits,
                best_fallback_query_anchor_hits = best_fallback_query_anchor_hits,
                best_fallback_followup_anchor_hits = best_fallback_followup_anchor_hits,
                "Overlap filter rescued best lexical matches"
            );
            return rescued;
        }

        // For short and/or acronym-heavy queries, fail-closed when there is
        // no lexical support at all. Returning semantically-near neighbors here
        // can produce obviously irrelevant citations (e.g., acronym drift).
        let strict_fail_closed = !query_terms.is_empty()
            && (!acronym_terms.is_empty()
                || require_hyde_anchor
                || require_query_anchor
                || require_followup_anchor)
            && !short_followup_query;
        if strict_fail_closed {
            debug!(
                query_terms = ?query_terms,
                acronym_terms = ?acronym_terms,
                require_query_anchor = require_query_anchor,
                require_hyde_anchor = require_hyde_anchor,
                require_followup_anchor = require_followup_anchor,
                "Overlap filter found no lexical support for high-precision query; returning empty set"
            );
            return Vec::new();
        }

        // For broader natural-language queries, keep fail-open behavior so
        // minor token mismatches do not wipe out context entirely.
        let fallback_only: Vec<SearchResultDto> = fallback_results
            .into_iter()
            .map(|(_, _, _, _, result)| result)
            .collect();
        debug!(
            fallback_count = fallback_only.len(),
            "Overlap filter kept no results; returning unfiltered fallback set"
        );
        fallback_only
    } else {
        filtered
    }
}

pub(super) fn filter_followup_anchor_terms_by_candidate_coverage(
    followup_anchor_terms: HashSet<String>,
    results: &[SearchResultDto],
) -> HashSet<String> {
    if followup_anchor_terms.is_empty() || results.len() < 3 {
        return followup_anchor_terms;
    }

    let total = results.len() as f32;
    let mut doc_frequency: HashMap<String, usize> = followup_anchor_terms
        .iter()
        .map(|term| (term.clone(), 0))
        .collect();

    for result in results {
        let mut haystack = String::new();
        haystack.push_str(&result.title);
        haystack.push(' ');
        haystack.push_str(&result.content);
        if let Some(path) = &result.path {
            haystack.push(' ');
            haystack.push_str(path);
        }
        let haystack_terms = tokenize_overlap_terms(&haystack);
        for term in &followup_anchor_terms {
            if haystack_terms.contains(term.as_str()) {
                *doc_frequency.entry(term.clone()).or_insert(0) += 1;
            }
        }
    }

    // Drop anchors that match most candidates; they do not help discriminate.
    followup_anchor_terms
        .into_iter()
        .filter(|term| {
            let frequency = *doc_frequency.get(term).unwrap_or(&0) as f32;
            let coverage = if total > 0.0 { frequency / total } else { 0.0 };
            coverage < 0.75
        })
        .collect()
}

fn filter_anchor_terms_by_candidate_coverage(
    anchor_terms: HashSet<String>,
    results: &[SearchResultDto],
    max_coverage: f32,
) -> HashSet<String> {
    if anchor_terms.is_empty() || results.len() < 3 {
        return anchor_terms;
    }

    let total = results.len() as f32;
    let mut doc_frequency: HashMap<String, usize> =
        anchor_terms.iter().map(|term| (term.clone(), 0)).collect();

    for result in results {
        let mut haystack = String::new();
        haystack.push_str(&result.title);
        haystack.push(' ');
        haystack.push_str(&result.content);
        if let Some(path) = &result.path {
            haystack.push(' ');
            haystack.push_str(path);
        }
        let haystack_terms = tokenize_overlap_terms(&haystack);
        for term in &anchor_terms {
            if haystack_terms.contains(term.as_str()) {
                *doc_frequency.entry(term.clone()).or_insert(0) += 1;
            }
        }
    }

    anchor_terms
        .into_iter()
        .filter(|term| {
            let frequency = *doc_frequency.get(term).unwrap_or(&0) as f32;
            let coverage = if total > 0.0 { frequency / total } else { 0.0 };
            coverage <= max_coverage
        })
        .collect()
}

pub(super) fn select_informative_overlap_terms(
    query_terms: Vec<String>,
    query: &str,
    hyde_text: Option<&str>,
    results: &[SearchResultDto],
) -> Vec<String> {
    if query_terms.is_empty() {
        return query_terms;
    }
    if results.is_empty() {
        return query_terms;
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct TermStats {
        doc_frequency: usize,
        weighted_presence: f32,
    }

    let mut term_stats: HashMap<String, TermStats> = HashMap::new();
    let query_has_question_suffix = query.trim_end().ends_with('?');
    let query_positions: HashMap<String, usize> = tokenize_keyword_terms(query)
        .into_iter()
        .enumerate()
        .fold(HashMap::new(), |mut acc, (idx, token)| {
            acc.entry(token).or_insert(idx);
            acc
        });
    let hyde_stems: HashSet<String> = hyde_text
        .map(tokenize_keyword_terms)
        .unwrap_or_default()
        .into_iter()
        .map(|token| stem_token(&token))
        .collect();
    let has_hyde_anchor = !hyde_stems.is_empty();
    let max_result_score = results
        .iter()
        .map(|result| result.score.max(0.0))
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    for (rank, result) in results.iter().enumerate() {
        let mut haystack = String::new();
        haystack.push_str(&result.title);
        haystack.push(' ');
        haystack.push_str(&result.content);
        if let Some(path) = &result.path {
            haystack.push(' ');
            haystack.push_str(path);
        }
        let haystack_terms = tokenize_overlap_terms(&haystack);
        let rank_weight = 1.0 / ((rank + 1) as f32);
        let score_weight = (result.score.max(0.0) / max_result_score).clamp(0.0, 1.0);
        let presence_weight = 0.5 * rank_weight + 0.5 * score_weight;

        for term in query_terms.iter() {
            if haystack_terms.contains(term.as_str()) {
                let entry = term_stats.entry(term.clone()).or_default();
                entry.doc_frequency += 1;
                entry.weighted_presence += presence_weight;
            }
        }
    }

    let total_docs = results.len() as f32;
    let max_presence = term_stats
        .values()
        .map(|stats| stats.weighted_presence)
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);
    let term_cap = query_terms.len().clamp(2, 8);

    let mut ranked: Vec<(String, f32, f32)> = query_terms
        .into_iter()
        .map(|term| {
            let stats = term_stats.get(&term).copied().unwrap_or_default();
            let df = stats.doc_frequency as f32;
            if df <= 0.0 {
                return (term, 0.0, 1.0);
            }
            let coverage = if total_docs > 0.0 {
                df / total_docs
            } else {
                0.0
            };
            let idf = ((total_docs + 1.0) / (df + 1.0)).ln() + 1.0;
            let concentration = (stats.weighted_presence / max_presence).clamp(0.0, 1.0);
            let length_factor = 1.0 + 0.08 * ((term.len() as f32) + 1.0).ln();
            let query_position = query_positions.get(&term).copied().unwrap_or(usize::MAX);
            let first_token_question_penalty =
                if query_has_question_suffix && query_position == 0 && term.len() <= 5 {
                    0.28
                } else {
                    1.0
                };
            let structural_coverage_penalty = if query_has_question_suffix
                && query_position == 0
                && term.len() <= 5
                && coverage >= 0.35
            {
                0.4
            } else {
                1.0
            };
            let hyde_alignment = if has_hyde_anchor {
                let term_stem = stem_token(&term);
                if hyde_stems.contains(&term_stem) {
                    1.25
                } else {
                    0.45
                }
            } else {
                1.0
            };
            let score = idf
                * (0.55 + 0.45 * concentration)
                * length_factor
                * first_token_question_penalty
                * structural_coverage_penalty
                * hyde_alignment;
            (term, score, coverage)
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let best_score = ranked.first().map(|(_, score, _)| *score).unwrap_or(0.0);
    if best_score <= f32::EPSILON {
        return ranked
            .into_iter()
            .take(term_cap.min(2))
            .map(|(term, _, _)| term)
            .collect();
    }

    let filtered: Vec<String> = ranked
        .iter()
        .filter_map(|(term, score, coverage)| {
            let within_relative_band = *score >= best_score * 0.35;
            let weak_high_coverage = *coverage >= 0.75 && *score < best_score * 0.72;
            let high_coverage_requires_stronger_signal =
                *coverage <= 0.72 || *score >= best_score * 0.75;
            let short_ubiquitous_term = term.len() <= 4 && *coverage >= 0.45;
            let low_specificity_term =
                term.len() <= 5 && *coverage >= 0.35 && *score < best_score * 0.65;
            let query_position = query_positions.get(term).copied().unwrap_or(usize::MAX);
            let suppress_question_opener = query_has_question_suffix
                && query_position == 0
                && term.len() <= 5
                && *coverage >= 0.35;
            (within_relative_band
                && high_coverage_requires_stronger_signal
                && !weak_high_coverage
                && !short_ubiquitous_term
                && !low_specificity_term
                && !suppress_question_opener)
                .then_some(term.clone())
        })
        .take(term_cap)
        .collect();

    if filtered.is_empty() {
        ranked
            .into_iter()
            .take(term_cap.min(2))
            .map(|(term, _, _)| term)
            .collect()
    } else {
        filtered
    }
}

pub(crate) fn extract_overlap_query_terms(query: &str) -> Vec<String> {
    let raw_tokens = tokenize_keyword_terms(query);
    let candidates = {
        let selected = select_informative_terms(raw_tokens.clone(), 14);
        if selected.is_empty() {
            raw_tokens
        } else {
            selected
        }
    };
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|term| term.len() >= 3)
        .filter_map(|term| {
            if seen.insert(term.clone()) {
                Some(term)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn tokenize_overlap_terms(text: &str) -> HashSet<String> {
    let mut terms: HashSet<String> = HashSet::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            terms.insert(current);
            current = String::new();
        }
    }
    if !current.is_empty() {
        terms.insert(current);
    }

    terms
}

fn extract_hyde_overlap_anchor_terms(query: &str, hyde_text: Option<&str>) -> HashSet<String> {
    let mut terms = HashSet::new();
    let Some(hyde) = hyde_text else {
        return terms;
    };

    let query_terms = tokenize_overlap_terms(query);
    let raw_tokens = tokenize_keyword_terms(hyde);
    let informative_tokens = {
        let selected = select_informative_terms(raw_tokens.clone(), 28);
        if selected.is_empty() {
            raw_tokens
        } else {
            selected
        }
    };
    for term in informative_tokens {
        if term.len() < 6
            || query_terms.contains(&term)
            || !term.chars().any(|c| c.is_ascii_alphabetic())
        {
            continue;
        }
        terms.insert(term);
        if terms.len() >= 20 {
            break;
        }
    }

    terms
}

pub(super) fn extract_query_anchor_terms(query: &str) -> HashSet<String> {
    let raw_tokens = tokenize_keyword_terms(query);
    let informative = {
        let selected = select_informative_terms(raw_tokens.clone(), 8);
        if selected.is_empty() {
            raw_tokens
        } else {
            selected
        }
    };
    let long_terms: HashSet<String> = informative
        .iter()
        .filter(|term| term.len() >= 7)
        .cloned()
        .collect();
    if !long_terms.is_empty() {
        return long_terms;
    }

    informative
        .into_iter()
        .filter(|term| term.len() >= 6)
        .collect()
}
