//! Lesson quotation provenance: check the author's quotations against the
//! supplied excerpts, restore their typography, and re-anchor a quotation the
//! author could not match to the nearest real passage.
use crate::dto::LearningSourceDto;
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::FindingSite;
use lattice_core::shared::text::sentences;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A source quotation that matches no saved reference, at the part that
/// cites it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteCheck {
    pub path: String,
    pub source_index: Option<u64>,
    pub quote: String,
    pub error: String,
}

/// The sources a citation may name, each with the index that names it and its text.
pub(crate) type Table<'a> = Vec<(u64, &'a str)>;

/// The table of the excerpts an authoring context supplies; `None` when it has no sources.
pub(crate) fn authored(authoring: &Value) -> Option<Table<'_>> {
    let sources = authoring.get("sources").and_then(Value::as_array)?;
    Some(
        sources
            .iter()
            .filter_map(|source| {
                Some((
                    source.get("sourceIndex")?.as_u64()?,
                    source.get("excerpt")?.as_str()?,
                ))
            })
            .collect(),
    )
}

/// The table of a lesson's numbered sources: `sourceIndex` i names the i-th.
pub(crate) fn numbered(sources: &[crate::dto::LearningSourceDto]) -> Table<'_> {
    sources
        .iter()
        .enumerate()
        .map(|(index, source)| (index as u64, source.excerpt.as_str()))
        .collect()
}

/// The saved source a numbered one is: its whole text, where the writer was shown passages of it.
pub(crate) fn saved<'a>(
    references: &'a ReferenceCollection<'_>,
    source: &'a LearningSourceDto,
) -> &'a LearningSourceDto {
    references
        .sources
        .iter()
        .find(|saved| saved.id == source.id)
        .unwrap_or(source)
}

/// What a lesson's quotations are checked against: `sourceIndex` i names the
/// whole saved text of the i-th numbered source, so a citation may quote any
/// part of the source it names, not only the passages the writer was shown.
pub(crate) fn checked<'a>(
    numbered: &'a [LearningSourceDto],
    references: &'a ReferenceCollection<'_>,
) -> Table<'a> {
    numbered
        .iter()
        .enumerate()
        .map(|(index, source)| (index as u64, saved(references, source).excerpt.as_str()))
        .collect()
}

/// Whether `quote` is a quotation of the text `index` names.
fn holds(table: &Table, index: Option<u64>, quote: &str) -> bool {
    excerpt_of(table, index)
        .is_some_and(|excerpt| crate::generation::validate_excerpt_quote(quote, excerpt).is_ok())
}

/// The text `index` names.
fn excerpt_of<'a>(table: &Table<'a>, index: Option<u64>) -> Option<&'a str> {
    let index = index?;
    table
        .iter()
        .find(|(named, _)| *named == index)
        .map(|(_, text)| *text)
}

/// Why a quotation fails against the supplied excerpts; `None` when it holds.
fn quote_error(sources: &Table, index: Option<u64>, quote: &str) -> Option<String> {
    if sources.is_empty() {
        return (index.is_some() || !quote.trim().is_empty())
            .then(|| "Topic-based material must not invent source citations.".to_owned());
    }
    match excerpt_of(sources, index) {
        Some(excerpt) => crate::generation::validate_excerpt_quote(quote, excerpt)
            .err()
            .map(|e| e.to_string()),
        None => Some("The citation must select a supplied sourceIndex.".to_owned()),
    }
}

/// The failures of a candidate cited against what `authoring` supplies.
pub(super) fn lesson_quote_checks(candidate: &Value, authoring: &Value) -> Vec<QuoteCheck> {
    authored(authoring).map_or_else(Vec::new, |table| quote_checks_in(candidate, &table))
}

type SiteOf = fn(usize) -> FindingSite;

pub(crate) fn quote_checks_in(candidate: &Value, sources: &Table) -> Vec<QuoteCheck> {
    let mut failures = Vec::new();
    let fields: [(&str, SiteOf); 2] = [
        ("blocks", FindingSite::Section),
        ("questions", FindingSite::Question),
    ];
    for (field, site) in fields {
        for (ordinal, item) in candidate
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let path = site(ordinal).path().unwrap_or_default();
            let index = item.get("sourceIndex").and_then(Value::as_u64);
            let quote = item
                .get("quote")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(error) = quote_error(sources, index, quote) {
                failures.push(QuoteCheck {
                    path: path.clone(),
                    source_index: index,
                    quote: quote.to_owned(),
                    error,
                });
            }
            // A sentence the writer cited carries its own source and quote.
            for (position, cited) in item
                .get("citations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let index = cited.get("sourceIndex").and_then(Value::as_u64);
                let quote = cited
                    .get("quote")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(error) = quote_error(sources, index, quote) {
                    failures.push(QuoteCheck {
                        path: path.clone(),
                        source_index: index,
                        quote: quote.to_owned(),
                        error: format!("Citation {} of this part: {error}", position + 1),
                    });
                }
            }
        }
    }
    failures
}

/// The exact words of the excerpt `index` names that a quotation of it
/// matches but for typography; `None` when it is those words already or
/// matches none.
fn restored_quote(sources: &Table, index: Option<u64>, quote: &str) -> Option<String> {
    let excerpt = excerpt_of(sources, index)?;
    crate::outline_citations::exact_source_quote(excerpt, quote)
        .filter(|exact| *exact != quote)
        .map(str::to_owned)
}

pub(super) fn restore_quotes_in(candidate: &mut Value, sources: &Table) -> usize {
    let mut restored = 0;
    let mut restore = |part: &mut Value| {
        let exact = restored_quote(
            sources,
            part.get("sourceIndex").and_then(Value::as_u64),
            part.get("quote")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        if let (Some(exact), Some(quote)) = (exact, part.get_mut("quote")) {
            *quote = json!(exact);
            restored += 1;
        }
    };
    for field in ["blocks", "questions"] {
        for item in candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            restore(item);
            for cited in item
                .get_mut("citations")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                restore(cited);
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

/// The run of one to four whole sentences of `excerpt` that shares the largest
/// share of `target`'s terms and passes the quote rules. This chooses
/// provenance text for a citation, never evidence: every factual claim is
/// checked separately against independently retrieved passages before
/// publication.
fn nearest_passage_span(excerpt: &str, target: &str) -> Option<(f64, usize, String)> {
    let wanted_index: std::collections::HashMap<String, usize> = content_terms(target)
        .into_iter()
        .enumerate()
        .map(|(idx, term)| (term, idx))
        .collect();
    let wanted_len = wanted_index.len();
    if wanted_len == 0 {
        return None;
    }
    let spans = sentences(excerpt);
    let sentence_hits: Vec<Vec<usize>> = spans
        .iter()
        .map(|&(start, end)| {
            let mut hits = Vec::new();
            if let Some(slice) = excerpt.get(start..end) {
                for word in slice.split(|c: char| !c.is_alphanumeric()) {
                    if word.chars().count() < 4 {
                        continue;
                    }
                    if let Some(&idx) = wanted_index.get(&word.to_lowercase()) {
                        if !hits.contains(&idx) {
                            hits.push(idx);
                        }
                    }
                }
            }
            hits
        })
        .collect();
    let mut seen = vec![0usize; wanted_len];
    let mut stamp = 0usize;
    let mut best: Option<(f64, usize, String)> = None;
    for first in 0..spans.len() {
        let end_limit = spans.len().min(first + 4);
        let any_hit = sentence_hits
            .get(first..end_limit)
            .is_some_and(|window| window.iter().any(|hits| !hits.is_empty()));
        if !any_hit {
            continue;
        }
        stamp = stamp.saturating_add(1);
        let mut shared = 0usize;
        for last in first..end_limit {
            if let Some(hits) = sentence_hits.get(last) {
                for &idx in hits {
                    if let Some(slot) = seen.get_mut(idx) {
                        if *slot != stamp {
                            *slot = stamp;
                            shared += 1;
                        }
                    }
                }
            }
            if shared == 0 {
                continue;
            }
            let (Some(&(start, _)), Some(&(_, end))) = (spans.get(first), spans.get(last)) else {
                continue;
            };
            let Some(text) = excerpt.get(start..end).map(str::trim) else {
                continue;
            };
            if text.chars().count() > crate::generation::MAX_QUOTE_CHARS {
                break;
            }
            let share = shared as f64 / wanted_len as f64;
            let beats_best = best.as_ref().is_none_or(|(top, _, kept)| {
                share > *top || (share == *top && text.len() < kept.len())
            });
            if !beats_best {
                continue;
            }
            if crate::generation::validate_excerpt_quote(text, excerpt).is_err() {
                continue;
            }
            best = Some((share, shared, text.to_owned()));
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

/// The nearest span of the supplied excerpts to `target` that shares at least
/// `terms` of its terms and the share `minimum` of them, with the source it is
/// in; the cited source `index` wins a tie.
fn nearest(
    excerpts: &Table,
    index: Option<u64>,
    (target, minimum, terms): (&str, f64, usize),
) -> Option<(f64, u64, String)> {
    excerpts
        .iter()
        .filter_map(|(source, excerpt)| {
            nearest_passage_span(excerpt, target)
                .filter(|(_, shared, _)| *shared >= terms)
                .map(|(share, _, text)| (share, *source, text))
        })
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (Some(a.1) == index).cmp(&(Some(b.1) == index)))
        })
        .filter(|(share, _, _)| *share >= minimum && *share > 0.0)
}

/// The source and passage a quotation the author could not match is cited to
/// instead: the span of the passages `shown` closest to the quotation (at
/// least 60% of its terms), otherwise the one that shares the most terms (at
/// least two) with `own_text`, the sentence or item it supports. `None` when
/// the quotation holds in `checked`, or nothing near it holds there too.
fn reanchored(
    checked: &Table,
    shown: &Table,
    index: Option<u64>,
    quote: &str,
    own_text: &str,
) -> Option<(u64, String)> {
    if holds(checked, index, quote) {
        return None;
    }
    let (_, source, text) = (!quote.trim().is_empty())
        .then(|| nearest(shown, index, (quote, 0.6, 1)))
        .flatten()
        .or_else(|| nearest(shown, index, (own_text, 0.0, 2)))?;
    holds(checked, Some(source), &text).then_some((source, text))
}

/// After the author has had its chance to correct its quotations, cite the
/// real passage it meant instead of discarding the lesson: a quotation that
/// holds in `checked` is left alone; otherwise the nearest passage among those
/// `shown` (closest to the quotation, else sharing the most terms with the
/// item or the sentence a citation is for) is written if it holds there too.
/// Citations are provenance; the final review below and claim verification
/// still check every claim before anything is published.
pub(super) fn reanchor_quotes_in(candidate: &mut Value, checked: &Table, shown: &Table) -> usize {
    if shown.is_empty() {
        return 0;
    }
    let mut changed = 0;
    let mut cite = |part: &mut Value, own_text: &str| {
        let quote = part
            .get("quote")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let index = part.get("sourceIndex").and_then(Value::as_u64);
        let Some((source, text)) = reanchored(checked, shown, index, &quote, own_text) else {
            return;
        };
        if let Some(object) = part.as_object_mut() {
            object.insert("sourceIndex".into(), json!(source));
            object.insert("quote".into(), json!(text));
            changed += 1;
            tracing::info!(
                source,
                "Cited the nearest real passage for a quotation the author could not match"
            );
        }
    };
    for field in ["blocks", "questions"] {
        for item in candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let own_text = item_text(item);
            cite(item, &own_text);
            for cited in item
                .get_mut("citations")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let sentence = cited
                    .get("sentence")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                cite(cited, &sentence);
            }
        }
    }
    changed
}
