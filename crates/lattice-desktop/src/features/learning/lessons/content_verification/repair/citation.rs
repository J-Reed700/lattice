//! The citation repair (course evidence index §5.1): a claim pass B found
//! support for elsewhere in the course keeps its text, and its section's
//! citation moves to the passage found. No model writes it: the found
//! passage is the citation. A source the lesson does not number yet is
//! added to the lesson's numbered sources, as the writer's are, so the
//! citation can name it.
use super::super::*;
use crate::features::learning::generation::{validate_excerpt_quote, MAX_QUOTE_CHARS};
use crate::features::learning::reference_collection::shared_terms;
use lattice_core::domain::learning::claim_review::ClaimEvent;
use lattice_core::shared::text::sentences;
use std::collections::BTreeSet;

/// The quotation a section can cite for the found `passage` of `source`:
/// the contiguous run of the passage's whole sentences that holds most of
/// `claim`'s terms, read as the course index reads words ([`shared_terms`]),
/// no longer than a lesson quotation may be. Of the runs that match best,
/// the shortest, since a longer one only adds words the claim does not name,
/// and of those the earliest. `None` when no run of whole sentences is one a
/// lesson citation may carry.
pub(in crate::features::learning::lessons::content_verification) fn quotation<'p>(
    claim: &str,
    passage: &'p str,
    source: &str,
) -> Option<&'p str> {
    let spans = sentences(passage);
    let mut runs: Vec<(usize, usize, usize, &str)> = Vec::new();
    for (first, &(start, _)) in spans.iter().enumerate() {
        for &(_, end) in spans.iter().skip(first) {
            let Some(run) = passage.get(start..end) else {
                continue;
            };
            let chars = run.chars().count();
            if chars > MAX_QUOTE_CHARS {
                break;
            }
            runs.push((shared_terms(claim, run), start, chars, run));
        }
    }
    runs.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    runs.into_iter()
        .map(|(_, _, _, run)| run)
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

/// Rewrites the citation of each section one of whose claims pass B supported
/// elsewhere to the passage found: `quote` becomes the best-matching run of
/// its sentences ([`quotation`]) and `sourceIndex` its source, numbered if
/// new. Only sections with a citation and undoubted support; `tried`
/// citations are never written again, so two passages cannot trade places.
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
        if finding.verdict != ClaimVerdict::Supported
            || finding.scrutinised().is_some()
            || rewritten.contains(&finding.unit)
        {
            continue;
        }
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == found.source_id)
        else {
            continue;
        };
        let passage = source
            .excerpt
            .get(found.start_byte..found.end_byte)
            .unwrap_or_default();
        let Some(quote) = quotation(&finding.statement, passage, &source.excerpt) else {
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
        let numbered_at = numbered.iter().position(|known| known.id == source.id);
        let index = numbered_at.unwrap_or(numbered.len());
        if section.get("quote").and_then(Value::as_str) == Some(quote)
            && section.get("sourceIndex").and_then(Value::as_u64) == Some(index as u64)
        {
            continue;
        }
        if numbered_at.is_none() {
            numbered.push(source.clone());
        }
        section.insert("quote".into(), json!(quote));
        section.insert("sourceIndex".into(), json!(index));
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
