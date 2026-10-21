//! The claim in its lesson and what research tried for it (UI design §5.1):
//! the plain sentence and paragraph a card shows, where in the lesson it
//! is, and research summed up.

use std::collections::{HashMap, HashSet};

use lattice_core::domain::learning::claim_review::{
    AttachOutcome, ClaimEvent, ClaimLedgerEntry, ClaimState, UnitKind,
};
use lattice_core::shared::text::{plain_sentence_context, plain_text};
use serde_json::Value;

use super::super::dto::{ClaimPresentationDto, ResearchSummaryDto};
use super::SourceFacts;
use crate::features::learning::content_verification::field_text;

/// The claim as plain text in its sentence and paragraph. `units` is the
/// lesson's blocks and questions, as the draft or the published lesson has
/// them; `title` is the claim's unit title.
pub(super) fn presentation(
    entry: &ClaimLedgerEntry,
    units: &Value,
    title: &str,
) -> ClaimPresentationDto {
    let location = entry.location();
    let (context_before, sentence, context_after) =
        match field_text(units, location.unit_index, &location.field) {
            Some(text) => plain_sentence_context(&text, &location.quote),
            None => (String::new(), plain_text(&location.quote), String::new()),
        };
    ClaimPresentationDto {
        statement: plain_text(entry.statement()),
        sentence,
        context_before,
        context_after,
        place: place(location.unit_kind, location.unit_index, units, title),
    }
}

/// "Section 2 · Defining Tool Schemas", "Question 3", "Question 3 · answer
/// key": blocks are numbered from one, questions after the blocks.
fn place(kind: UnitKind, unit_index: usize, units: &Value, title: &str) -> String {
    let blocks = units
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let question = unit_index.saturating_sub(blocks) + 1;
    match kind {
        UnitKind::Block if title.trim().is_empty() => format!("Section {}", unit_index + 1),
        UnitKind::Block => format!("Section {} · {}", unit_index + 1, plain_text(title)),
        UnitKind::Question => format!("Question {question}"),
        UnitKind::AnswerKey => format!("Question {question} · answer key"),
    }
}

/// Research summed up: rounds with a query, the versions it saved, those
/// of them a check compared, and whether one of them supports the claim
/// now.
pub(super) fn tried(
    entry: &ClaimLedgerEntry,
    sources: &HashMap<String, SourceFacts>,
) -> ResearchSummaryDto {
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    let mut searches = 0;
    // Each saved version with when research last saved it.
    let mut saved: HashMap<&str, i64> = HashMap::new();
    let mut compared: HashSet<&str> = HashSet::new();
    for record in entry.events() {
        match &record.event {
            ClaimEvent::Researched {
                queries,
                sources_added,
                ..
            } => {
                if !queries.is_empty() {
                    searches += 1;
                }
                for version in sources_added {
                    saved.insert(version.as_str(), record.provenance.at);
                }
            }
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } => compared.extend(
                check
                    .evidence
                    .iter()
                    .map(|seen| seen.source_version_id.as_str()),
            ),
            _ => {}
        }
    }
    let mut mentioning: Vec<(&str, i64)> = saved
        .iter()
        .filter(|(version, _)| compared.contains(*version))
        .map(|(version, at)| (*version, *at))
        .collect();
    mentioning.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let stated = match entry.state() {
        ClaimState::ResolvedByEvidence {
            source_version_id, ..
        } => saved.contains_key(source_version_id.as_str()),
        _ => false,
    };
    ResearchSummaryDto {
        searches: count(searches),
        pages_read: count(saved.len()),
        pages_mentioning: count(mentioning.len()),
        stated,
        relevant: mentioning
            .iter()
            .filter_map(|(version, _)| sources.get(*version).map(|facts| facts.reference(version)))
            .collect(),
    }
}
