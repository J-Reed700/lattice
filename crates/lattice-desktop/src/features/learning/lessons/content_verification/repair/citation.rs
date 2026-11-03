//! The citation repair (course evidence index §5.1): a claim pass B found
//! support for elsewhere in the course keeps its text, and the citation of its
//! sentence (its section's when the writer cited none) moves to the passage
//! found. No model writes it: the found passage is the citation. A source the
//! lesson does not number yet is added to the lesson's numbered sources, as
//! the writer's are, so the citation can name it.
use super::super::classify::figures;
use super::super::*;
use crate::features::learning::generation::{validate_excerpt_quote, MAX_QUOTE_CHARS};
use crate::features::learning::reference_collection::{checked_span, shared_terms};
use lattice_core::domain::learning::claim_review::ClaimEvent;
use lattice_core::shared::text::sentences;
use std::collections::BTreeSet;
use std::ops::Range;

/// A run of a passage's whole sentences that a section could cite.
struct Run<'s> {
    /// How many of the claim's figures its checked passage shows.
    figures: usize,
    /// How many of the claim's terms its checked passage holds.
    read: usize,
    /// How many of the claim's terms it holds itself.
    own: usize,
    chars: usize,
    start: usize,
    text: &'s str,
}

/// The quotation a section can cite for `claim` from the passage `within`
/// `source` that supports it: a contiguous run of the passage's whole
/// sentences, no longer than a lesson quotation may be. The check that
/// decides the repair reads the citation as verification widens it
/// ([`checked_span`]), so the run is the one whose checked passage shows most
/// of the claim's figures, then most of its terms, read as the course index
/// reads words ([`shared_terms`]). Of those, the run holding most of the
/// terms itself, then the shortest, since a longer one only adds words the
/// claim does not name, then the earliest. `None` when no run of whole
/// sentences is one a lesson citation may carry.
pub(in crate::features::learning::lessons::content_verification) fn quotation<'s>(
    claim: &str,
    source: &'s str,
    within: Range<usize>,
) -> Option<&'s str> {
    let passage = source.get(within.clone())?;
    let stated: BTreeSet<&str> = figures(claim).collect();
    let spans = sentences(passage);
    let mut runs: Vec<Run<'s>> = Vec::new();
    for (first, &(start, _)) in spans.iter().enumerate() {
        for &(_, end) in spans.iter().skip(first) {
            let Some(text) = passage.get(start..end) else {
                continue;
            };
            let chars = text.chars().count();
            if chars > MAX_QUOTE_CHARS {
                break;
            }
            let (from, to) = checked_span(source, within.start + start, within.start + end);
            let read = source.get(from..to).unwrap_or(text);
            let shown: BTreeSet<&str> = figures(read).collect();
            runs.push(Run {
                figures: stated.intersection(&shown).count(),
                read: shared_terms(claim, read),
                own: shared_terms(claim, text),
                chars,
                start,
                text,
            });
        }
    }
    runs.sort_by(|a, b| {
        b.figures
            .cmp(&a.figures)
            .then(b.read.cmp(&a.read))
            .then(b.own.cmp(&a.own))
            .then(a.chars.cmp(&b.chars))
            .then(a.start.cmp(&b.start))
    });
    runs.into_iter()
        .map(|run| run.text)
        .find(|run| validate_excerpt_quote(run, source).is_ok())
}

/// Every `sourceIndex` of a lesson `schema` allows the `count` sources the
/// lesson numbers, once verification has numbered one more.
pub(in crate::features::learning::lessons::content_verification) fn number_sources(
    schema: &mut Value,
    count: usize,
) {
    fn allow(index: &mut Value, count: usize) {
        if let Some(maximum) = index.get_mut("maximum") {
            *maximum = json!(count.saturating_sub(1));
        }
        if let Some(options) = index.get_mut("anyOf").and_then(Value::as_array_mut) {
            for option in options {
                allow(option, count);
            }
        }
    }
    match schema {
        Value::Object(fields) => {
            for (name, value) in fields.iter_mut() {
                if name == "sourceIndex" {
                    allow(value, count);
                } else {
                    number_sources(value, count);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                number_sources(item, count);
            }
        }
        _ => {}
    }
}

/// Names, in the repair context's evidence table, the `sourceIndex` of each
/// passage whose source the lesson numbers, so a repair that cites a passage
/// can say which excerpt it is.
pub(in crate::features::learning::lessons::content_verification) fn number_evidence(
    context: &mut Value,
    numbered: &[LearningSourceDto],
) {
    let Some(passages) = context.get_mut("evidence").and_then(Value::as_array_mut) else {
        return;
    };
    for passage in passages {
        let index = passage
            .get("sourceId")
            .and_then(Value::as_str)
            .and_then(|id| numbered.iter().position(|source| source.id == id));
        if let (Some(index), Some(passage)) = (index, passage.as_object_mut()) {
            passage.insert("sourceIndex".into(), json!(index));
        }
    }
}

/// The citations `repaired` appended to `section`'s own, in order.
pub(super) fn appended(section: &Value, repaired: &Value) -> Vec<Value> {
    let had = section
        .get("citations")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    repaired
        .get("citations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .skip(had)
        .cloned()
        .collect()
}

/// Puts the `appended` citations back into `confined`, a section whose repair
/// was confined to the failed sentences. Only a citation of a sentence that
/// stands in the section's text is kept, and none twice.
pub(super) fn restore_appended(confined: &mut Value, appended: Vec<Value>, field: &str) {
    let text = if field == "blocks" {
        "body"
    } else {
        "explanation"
    };
    let held = confined
        .get(text)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let standing: Vec<Value> = appended
        .into_iter()
        .filter(|entry| {
            entry
                .get("sentence")
                .and_then(Value::as_str)
                .is_some_and(|sentence| held.contains(sentence))
        })
        .collect();
    if standing.is_empty() {
        return;
    }
    let Some(listed) = confined
        .as_object_mut()
        .map(|section| section.entry("citations").or_insert_with(|| json!([])))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for entry in standing {
        if !listed.contains(&entry) {
            listed.push(entry);
        }
    }
}

/// Rewrites the citation of each section one of whose claims pass B supported
/// elsewhere to the passage found, the citation of the claim's sentence when
/// the writer cited it: `quote` becomes the best-matching run of its
/// sentences ([`quotation`]) and `sourceIndex` its source, numbered if new.
/// Only sections with a citation and undoubted support; `tried` citations are
/// never written again, so two passages cannot trade places.
/// Returns the finding's position and ledger event per rewritten section.
pub(in crate::features::learning::lessons::content_verification) fn cite_where_supported(
    candidate: &mut Value,
    findings: &[Finding],
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
    tried: &mut HashSet<(usize, String)>,
) -> Result<Vec<(usize, ClaimEvent)>> {
    let mut rewritten = BTreeSet::new();
    let mut repairs = Vec::new();
    for (position, finding) in findings.iter().enumerate() {
        let Some(found) = finding.two_pass.supported_by.as_ref() else {
            continue;
        };
        if finding.verdict != ClaimVerdict::Supported || rewritten.contains(&finding.unit) {
            continue;
        }
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == found.source_id)
        else {
            continue;
        };
        let Some(quote) = quotation(
            &finding.statement,
            &source.excerpt,
            found.start_byte..found.end_byte,
        ) else {
            tracing::warn!(
                unit = finding.unit,
                source = %found.source_id,
                "No run of whole sentences of the supporting passage can be a lesson citation"
            );
            continue;
        };
        if !tried.insert((finding.unit, quote.to_owned())) {
            continue;
        }
        let (field, item) = super::location(candidate, finding.unit)?;
        // Only a section that carries a citation has one to repair: the
        // lesson's own sections always do; one without any is not given one.
        let Some(section) = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(item))
            .and_then(Value::as_object_mut)
            .filter(|section| section.contains_key("quote"))
        else {
            continue;
        };
        let entry = section
            .get("citations")
            .and_then(Value::as_array)
            .and_then(|cited| {
                cited.iter().position(|entry| {
                    entry
                        .get("sentence")
                        .and_then(Value::as_str)
                        .is_some_and(|sentence| restates(sentence, &finding.statement))
                })
            });
        let target = match entry {
            Some(at) => section
                .get_mut("citations")
                .and_then(Value::as_array_mut)
                .and_then(|cited| cited.get_mut(at))
                .and_then(Value::as_object_mut),
            None => Some(section),
        };
        let Some(target) = target else {
            continue;
        };
        let numbered_at = numbered.iter().position(|known| known.id == source.id);
        let index = numbered_at.unwrap_or(numbered.len());
        if target.get("quote").and_then(Value::as_str) == Some(quote)
            && target.get("sourceIndex").and_then(Value::as_u64) == Some(index as u64)
        {
            continue;
        }
        if numbered_at.is_none() {
            numbered.push(source.clone());
        }
        target.insert("quote".into(), json!(quote));
        target.insert("sourceIndex".into(), json!(index));
        rewritten.insert(finding.unit);
        tracing::info!(
            unit = finding.unit,
            source = %found.source_id,
            cited = ?found.cited,
            added_source = numbered_at.is_none(),
            "Repaired a section's citation to the passage that supports its claim"
        );
        repairs.push((
            position,
            ClaimEvent::CitationRepaired {
                cited: found.cited.clone(),
                source_version_id: source.id.clone(),
                quote: quote.to_owned(),
                added_source: numbered_at.is_none(),
            },
        ));
    }
    Ok(repairs)
}
