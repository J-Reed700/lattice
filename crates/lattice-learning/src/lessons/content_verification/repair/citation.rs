//! The citation repair (course evidence index §5.1): a claim a page of the
//! course gave support for elsewhere keeps its text, and the citation of the
//! sentence that states it moves to the passage found, or is added where the
//! sentence has none of its own. No model writes it: the found passage is the
//! citation. A source the lesson does not number yet is added to the lesson's
//! numbered sources, as the writer's are, so the citation can name it.
use super::super::classify::figures;
use super::super::*;
use crate::generation::{validate_excerpt_quote, MAX_QUOTE_CHARS};
use crate::reference_collection::{shared_terms, verification_window};
use lattice_core::domain::learning::claim_review::ClaimEvent;
use lattice_core::shared::text::same_text::holds_sentence;
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
/// ([`verification_window`]), so the run is the one whose checked passage shows most
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
            let window = verification_window(source, within.start + start..within.start + end);
            let read = source.get(window).unwrap_or(text);
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
    /// No citation was written for it, for the reason given.
    Cannot(Unwritten),
}

/// Why the citation repair wrote nothing for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::lessons::content_verification) enum Unwritten {
    /// The finding is not a support found elsewhere in the course.
    NotSupportedElsewhere,
    /// The claim's quote is not one sentence.
    SeveralSentences,
    /// The source of the passage found is not among the saved sources.
    SourceNotSaved,
    /// No run of the passage's whole sentences can be a lesson citation.
    NoCitableRun,
    /// This citation was written before in the job.
    WrittenBefore,
    /// The claim's section is not in the draft.
    SectionMissing,
    /// The sentence's citation already is the passage found.
    AlreadyCited,
    /// The claim's sentence is not in its section's citable text.
    NotInSection,
}

impl Unwritten {
    /// The reason, as one plain sentence.
    pub(in crate::lessons::content_verification) fn why(&self) -> &'static str {
        match self {
            Self::NotSupportedElsewhere => "no page of the course found it supported elsewhere",
            Self::SeveralSentences => "its quote is more than one sentence, so none is its own",
            Self::SourceNotSaved => "the source of the passage found is not saved",
            Self::NoCitableRun => {
                "no run of whole sentences of the passage found can be a lesson citation"
            }
            Self::WrittenBefore => "this citation was written before",
            Self::SectionMissing => "its section is not in the draft",
            Self::AlreadyCited => "its sentence already cites the passage found",
            Self::NotInSection => "its sentence is not in its section's citable text",
        }
    }
}

/// Cites the sentence that states each claim a page found support for
/// elsewhere at the passage found: `quote` becomes the best-matching run of
/// the passage's sentences ([`quotation`]) and `sourceIndex` its source,
/// numbered if new. A finding's `quote` is the one sentence it stands on. The
/// entry of the section's `citations` that holds the sentence moves there.
/// Where the sentence has no entry, or its entry also holds a sentence of
/// `kept` (the live claims' sentences not being cited), an entry is added
/// instead and the shared one stays. The section's own `quote` and
/// `sourceIndex`, which its other statements are read against, are never
/// written. One claim of a sentence is cited in a call, so that two cannot
/// write one entry; a sentence is not cited again at a passage `tried` cited
/// it at, so two passages cannot trade places. Gives what became of each of
/// `findings`, in their order.
pub(in crate::lessons::content_verification) fn cite_where_supported(
    candidate: &mut Value,
    findings: &[Finding],
    kept: &[(usize, String)],
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
    tried: &mut HashSet<(usize, String, String)>,
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
            repairs.push(Cited::Cannot(Unwritten::NotSupportedElsewhere));
            continue;
        };
        let sentence = finding.quote.as_str();
        if sentences(sentence).len() != 1 {
            repairs.push(Cited::Cannot(Unwritten::SeveralSentences));
            continue;
        }
        if written.contains(&(finding.unit, sentence.to_owned())) {
            repairs.push(Cited::Deferred);
            continue;
        }
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == found.source_id)
        else {
            repairs.push(Cited::Cannot(Unwritten::SourceNotSaved));
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
            repairs.push(Cited::Cannot(Unwritten::NoCitableRun));
            continue;
        };
        if !tried.insert((finding.unit, sentence.to_owned(), quote.to_owned())) {
            repairs.push(Cited::Cannot(Unwritten::WrittenBefore));
            continue;
        }
        let (field, item) = super::section_of(candidate, finding.unit)?;
        let numbered_at = numbered.iter().position(|known| known.id == source.id);
        let index = numbered_at.unwrap_or(numbered.len());
        let added = candidate
            .get(field)
            .and_then(Value::as_array)
            .and_then(|items| items.get(item))
            .and_then(|part| {
                crate::lessons::text_edits::citation_entry(
                    part,
                    field != "blocks",
                    sentence,
                    json!(index),
                    quote,
                )
            });
        let Some(section) = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(item))
            .and_then(Value::as_object_mut)
        else {
            repairs.push(Cited::Cannot(Unwritten::SectionMissing));
            continue;
        };
        let listed = section.get("citations").and_then(Value::as_array);
        let own = listed.and_then(|listed| {
            listed.iter().position(|entry| {
                entry
                    .get("sentence")
                    .and_then(Value::as_str)
                    .is_some_and(|cited| holds_sentence(cited, sentence))
            })
        });
        let entry = own.and_then(|at| listed.and_then(|listed| listed.get(at)));
        let shared = entry
            .and_then(|entry| entry.get("sentence"))
            .and_then(Value::as_str)
            .is_some_and(|cited| {
                kept.iter()
                    .any(|(unit, other)| *unit == finding.unit && holds_sentence(cited, other))
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
        if already {
            repairs.push(Cited::Cannot(Unwritten::AlreadyCited));
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
            None => added.and_then(|entry| {
                section
                    .entry("citations")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .map(|listed| listed.push(entry))
            }),
        };
        if wrote.is_none() {
            repairs.push(Cited::Cannot(Unwritten::NotInSection));
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

/// Drops the sentence's own citation for each of `quotes`, a section's index
/// and the sentence a claim stands on: the entry of the section's `citations`
/// whose sentence holds it, unless a sentence of `kept` is held by it too,
/// since it is that claim's citation. A sentence with no entry of its own keeps
/// what it has: the section's quote, which the others are read against. Gives,
/// for each sentence, the source versions of the entry it stood on and lost,
/// empty when nothing went.
pub(in crate::lessons::content_verification) fn uncite(
    candidate: &mut Value,
    quotes: &[(usize, String)],
    kept: &[(usize, String)],
    references: &ReferenceCollection<'_>,
) -> Result<Vec<Vec<String>>> {
    let mut dropped: Vec<Vec<String>> = Vec::with_capacity(quotes.len());
    let mut removed: Vec<(usize, String, Vec<String>)> = Vec::new();
    for (unit, claimed) in quotes {
        if let Some((_, _, sources)) = removed
            .iter()
            .find(|(held, sentence, _)| held == unit && holds_sentence(sentence, claimed))
        {
            dropped.push(sources.clone());
            continue;
        }
        let (field, item) = super::section_of(candidate, *unit)?;
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
                    .is_some_and(|sentence| holds_sentence(sentence, claimed))
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
            .any(|(held, other)| held == unit && holds_sentence(&sentence, other));
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
