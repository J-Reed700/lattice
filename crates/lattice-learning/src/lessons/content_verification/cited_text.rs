//! Where a lesson's citation chips go, which passages they open and how they
//! are numbered down the lesson.
use super::learner_view::statement_class;
use super::{EvidencePassage, Finding};
use crate::lesson_evidence::LearningStatementClassDto;
use lattice_core::application::contracts::replies::{ReplyPieceDto, ReplyReferenceDto};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::shared::text::code_spans;
use lattice_core::shared::text::segmentation::code_fences;
use std::collections::{BTreeMap, BTreeSet, HashMap};

type Place<'p> = (&'p str, usize, usize);

fn place(passage: &EvidencePassage) -> Place<'_> {
    (
        passage.source_id.as_str(),
        passage.start_byte,
        passage.end_byte,
    )
}

fn distinct<'p>(
    passages: impl IntoIterator<Item = &'p EvidencePassage>,
) -> Vec<&'p EvidencePassage> {
    let mut seen = BTreeSet::new();
    passages
        .into_iter()
        .filter(|passage| seen.insert(place(passage)))
        .collect()
}

/// The passages a statement cites, each once and never an example's run.
pub(super) fn stands_on(finding: &Finding) -> Vec<&EvidencePassage> {
    let sources = || finding.evidence.iter().filter(|passage| !passage.is_run());
    match finding
        .class
        .and_then(|classed| statement_class(classed.class))
    {
        Some(LearningStatementClassDto::Common) => Vec::new(),
        Some(LearningStatementClassDto::Reasoning) => distinct(sources()),
        Some(LearningStatementClassDto::Sourced) | None => {
            let stated = distinct(
                finding
                    .two_pass
                    .parts
                    .iter()
                    .filter(|part| part.kind == PartKind::Stated)
                    .filter_map(|part| part.source_id.as_deref())
                    .filter_map(|id| sources().find(|passage| passage.source_id == id)),
            );
            if !stated.is_empty() {
                return stated;
            }
            finding
                .supporting_quote
                .as_deref()
                .filter(|quote| !quote.is_empty())
                .and_then(|quote| sources().find(|passage| passage.text.contains(quote)))
                .or_else(|| sources().next())
                .into_iter()
                .collect()
        }
    }
}

/// The byte of a section's body a statement's citation goes at: the end of
/// its quote, moved past an inline code span it ends inside; none when the
/// quote is not in the body or ends inside a fenced code block.
pub(super) fn citation_point(body: &str, quote: &str) -> Option<usize> {
    if quote.trim().is_empty() {
        return None;
    }
    let end = body.find(quote)? + quote.len();
    if code_fences(body).iter().any(|fence| {
        let whole = fence.whole();
        whole.start < end && end < whole.end
    }) {
        return None;
    }
    let line_start = body.get(..end)?.rfind('\n').map_or(0, |at| at + 1);
    let line_end = body
        .get(end..)?
        .find('\n')
        .map_or(body.len(), |at| end + at);
    let line = body.get(line_start..line_end)?;
    let at = end - line_start;
    Some(
        code_spans(line)
            .into_iter()
            .find(|span| span.start < at && at < span.end)
            .map_or(end, |span| line_start + span.end),
    )
}

/// The lesson's numbered passages, what each statement cites and each
/// section's pieces.
pub(super) struct Numbered<'f> {
    pub(super) passages: Vec<&'f EvidencePassage>,
    pub(super) cites: Vec<Vec<u32>>,
    pub(super) sections: Vec<Vec<ReplyPieceDto>>,
}

#[derive(Default)]
struct Passages<'f> {
    listed: Vec<&'f EvidencePassage>,
    numbers: HashMap<Place<'f>, u32>,
}

impl<'f> Passages<'f> {
    fn number_of(&mut self, passage: &'f EvidencePassage) -> u32 {
        if let Some(&number) = self.numbers.get(&place(passage)) {
            return number;
        }
        self.listed.push(passage);
        let number = u32::try_from(self.listed.len()).unwrap_or(u32::MAX);
        self.numbers.insert(place(passage), number);
        number
    }
}

fn reference(number: u32) -> ReplyPieceDto {
    ReplyPieceDto::Reference {
        reference: ReplyReferenceDto {
            written: format!("[{number}]"),
            kind: "source".into(),
            number,
            lines: None,
            name: None,
        },
    }
}

fn pieces(body: &str, marks: &BTreeMap<usize, BTreeSet<u32>>) -> Vec<ReplyPieceDto> {
    let text = |text: &str| ReplyPieceDto::Text { text: text.into() };
    let mut pieces = Vec::new();
    let mut from = 0;
    for (&point, numbers) in marks {
        if let Some(before) = body.get(from..point).filter(|before| !before.is_empty()) {
            pieces.push(text(before));
        }
        pieces.extend(numbers.iter().map(|&number| reference(number)));
        from = point;
    }
    if let Some(rest) = body.get(from..).filter(|rest| !rest.is_empty()) {
        pieces.push(text(rest));
    }
    pieces
}

/// Numbers the passages the `findings` stand on down the lesson and cuts each
/// body, `bodies[unit]`, at its citation points. A section's placed statements
/// are numbered first, in reading order, then its others; passages only
/// compared come last and get no chip.
pub(super) fn number<'f>(bodies: &[&str], findings: &[&'f Finding]) -> Numbered<'f> {
    let mut passages = Passages::default();
    let mut cites = vec![Vec::new(); findings.len()];
    let mut sections = Vec::with_capacity(bodies.len());
    for (unit, body) in bodies.iter().enumerate() {
        let mut placed = Vec::new();
        let mut unplaced = Vec::new();
        for (index, finding) in findings.iter().copied().enumerate() {
            if finding.unit != unit {
                continue;
            }
            let stands = stands_on(finding);
            if stands.is_empty() {
                continue;
            }
            match citation_point(body, &finding.quote) {
                Some(point) => placed.push((point, index, stands)),
                None => unplaced.push((index, stands)),
            }
        }
        placed.sort_by_key(|(point, index, _)| (*point, *index));
        let mut marks: BTreeMap<usize, BTreeSet<u32>> = BTreeMap::new();
        let mut cited = Vec::new();
        for (point, index, stands) in placed {
            let numbers: Vec<u32> = stands
                .into_iter()
                .map(|passage| passages.number_of(passage))
                .collect();
            marks
                .entry(point)
                .or_default()
                .extend(numbers.iter().copied());
            cited.push((index, numbers));
        }
        for (index, stands) in unplaced {
            let numbers: Vec<u32> = stands
                .into_iter()
                .map(|passage| passages.number_of(passage))
                .collect();
            cited.push((index, numbers));
        }
        for (index, numbers) in cited {
            if let Some(slot) = cites.get_mut(index) {
                *slot = numbers;
            }
        }
        sections.push(pieces(body, &marks));
    }
    for finding in findings.iter().copied() {
        for passage in &finding.evidence {
            passages.number_of(passage);
        }
    }
    Numbered {
        passages: passages.listed,
        cites,
        sections,
    }
}

#[cfg(test)]
#[path = "cited_text_tests.rs"]
mod tests;
