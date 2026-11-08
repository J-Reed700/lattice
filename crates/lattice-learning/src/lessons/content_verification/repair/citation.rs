//! The citation repair (course evidence index §5.1): a claim a page of the
//! course gave support for elsewhere keeps its text, and the citation of the
//! sentence that states it moves to the passage found, or is added where the
//! sentence has none of its own. No model writes it: the found passage is the
//! citation. A source the lesson does not number yet is added to the lesson's
//! numbered sources, as the writer's are, so the citation can name it.
use super::super::classify::figures;
use super::super::*;
use crate::generation::{validate_excerpt_quote, MAX_CITATIONS, MAX_QUOTE_CHARS};
use crate::reference_collection::{checked_span, shared_terms};
use lattice_core::domain::learning::claim_review::ClaimEvent;
use lattice_core::shared::text::{claim_sentence, sentences};
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
pub(in crate::lessons::content_verification) fn quotation<'s>(
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
pub(in crate::lessons::content_verification) fn number_sources(schema: &mut Value, count: usize) {
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
pub(in crate::lessons::content_verification) fn number_evidence(
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
    let held: Vec<String> = crate::lessons::text_edits::citable_texts(confined, field != "blocks")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let standing: Vec<Value> = appended
        .into_iter()
        .filter(|entry| {
            entry
                .get("sentence")
                .and_then(Value::as_str)
                .is_some_and(|sentence| held.iter().any(|text| text.contains(sentence)))
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

/// What the citation repair did for one finding.
pub(in crate::lessons::content_verification) enum Cited {
    /// The sentence's citation moved to the passage found, or the sentence
    /// was given an entry for it: the event the ledger records.
    Recited(Box<ClaimEvent>),
    /// Another claim of its sentence was cited already in this call, which
    /// cites one claim of a sentence so that two cannot write one entry: the
    /// claim waits for the next round.
    Deferred,
    /// No citation can be written for it: the locator names no sentence of its
    /// quote, no run of the passage's whole sentences can be one, it was
    /// written before, the section already cites the passage, or the section
    /// lists as many citations as the lesson schema allows.
    Cannot,
}

/// The sentence of `quote`, a claim's passage, that `statement` restates.
fn stating<'q>(quote: &'q str, statement: &str) -> Option<&'q str> {
    let (start, end) = claim_sentence(quote, (0, quote.len()), statement)?;
    quote.get(start..end)
}

/// Cites the sentence that states each claim a page found support for
/// elsewhere at the passage found: `quote` becomes the best-matching run of
/// the passage's sentences ([`quotation`]) and `sourceIndex` its source,
/// numbered if new. The entry of the section's `citations` that restates the
/// claim moves there. Where the sentence has no entry, or its entry is also the
/// citation of a statement of `kept` (the live statements not being cited), an
/// entry is added instead and the shared one stays, unless the section lists as
/// many as the lesson schema allows. The section's own `quote` and
/// `sourceIndex`, which its other statements are read against, are never
/// written. One claim of a sentence is cited in a call, so that two cannot
/// write one entry; `tried` citations are never written again, so two passages
/// cannot trade places. Gives what became of each of `findings`, in their
/// order.
pub(in crate::lessons::content_verification) fn cite_where_supported(
    candidate: &mut Value,
    findings: &[Finding],
    kept: &[(usize, String)],
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
    tried: &mut HashSet<(usize, String)>,
) -> Result<Vec<Cited>> {
    let mut written = BTreeSet::new();
    let mut repairs = Vec::with_capacity(findings.len());
    for finding in findings {
        let Some(found) = finding
            .two_pass
            .supported_by
            .as_ref()
            .filter(|_| finding.verdict == ClaimVerdict::Supported)
        else {
            repairs.push(Cited::Cannot);
            continue;
        };
        let Some(sentence) = stating(&finding.quote, &finding.statement) else {
            repairs.push(Cited::Cannot);
            continue;
        };
        if written.contains(&(finding.unit, sentence.to_owned())) {
            repairs.push(Cited::Deferred);
            continue;
        }
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == found.source_id)
        else {
            repairs.push(Cited::Cannot);
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
            repairs.push(Cited::Cannot);
            continue;
        };
        if !tried.insert((finding.unit, quote.to_owned())) {
            repairs.push(Cited::Cannot);
            continue;
        }
        let (field, item) = super::location(candidate, finding.unit)?;
        let Some(section) = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(item))
            .and_then(Value::as_object_mut)
        else {
            repairs.push(Cited::Cannot);
            continue;
        };
        let numbered_at = numbered.iter().position(|known| known.id == source.id);
        let index = numbered_at.unwrap_or(numbered.len());
        let listed = section.get("citations").and_then(Value::as_array);
        let own = listed.and_then(|listed| {
            listed.iter().position(|entry| {
                entry
                    .get("sentence")
                    .and_then(Value::as_str)
                    .is_some_and(|cited| restates(cited, &finding.statement))
            })
        });
        let entry = own.and_then(|at| listed.and_then(|listed| listed.get(at)));
        let shared = entry
            .and_then(|entry| entry.get("sentence"))
            .and_then(Value::as_str)
            .is_some_and(|cited| {
                kept.iter()
                    .any(|(unit, other)| *unit == finding.unit && restates(cited, other))
            });
        let moves = own.filter(|_| !shared);
        let cites = |quoted: Option<&Value>, indexed: Option<&Value>| {
            quoted.and_then(Value::as_str) == Some(quote)
                && indexed.and_then(Value::as_u64) == Some(index as u64)
        };
        let already = match entry {
            Some(entry) => cites(entry.get("quote"), entry.get("sourceIndex")),
            None => cites(section.get("quote"), section.get("sourceIndex")),
        };
        let full = listed.map_or(0, Vec::len) >= MAX_CITATIONS;
        if already || (moves.is_none() && full) {
            repairs.push(Cited::Cannot);
            continue;
        }
        let wrote = match moves {
            Some(at) => section
                .get_mut("citations")
                .and_then(Value::as_array_mut)
                .and_then(|listed| listed.get_mut(at))
                .and_then(Value::as_object_mut)
                .map(|entry| {
                    entry.insert("quote".into(), json!(quote));
                    entry.insert("sourceIndex".into(), json!(index));
                }),
            None => section
                .entry("citations")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .map(|listed| {
                    listed
                        .push(json!({"sentence": sentence, "sourceIndex": index, "quote": quote}));
                }),
        };
        if wrote.is_none() {
            repairs.push(Cited::Cannot);
            continue;
        }
        if numbered_at.is_none() {
            numbered.push(source.clone());
        }
        written.insert((finding.unit, sentence.to_owned()));
        tracing::info!(
            unit = finding.unit,
            source = %found.source_id,
            cited = ?found.cited,
            added_source = numbered_at.is_none(),
            moved = moves.is_some(),
            "Cited the sentence that states a claim at the passage that supports it"
        );
        repairs.push(Cited::Recited(Box::new(ClaimEvent::CitationRepaired {
            cited: found.cited.clone(),
            source_version_id: source.id.clone(),
            quote: quote.to_owned(),
            added_source: numbered_at.is_none(),
        })));
    }
    Ok(repairs)
}

/// Drops the sentence's own citation for each of the `statements`, a section's
/// index and what it states: the entry of the section's `citations` whose
/// sentence restates the statement, unless a statement of `kept` restates it
/// too, since it is that statement's citation. A sentence with no entry of its
/// own keeps what it has: the section's quote, which the others are read
/// against. Gives, for each statement, the source versions of the entry it
/// stood on and lost, empty when nothing went.
pub(in crate::lessons::content_verification) fn uncite(
    candidate: &mut Value,
    statements: &[(usize, String)],
    kept: &[(usize, String)],
    references: &ReferenceCollection<'_>,
) -> Result<Vec<Vec<String>>> {
    let mut dropped: Vec<Vec<String>> = Vec::with_capacity(statements.len());
    let mut removed: Vec<(usize, String, Vec<String>)> = Vec::new();
    for (unit, statement) in statements {
        if let Some((_, _, sources)) = removed
            .iter()
            .find(|(held, sentence, _)| held == unit && restates(sentence, statement))
        {
            dropped.push(sources.clone());
            continue;
        }
        let (field, item) = super::location(candidate, *unit)?;
        let cited = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(item))
            .and_then(|section| section.get_mut("citations"))
            .and_then(Value::as_array_mut);
        let at = cited.as_ref().and_then(|cited| {
            cited.iter().position(|entry| {
                entry
                    .get("sentence")
                    .and_then(Value::as_str)
                    .is_some_and(|sentence| restates(sentence, statement))
            })
        });
        let (Some(cited), Some(at)) = (cited, at) else {
            dropped.push(Vec::new());
            continue;
        };
        let sentence = cited
            .get(at)
            .and_then(|entry| entry.get("sentence"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let stands = kept
            .iter()
            .any(|(held, other)| held == unit && restates(&sentence, other));
        if stands {
            dropped.push(Vec::new());
            continue;
        }
        let entry = cited.remove(at);
        let mut sources: Vec<String> = match entry.get("quote").and_then(Value::as_str) {
            Some(quote) => references
                .cited_passages(&[quote])?
                .into_iter()
                .map(|passage| passage.source_id)
                .collect(),
            None => Vec::new(),
        };
        sources.sort();
        sources.dedup();
        removed.push((*unit, sentence, sources.clone()));
        dropped.push(sources);
    }
    Ok(dropped)
}
