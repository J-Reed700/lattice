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
use std::collections::BTreeSet;

/// The byte spans of `text`'s sentences, trimmed: a sentence ends at a
/// line break, or at '.', '!' or '?' before whitespace or the end.
fn sentences(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut push = |from: usize, to: usize| {
        let Some(piece) = text.get(from..to) else {
            return;
        };
        let lead = piece.len() - piece.trim_start().len();
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            spans.push((from + lead, from + lead + trimmed.len()));
        }
    };
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let before_space = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        if ch == '\n' || (matches!(ch, '.' | '!' | '?') && before_space) {
            let end = at + ch.len_utf8();
            push(start, end);
            start = end;
        }
    }
    push(start, text.len());
    spans
}

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

/// Rewrites the citation of each section one of whose claims pass B
/// supported elsewhere to the passage it found (`supported_by`): the
/// section's `quote` becomes the run of that passage's sentences that best
/// matches the claim ([`quotation`]), and its `sourceIndex` the passage's
/// source in `numbered`, the sources the lesson numbers. A source not
/// numbered yet is added, so a supported claim never keeps a citation to the
/// wrong source; the next pass reads the new citation as the section's own
/// (pass A). A section is rewritten only when the claim's support is not in
/// doubt (the provenance gate asked for no scrutiny); a section's first such
/// claim decides. `tried` holds the citations written before in this job,
/// never written again, so two passages a judge weighs differently on
/// successive passes cannot trade places. Returns, per rewritten section,
/// the finding's position and the ledger event that records the repair.
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
        let Some(section) = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(item))
            .and_then(Value::as_object_mut)
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
