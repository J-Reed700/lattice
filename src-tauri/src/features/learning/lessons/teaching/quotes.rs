//! Lesson quotation provenance: check the author's quotations against the
//! supplied excerpts, restore their typography, and re-anchor a quotation the
//! author could not match to the nearest real passage.
use serde_json::{json, Value};

pub(super) fn lesson_quote_checks(candidate: &Value, authoring: &Value) -> Vec<Value> {
    let Some(sources) = authoring.get("sources").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut failures = Vec::new();
    for field in ["blocks", "questions"] {
        for (ordinal, item) in candidate
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let index = item.get("sourceIndex").and_then(Value::as_u64);
            let quote = item
                .get("quote")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let error = if sources.is_empty() {
                (index.is_some() || !quote.trim().is_empty())
                    .then(|| "Topic-based material must not invent source citations.".to_owned())
            } else {
                match index
                    .and_then(|index| {
                        sources.iter().find(|source| {
                            source.get("sourceIndex").and_then(Value::as_u64) == Some(index)
                        })
                    })
                    .and_then(|source| source.get("excerpt"))
                    .and_then(Value::as_str)
                {
                    Some(excerpt) => crate::features::learning::generation::validate_excerpt_quote(
                        quote, excerpt,
                    )
                    .err()
                    .map(|e| e.to_string()),
                    None => Some("The citation must select a supplied sourceIndex.".to_owned()),
                }
            };
            if let Some(error) = error {
                failures.push(json!({"path":format!("{field}[{ordinal}]"),"sourceIndex":index,"quote":quote,"error":error}));
            }
        }
    }
    failures
}

pub(super) fn restore_lesson_quotes(candidate: &mut Value, authoring: &Value) -> usize {
    let Some(sources) = authoring.get("sources").and_then(Value::as_array) else {
        return 0;
    };
    let mut restored = 0;
    for field in ["blocks", "questions"] {
        for item in candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let Some(index) = item.get("sourceIndex").and_then(Value::as_u64) else {
                continue;
            };
            let Some(excerpt) = sources
                .iter()
                .find(|source| source.get("sourceIndex").and_then(Value::as_u64) == Some(index))
                .and_then(|source| source.get("excerpt"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let Some(quote) = item.get("quote").and_then(Value::as_str) else {
                continue;
            };
            if crate::features::learning::generation::validate_excerpt_quote(quote, excerpt).is_ok()
            {
                continue;
            }
            let Some(exact) =
                crate::features::learning::outline_citations::exact_source_quote(excerpt, quote)
            else {
                continue;
            };
            if let Some(quote) = item.get_mut("quote") {
                *quote = json!(exact);
                restored += 1;
            }
        }
    }
    restored
}

fn content_terms(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        .map(str::to_lowercase)
        .collect()
}

/// Whole-sentence byte ranges of `text`; a line break also ends a sentence.
fn sentence_spans(text: &str) -> Vec<(usize, usize)> {
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut start = 0;
    for (position, (offset, character)) in characters.iter().enumerate() {
        let ends = match character {
            '\n' => true,
            '.' | '!' | '?' => characters
                .get(position + 1)
                .is_none_or(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if ends {
            let end = offset + character.len_utf8();
            if !text[start..end].trim().is_empty() {
                spans.push((start, end));
            }
            start = end;
        }
    }
    if !text[start..].trim().is_empty() {
        spans.push((start, text.len()));
    }
    spans
}

/// The run of one to four whole sentences of `excerpt` that shares the largest
/// share of `target`'s terms and passes the quote rules. This chooses
/// provenance text for a citation, never evidence: every factual claim is
/// checked separately against independently retrieved passages before
/// publication.
fn nearest_passage_span(excerpt: &str, target: &str) -> Option<(f64, usize, String)> {
    let wanted = content_terms(target);
    if wanted.is_empty() {
        return None;
    }
    let spans = sentence_spans(excerpt);
    let mut best: Option<(f64, usize, String)> = None;
    for first in 0..spans.len() {
        for last in first..spans.len().min(first + 4) {
            let (Some(&(start, _)), Some(&(_, end))) = (spans.get(first), spans.get(last)) else {
                continue;
            };
            let Some(text) = excerpt.get(start..end).map(str::trim) else {
                continue;
            };
            if text.chars().count() > crate::features::learning::generation::MAX_QUOTE_CHARS {
                break;
            }
            if crate::features::learning::generation::validate_excerpt_quote(text, excerpt).is_err()
            {
                continue;
            }
            let shared = content_terms(text).intersection(&wanted).count();
            let share = shared as f64 / wanted.len() as f64;
            if best.as_ref().is_none_or(|(top, _, kept)| {
                share > *top || (share == *top && text.len() < kept.len())
            }) {
                best = Some((share, shared, text.to_owned()));
            }
        }
    }
    best
}

/// Every text of a lesson block or question except its citation.
fn item_text(item: &Value) -> String {
    let mut parts = Vec::new();
    for (key, value) in item.as_object().into_iter().flatten() {
        if matches!(key.as_str(), "quote" | "kind" | "id") {
            continue;
        }
        match value {
            Value::String(text) => parts.push(text.clone()),
            Value::Array(values) => {
                parts.extend(values.iter().filter_map(Value::as_str).map(str::to_owned))
            }
            _ => {}
        }
    }
    parts.join(" ")
}

/// After the author has had its chance to correct its quotations, cite the
/// real passage it meant instead of discarding the lesson: the span closest
/// to its quotation (at least 60% of its terms) in the cited or another
/// supplied excerpt, otherwise the span that shares the most terms (at least
/// two) with the item itself. Citations are provenance; the final review below and claim
/// verification still check every claim before anything is published.
pub(super) fn reanchor_lesson_quotes(candidate: &mut Value, authoring: &Value) -> usize {
    let Some(sources) = authoring.get("sources").and_then(Value::as_array) else {
        return 0;
    };
    let excerpts: Vec<(u64, &str)> = sources
        .iter()
        .filter_map(|source| {
            Some((
                source.get("sourceIndex")?.as_u64()?,
                source.get("excerpt")?.as_str()?,
            ))
        })
        .collect();
    if excerpts.is_empty() {
        return 0;
    }
    let mut changed = 0;
    for field in ["blocks", "questions"] {
        for item in candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let index = item.get("sourceIndex").and_then(Value::as_u64);
            let quote = item
                .get("quote")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let valid = index
                .and_then(|index| excerpts.iter().find(|(source, _)| *source == index))
                .is_some_and(|(_, excerpt)| {
                    crate::features::learning::generation::validate_excerpt_quote(&quote, excerpt)
                        .is_ok()
                });
            if valid {
                continue;
            }
            let nearest = |target: &str, minimum: f64, terms: usize| {
                excerpts
                    .iter()
                    .filter_map(|(source, excerpt)| {
                        nearest_passage_span(excerpt, target)
                            .filter(|(_, shared, _)| *shared >= terms)
                            .map(|(share, _, text)| (share, *source, text))
                    })
                    // The cited source wins a tie.
                    .max_by(|a, b| {
                        a.0.partial_cmp(&b.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| (Some(a.1) == index).cmp(&(Some(b.1) == index)))
                    })
                    .filter(|(share, _, _)| *share >= minimum && *share > 0.0)
            };
            let own_text = item_text(item);
            let Some((_, source, text)) = (!quote.trim().is_empty())
                .then(|| nearest(&quote, 0.6, 1))
                .flatten()
                .or_else(|| nearest(&own_text, 0.0, 2))
            else {
                continue;
            };
            if let Some(object) = item.as_object_mut() {
                object.insert("sourceIndex".into(), json!(source));
                object.insert("quote".into(), json!(text));
                changed += 1;
                tracing::info!(
                    field,
                    source,
                    "Cited the nearest real passage for a quotation the author could not match"
                );
            }
        }
    }
    changed
}
