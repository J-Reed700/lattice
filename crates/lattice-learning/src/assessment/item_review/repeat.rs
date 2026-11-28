//! Deterministic structural and prompt-overlap detection and topic extraction
//! for [`super::Item`] repeat checks.
use super::Item;
use crate::assessment_engine::{LearningAssessmentKey as Key, LearningItemFormat};
use std::collections::{HashMap, HashSet};

/// Formats a compact, format-aware prompt descriptor for repeat detection so
/// structured items (`Matching`, `Cloze`, `Ordering`, `MultiSelect`) expose
/// their actual paired decisions / blank answers alongside their stem.
pub(crate) fn item_repeat_descriptor(candidate: &Item) -> String {
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::Matching, Key::Matching(pairs)) if !candidate.left.is_empty() => {
            let summary: Vec<String> = candidate
                .left
                .iter()
                .zip(pairs)
                .map(|(left, right)| {
                    let answer = candidate
                        .options
                        .get(*right)
                        .map(String::as_str)
                        .unwrap_or_default();
                    format!("{left} -> {answer}")
                })
                .collect();
            format!("{} [pairs: {}]", candidate.prompt, summary.join("; "))
        }
        (LearningItemFormat::Cloze, Key::Cloze(blanks)) if !blanks.is_empty() => {
            let summary: Vec<&str> = blanks
                .iter()
                .filter_map(|key| key.variants.first().map(String::as_str))
                .collect();
            format!("{} [blanks: {}]", candidate.prompt, summary.join(" | "))
        }
        (LearningItemFormat::Ordering, Key::Ordering(order)) if !order.is_empty() => {
            format!("{} [order: {}]", candidate.prompt, order.join(" -> "))
        }
        (LearningItemFormat::MultiSelect, Key::Choices(indices)) if !indices.is_empty() => {
            let summary: Vec<&str> = indices
                .iter()
                .filter_map(|index| candidate.options.get(*index).map(String::as_str))
                .collect();
            format!("{} [picks: {}]", candidate.prompt, summary.join("; "))
        }
        _ => candidate.prompt.clone(),
    }
}

fn repeat_content_tokens(text: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "with",
        "from",
        "when",
        "that",
        "this",
        "each",
        "your",
        "into",
        "have",
        "been",
        "will",
        "should",
        "which",
        "about",
        "other",
        "scenario",
        "connector",
        "server",
        "request",
        "requests",
        "response",
        "responses",
        "client",
        "vendor",
        "match",
        "following",
        "based",
        "pairs",
        "blanks",
        "order",
        "picks",
    ];
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .map(|w| w.trim_matches('-').to_ascii_lowercase())
        .filter(|w| w.len() >= 4 && !STOP.contains(&w.as_str()))
        .collect()
}

fn strip_descriptor_tags(descriptor: &str) -> String {
    descriptor
        .replace("[pairs:", " ")
        .replace("[blanks:", " ")
        .replace("[order:", " ")
        .replace("[picks:", " ")
        .replace(']', " ")
}

fn stem_without_tag(descriptor: &str) -> &str {
    for tag in [" [pairs: ", " [blanks: ", " [order: ", " [picks: "] {
        if let Some((prompt, _)) = descriptor.split_once(tag) {
            return prompt;
        }
    }
    descriptor
}

fn extract_bracket_section<'a>(descriptor: &'a str, tag: &str) -> Option<&'a str> {
    let (_, rest) = descriptor.split_once(tag)?;
    rest.strip_suffix(']')
}

/// Detects deterministic structural and stem-level overlap for candidate items
/// against previously held/authored descriptors.
pub(crate) fn repeat_overlap_reason(
    candidate: &Item,
    known_descriptors: &[String],
) -> Option<String> {
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::Matching, Key::Matching(pairs)) if !candidate.left.is_empty() => {
            if let Some(reason) = matching_overlap_reason(candidate, pairs, known_descriptors) {
                return Some(reason);
            }
        }
        (LearningItemFormat::Cloze, Key::Cloze(blanks)) if !blanks.is_empty() => {
            if let Some(reason) = cloze_overlap_reason(candidate, blanks, known_descriptors) {
                return Some(reason);
            }
        }
        _ => {}
    }

    let cand_stem = repeat_content_tokens(&candidate.prompt);
    let is_open_written = matches!(
        candidate.format,
        LearningItemFormat::Explanation | LearningItemFormat::Artifact
    );
    for held in known_descriptors {
        let held_stem = repeat_content_tokens(stem_without_tag(held));
        let shared_stem = cand_stem.intersection(&held_stem).count();
        let min_stem = cand_stem.len().min(held_stem.len());
        if is_open_written && min_stem >= 10 && shared_stem * 4 >= min_stem * 3 {
            return Some(
                "It asks for the same written explanation as an existing question.".into(),
            );
        }
        if min_stem >= 14 && shared_stem * 20 >= min_stem * 17 {
            return Some(
                "It repeats the same question scenario and decision as an existing question."
                    .into(),
            );
        }
    }
    None
}

fn matching_overlap_reason(
    candidate: &Item,
    pairs: &[usize],
    known_descriptors: &[String],
) -> Option<String> {
    let cand_pairs: Vec<String> = candidate
        .left
        .iter()
        .zip(pairs)
        .map(|(left, right)| {
            format!(
                "{left} -> {}",
                candidate
                    .options
                    .get(*right)
                    .map(String::as_str)
                    .unwrap_or_default()
            )
        })
        .collect();
    let cand_all = repeat_content_tokens(&cand_pairs.join(" "));
    let cand_right: Vec<HashSet<String>> = pairs
        .iter()
        .filter_map(|r| candidate.options.get(*r))
        .map(|opt| repeat_content_tokens(opt))
        .collect();
    for held in known_descriptors {
        let Some(pair_str) = extract_bracket_section(held, " [pairs: ") else {
            continue;
        };
        let held_all = repeat_content_tokens(pair_str);
        let shared_all = cand_all.intersection(&held_all).count();
        let min_all = cand_all.len().min(held_all.len());
        if min_all >= 4 && shared_all * 20 >= min_all * 11 {
            return Some("It asks for the same matching decisions as an existing question.".into());
        }
        let held_right: Vec<HashSet<String>> = pair_str
            .split("; ")
            .filter_map(|p| p.split_once(" -> ").map(|(_, r)| repeat_content_tokens(r)))
            .collect();
        let matching_outcomes = cand_right
            .iter()
            .filter(|cr| {
                !cr.is_empty()
                    && held_right.iter().any(|hr| {
                        let shared = cr.intersection(hr).count();
                        let min_side = cr.len().min(hr.len());
                        shared >= 2 && min_side >= 2 && shared * 2 >= min_side
                    })
            })
            .count();
        if matching_outcomes >= 2 && shared_all >= 3 {
            return Some("It tests the same matching outcomes as an existing question.".into());
        }
    }
    None
}

fn cloze_overlap_reason(
    candidate: &Item,
    blanks: &[crate::assessment_engine::LearningTextKey],
    known_descriptors: &[String],
) -> Option<String> {
    let cand_blanks: Vec<String> = blanks
        .iter()
        .filter_map(|b| b.variants.first())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let cand_tokens = repeat_content_tokens(&cand_blanks.join(" "));
    let cand_stem = repeat_content_tokens(&candidate.prompt);
    for held in known_descriptors {
        let Some((held_prompt, blank_str)) = held
            .split_once(" [blanks: ")
            .and_then(|(prompt, rest)| rest.strip_suffix(']').map(|b| (prompt, b)))
        else {
            continue;
        };
        let held_blanks: Vec<String> = blank_str
            .split(" | ")
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let exact_shared = cand_blanks
            .iter()
            .filter(|b| held_blanks.contains(b))
            .count();
        let held_stem = repeat_content_tokens(held_prompt);
        let shared_stem = cand_stem.intersection(&held_stem).count();
        let min_stem = cand_stem.len().min(held_stem.len());
        if cand_blanks.len() == 1 && held_blanks.len() == 1 {
            if exact_shared == 1
                && min_stem >= 2
                && shared_stem >= 2
                && shared_stem * 5 >= min_stem * 2
            {
                return Some(
                    "It fills the exact same blank answer in the same scenario as an existing cloze question."
                        .into(),
                );
            }
            continue;
        }
        if exact_shared >= 2 {
            return Some(
                "It fills the exact same blank answers as an existing cloze question.".into(),
            );
        }
        let held_tokens = repeat_content_tokens(blank_str);
        let min_toks = cand_tokens.len().min(held_tokens.len());
        let shared_toks = cand_tokens.intersection(&held_tokens).count();
        if min_toks >= 2 && shared_toks * 2 >= min_toks && shared_stem >= 1 {
            return Some(
                "It tests the same cloze concept and answers as an existing question.".into(),
            );
        }
    }
    None
}

/// Identifies over-represented topic terms (>= 2 existing items) so authoring rounds
/// steer new items onto distinct untested concepts in the lesson passages.
pub(crate) fn overrepresented_topics(descriptors: &[String]) -> Vec<String> {
    if descriptors.len() < 2 {
        return Vec::new();
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for descriptor in descriptors {
        for token in repeat_content_tokens(&strip_descriptor_tags(descriptor)) {
            *counts.entry(token).or_insert(0) += 1;
        }
    }
    let mut frequent: Vec<(usize, String)> = counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(term, count)| (count, term))
        .collect();
    frequent.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    frequent.into_iter().take(4).map(|(_, term)| term).collect()
}
