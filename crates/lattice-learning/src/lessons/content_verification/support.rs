//! What a lesson statement's citation is: how it uses its sources and which
//! sentences of them its chips open. This is the one place that decides it.
use super::evidence_checks::holders;
use super::learner_view::statement_class;
use super::{ClaimVerdict, EvidencePassage, Finding};
use crate::lesson_evidence::LearningStatementClassDto;
use lattice_core::application::contracts::replies::{AnchorDto, ByteSpanDto};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::citation::{relation_of, Anchor, Grounds, Relation};
use lattice_core::domain::provenance::Label;
use lattice_core::shared::text::same_text::{find, quotation, sentences_over};
use std::collections::BTreeSet;

/// How a statement is cited: its relation to its sources, whether its own
/// text is drawn as a quotation, and the passages its chips open.
pub(super) struct Cited<'f> {
    pub(super) relation: Relation,
    /// The statement's own lesson text is drawn as a quotation.
    pub(super) quoted: bool,
    /// The passages the chips open, in the order the anchors come, each with its
    /// anchors; an empty list opens the passage whole.
    pub(super) on: Vec<(&'f EvidencePassage, Vec<Anchor>)>,
}

type On<'f> = Vec<(&'f EvidencePassage, Vec<Anchor>)>;

/// The passages a statement was checked against that are not an example's run.
fn sources(finding: &Finding) -> impl Iterator<Item = &EvidencePassage> {
    finding.evidence.iter().filter(|passage| !passage.is_run())
}

/// `passages`, each once, with no anchor: each is opened whole.
fn whole<'f>(passages: impl Iterator<Item = &'f EvidencePassage>) -> On<'f> {
    let mut seen = BTreeSet::new();
    passages
        .filter(|passage| {
            seen.insert((
                passage.source_id.as_str(),
                passage.start_byte,
                passage.end_byte,
            ))
        })
        .map(|passage| (passage, Vec::new()))
        .collect()
}

fn holds(passage: &EvidencePassage, anchor: &Anchor) -> bool {
    passage.source_id == anchor.source_version_id
        && passage.start_byte <= anchor.start_byte
        && anchor.end_byte <= passage.end_byte
}

/// The anchor of `passage`'s bytes `start..end`.
fn anchor_in(passage: &EvidencePassage, (start, end): (usize, usize)) -> Anchor {
    Anchor {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte + start,
        end_byte: passage.start_byte + end,
    }
}

/// `anchors` in source order, each once, under the first passage that holds
/// it; an anchor no passage holds is dropped. Passages come in the order of
/// their first anchor.
fn grouped(finding: &Finding, mut anchors: Vec<Anchor>) -> On<'_> {
    anchors.sort();
    anchors.dedup();
    let mut on: On<'_> = Vec::new();
    for anchor in anchors {
        let Some(passage) = sources(finding).find(|passage| holds(passage, &anchor)) else {
            continue;
        };
        match on.iter_mut().find(|(held, _)| std::ptr::eq(*held, passage)) {
            Some((_, list)) => list.push(anchor),
            None => on.push((passage, vec![anchor])),
        }
    }
    on
}

/// The anchors of the finding's parts of `kind`, in part order.
fn anchors_of(finding: &Finding, kind: PartKind) -> Vec<Anchor> {
    finding
        .two_pass
        .parts
        .iter()
        .filter(|part| part.kind == kind)
        .flat_map(|part| part.anchors.iter().cloned())
        .collect()
}

/// The finding's supporting quote, when it has one.
fn supporting(finding: &Finding) -> Option<&str> {
    finding
        .supporting_quote
        .as_deref()
        .filter(|quote| !quote.trim().is_empty())
}

/// The statement's own text is a quotation of a passage it was checked
/// against.
fn quoting(finding: &Finding) -> Option<Cited<'_>> {
    sources(finding).find_map(|passage| {
        let span = quotation(&passage.text, &finding.quote)?;
        Some(Cited {
            relation: Relation::Quotes,
            quoted: true,
            on: vec![(passage, vec![anchor_in(passage, span)])],
        })
    })
}

/// A quotation the lesson already marks as one: at the anchors of its stated
/// parts, else, for a finding from before parts had anchors, at the words of
/// its supporting quote.
fn attributed(finding: &Finding) -> Option<Cited<'_>> {
    let mut on = grouped(finding, anchors_of(finding, PartKind::Stated));
    if on.is_empty() {
        let quote = supporting(finding)?;
        let (passage, span) =
            sources(finding).find_map(|passage| Some((passage, find(&passage.text, quote)?)))?;
        on = vec![(passage, vec![anchor_in(passage, span)])];
    }
    Some(Cited {
        relation: Relation::Quotes,
        quoted: false,
        on,
    })
}

/// The statement as its stated and follows parts rest on sentences.
fn by_parts(finding: &Finding) -> Option<Cited<'_>> {
    let parts: Vec<(Grounds, &[Anchor])> = finding
        .two_pass
        .parts
        .iter()
        .filter_map(|part| {
            let grounds = match part.kind {
                PartKind::Stated => Grounds::Stated,
                PartKind::Follows => Grounds::Follows,
                _ => return None,
            };
            Some((grounds, part.anchors.as_slice()))
        })
        .collect();
    let (relation, anchors) = relation_of(&parts)?;
    let on = grouped(finding, anchors);
    (!on.is_empty()).then_some(Cited {
        relation,
        quoted: false,
        on,
    })
}

/// A supporting quote shorter than the passage that holds it backs the
/// sentences it lies in.
fn backing_sentences(finding: &Finding) -> Option<Cited<'_>> {
    let quote = supporting(finding)?;
    let passage = holders(quote, &finding.evidence)
        .into_iter()
        .find(|(passage, words)| !passage.is_run() && words.len() < passage.text.len())
        .map(|(passage, _)| passage)?;
    let lying = sentences_over(&passage.text, find(&passage.text, quote)?);
    let span = (lying.first()?.0, lying.last()?.1);
    Some(Cited {
        relation: Relation::Backs,
        quoted: false,
        on: vec![(passage, vec![anchor_in(passage, span)])],
    })
}

/// Backs whole the passages a check named without a sentence: the sources of
/// its stated parts, else the passage a supporting quote equals.
fn backing_whole(finding: &Finding) -> Option<Cited<'_>> {
    let mut on = whole(
        finding
            .two_pass
            .parts
            .iter()
            .filter(|part| part.kind == PartKind::Stated)
            .filter_map(|part| part.source_id.as_deref())
            .filter_map(|id| sources(finding).find(|passage| passage.source_id == id)),
    );
    if on.is_empty() {
        let quote = supporting(finding)?;
        on = whole(
            holders(quote, &finding.evidence)
                .into_iter()
                .find(|(passage, words)| !passage.is_run() && words.len() == passage.text.len())
                .map(|(passage, _)| passage)
                .into_iter(),
        );
    }
    (!on.is_empty()).then_some(Cited {
        relation: Relation::Backs,
        quoted: false,
        on,
    })
}

fn sourced(finding: &Finding) -> Option<Cited<'_>> {
    quoting(finding)
        .or_else(|| {
            matches!(finding.label, Some(Label::Attributed))
                .then(|| attributed(finding))
                .flatten()
        })
        .or_else(|| by_parts(finding))
        .or_else(|| backing_sentences(finding))
        .or_else(|| backing_whole(finding))
}

/// The citation of a finding's statement; none when it gets no chip.
pub(super) fn cited(finding: &Finding) -> Option<Cited<'_>> {
    if finding.verdict != ClaimVerdict::Supported {
        return None;
    }
    match finding
        .class
        .and_then(|classed| statement_class(classed.class))
    {
        Some(LearningStatementClassDto::Common) => None,
        Some(LearningStatementClassDto::Reasoning) => {
            let on = whole(sources(finding));
            (!on.is_empty()).then_some(Cited {
                relation: Relation::Agrees,
                quoted: false,
                on,
            })
        }
        Some(LearningStatementClassDto::Sourced) | None => sourced(finding),
    }
}

/// `anchor` as the wire carries it, its words cut from `passage`; none when
/// the anchor does not lie in that passage.
pub(super) fn anchor_dto(passage: &EvidencePassage, anchor: &Anchor) -> Option<AnchorDto> {
    if !holds(passage, anchor) {
        return None;
    }
    let exact = passage.text.get(
        anchor.start_byte.checked_sub(passage.start_byte)?
            ..anchor.end_byte.checked_sub(passage.start_byte)?,
    )?;
    Some(AnchorDto {
        exact: exact.to_string(),
        span: Some(ByteSpanDto {
            start: anchor.start_byte,
            end: anchor.end_byte,
        }),
    })
}

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
