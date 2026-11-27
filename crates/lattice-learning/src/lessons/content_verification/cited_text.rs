//! Where a lesson's citation chips go, which passages they open and how they
//! are numbered down the lesson.
use super::support::{anchor_dto, cited, Cited};
use super::{EvidencePassage, Finding};
use lattice_core::application::contracts::replies::{ReplyPieceDto, ReplyReferenceDto, SupportDto};
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

/// Where in a section's body a statement's citation goes, as the start of the
/// quote's first copy outside a fenced code block and the point after it: the
/// end of that copy, moved past an inline code span it ends inside; none when
/// the quote is not in the body outside a fence.
pub(super) fn citation_point(body: &str, quote: &str) -> Option<(usize, usize)> {
    if quote.trim().is_empty() {
        return None;
    }
    let fences = code_fences(body);
    let (start, end) = body
        .match_indices(quote)
        .map(|(at, _)| (at, at + quote.len()))
        .find(|&(_, end)| {
            !fences.iter().any(|fence| {
                let whole = fence.whole();
                whole.start < end && end < whole.end
            })
        })?;
    let line_start = body.get(..end)?.rfind('\n').map_or(0, |at| at + 1);
    let line_end = body
        .get(end..)?
        .find('\n')
        .map_or(body.len(), |at| end + at);
    let line = body.get(line_start..line_end)?;
    let at = end - line_start;
    let point = code_spans(line)
        .into_iter()
        .find(|span| span.start < at && at < span.end)
        .map_or(end, |span| line_start + span.end);
    Some((start, point))
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

    /// The numbers of the passages `found` opens, each with the support its
    /// chip carries: the relation and the anchors that lie in its own passage.
    fn chips(&mut self, found: &Cited<'f>) -> Vec<(u32, SupportDto)> {
        found
            .on
            .iter()
            .map(|(passage, anchors)| {
                let passage = *passage;
                let support = SupportDto {
                    relation: found.relation.into(),
                    anchors: anchors
                        .iter()
                        .filter_map(|anchor| anchor_dto(passage, anchor))
                        .collect(),
                };
                (self.number_of(passage), support)
            })
            .collect()
    }
}

type Marks = BTreeMap<usize, BTreeMap<u32, SupportDto>>;

fn reference(number: u32, support: &SupportDto) -> ReplyPieceDto {
    ReplyPieceDto::Reference {
        reference: ReplyReferenceDto {
            written: format!("[{number}]"),
            kind: "source".into(),
            number,
            lines: None,
            name: None,
            support: Some(support.clone()),
        },
    }
}

/// The spans of a body that are drawn as quotations: each of the `copies`
/// that overlaps none already kept and has no other statement's chip inside it.
fn quotations(copies: Vec<(usize, usize)>, marks: &Marks) -> Vec<(usize, usize)> {
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for (start, end) in copies {
        let overlaps = kept.iter().any(|&(from, to)| start < to && from < end);
        let split = marks.keys().any(|&point| start < point && point < end);
        if !overlaps && !split {
            kept.push((start, end));
        }
    }
    kept
}

fn pieces(body: &str, marks: &Marks, quotations: &[(usize, usize)]) -> Vec<ReplyPieceDto> {
    let mut cuts: BTreeSet<usize> = marks.keys().copied().collect();
    cuts.extend(quotations.iter().flat_map(|&(start, end)| [start, end]));
    let mut pieces = Vec::new();
    let mut from = 0;
    for cut in cuts {
        if let Some(before) = body.get(from..cut).filter(|before| !before.is_empty()) {
            let text = before.into();
            pieces.push(
                if quotations
                    .iter()
                    .any(|&(start, end)| start <= from && cut <= end)
                {
                    ReplyPieceDto::Quoted { text }
                } else {
                    ReplyPieceDto::Text { text }
                },
            );
        }
        if let Some(chips) = marks.get(&cut) {
            pieces.extend(
                chips
                    .iter()
                    .map(|(&number, support)| reference(number, support)),
            );
        }
        from = cut;
    }
    if let Some(rest) = body.get(from..).filter(|rest| !rest.is_empty()) {
        pieces.push(ReplyPieceDto::Text { text: rest.into() });
    }
    pieces
}

/// Numbers the passages the `findings` cite down the lesson and cuts each
/// body, `bodies[unit]`, at its citation points: every placed statement first,
/// block by block in reading order, then the statements without a chip;
/// passages only compared come last and get no chip.
pub(super) fn number<'f>(bodies: &[&str], findings: &[&'f Finding]) -> Numbered<'f> {
    let mut passages = Passages::default();
    let mut cites = vec![Vec::new(); findings.len()];
    let mut sections = Vec::with_capacity(bodies.len());
    let mut unplaced = Vec::new();
    for (unit, body) in bodies.iter().enumerate() {
        let mut placed = Vec::new();
        for (index, finding) in findings.iter().copied().enumerate() {
            if finding.unit != unit {
                continue;
            }
            let Some(found) = cited(finding) else {
                continue;
            };
            match citation_point(body, &finding.quote) {
                Some((start, point)) => {
                    placed.push((point, index, (start, start + finding.quote.len()), found));
                }
                None => unplaced.push((index, found)),
            }
        }
        placed.sort_by_key(|(point, index, _, _)| (*point, *index));
        let mut marks = Marks::new();
        let mut copies = Vec::new();
        for (point, index, copy, found) in placed {
            let chips = passages.chips(&found);
            if let Some(slot) = cites.get_mut(index) {
                *slot = chips.iter().map(|(number, _)| *number).collect();
            }
            let at = marks.entry(point).or_default();
            for (number, support) in chips {
                at.entry(number).or_insert(support);
            }
            if found.quoted && copy.1 == point {
                copies.push(copy);
            }
        }
        let quoted = quotations(copies, &marks);
        sections.push(pieces(body, &marks, &quoted));
    }
    for (index, found) in unplaced {
        let numbers = passages
            .chips(&found)
            .into_iter()
            .map(|(number, _)| number)
            .collect();
        if let Some(slot) = cites.get_mut(index) {
            *slot = numbers;
        }
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
