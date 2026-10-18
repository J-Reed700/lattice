//! Packing against a measure that counts characters: what the planner does
//! with the window, not what a window is.

use super::*;
use crate::features::learning::lessons::verifier_audit::AuditPassage;
use lattice_core::domain::learning::claim_review::ClaimId;

/// Fits while the batch's characters stay within `budget`: each claim's
/// statement and chosen passages, its section once (or its paragraph), and
/// a fixed answer per item.
struct Chars<'a> {
    claims: &'a [AuditClaim],
    budget: usize,
}

const SECTION: usize = 100;
const PARAGRAPH: usize = 10;
const ANSWER: usize = 5;

impl Sizer for Chars<'_> {
    fn fits(&self, items: &[Item]) -> bool {
        self.needed(items) <= self.budget
    }
    fn needed(&self, items: &[Item]) -> usize {
        let mut sections: Vec<usize> = Vec::new();
        items
            .iter()
            .map(|item| {
                let claim = &self.claims[item.claim];
                let passages: usize = item
                    .passages
                    .iter()
                    .map(|index| claim.passages[*index].text.len())
                    .sum();
                let context = match item.context {
                    Context::Unit if sections.contains(&claim.unit_index) => 0,
                    Context::Unit => {
                        sections.push(claim.unit_index);
                        SECTION
                    }
                    Context::Paragraph => PARAGRAPH,
                };
                claim.statement.len() + passages + context + ANSWER
            })
            .sum()
    }
}

fn claim(statement: &str, passages: &[usize]) -> AuditClaim {
    AuditClaim {
        claim_id: ClaimId(statement.into()),
        unit_index: 0,
        statement: statement.into(),
        quote: statement.into(),
        passages: passages
            .iter()
            .enumerate()
            .map(|(index, length)| AuditPassage {
                source_version_id: format!("v{index}"),
                title: "Handbook".into(),
                start_byte: 0,
                end_byte: *length,
                text: "x".repeat(*length),
            })
            .collect(),
        general_knowledge: false,
    }
}

fn whole(claim: usize, passages: usize) -> Item {
    Item {
        claim,
        passages: (0..passages).collect(),
        context: Context::Unit,
        part: None,
    }
}

#[test]
fn claims_go_in_lesson_order_into_the_fewest_batches_that_fit() {
    // Each claim: 10 + 20 + 5 = 35, and the section 100 once per batch.
    let claims: Vec<AuditClaim> = ["aaaaaaaaaa", "bbbbbbbbbb", "cccccccccc"]
        .iter()
        .map(|statement| claim(statement, &[20]))
        .collect();
    let planned = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 170,
        },
    );
    assert_eq!(
        planned,
        Planned {
            batches: vec![vec![whole(0, 1), whole(1, 1)], vec![whole(2, 1)]],
            oversize: vec![],
        }
    );
}

#[test]
fn a_claim_too_large_whole_is_split_by_passages_into_numbered_parts() {
    // Whole: 1 + 300 + 100 + 5 = 406 > 260; beside the section and the
    // answer (106), 154 is left for passages: two of 70, then the last.
    let claims = vec![claim("a", &[70, 70, 70, 70, 20])];
    let planned = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 260,
        },
    );
    let parts: Vec<(Vec<usize>, Option<(usize, usize)>)> = planned
        .batches
        .iter()
        .flatten()
        .map(|item| (item.passages.clone(), item.part))
        .collect();
    assert_eq!(
        parts,
        vec![
            (vec![0, 1], Some((1, 3))),
            (vec![2, 3], Some((2, 3))),
            (vec![4], Some((3, 3))),
        ]
    );
    assert_eq!(
        planned.batches.len(),
        3,
        "each part needs its own request here"
    );
    assert!(planned.oversize.is_empty());
}

#[test]
fn a_passage_too_large_beside_its_section_is_sent_with_its_paragraph() {
    // 1 + 150 + 100 + 5 = 256 > 200, but 1 + 150 + 10 + 5 = 166 fits.
    let claims = vec![claim("a", &[150])];
    let planned = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 200,
        },
    );
    assert_eq!(
        planned.batches,
        vec![vec![Item {
            claim: 0,
            passages: vec![0],
            context: Context::Paragraph,
            part: None,
        }]]
    );
}

#[test]
fn a_claim_nothing_holds_is_reported_with_what_it_needs_and_the_rest_go_on() {
    let claims = vec![claim("a", &[20]), claim("b", &[500]), claim("c", &[20])];
    let planned = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 200,
        },
    );
    assert_eq!(
        planned.oversize,
        vec![Oversize {
            claim: 1,
            needed_tokens: 1 + 500 + PARAGRAPH + ANSWER
        }]
    );
    let sent: Vec<usize> = planned
        .batches
        .iter()
        .flatten()
        .map(|item| item.claim)
        .collect();
    assert_eq!(sent, vec![0, 2]);
}

#[test]
fn a_claim_without_passages_falls_back_to_its_paragraph_or_is_oversize() {
    let long = "s".repeat(120);
    let claims = vec![claim(&long, &[])];
    let fits_reduced = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 200,
        },
    );
    assert_eq!(fits_reduced.batches[0][0].context, Context::Paragraph);
    let too_small = plan(
        &claims,
        &Chars {
            claims: &claims,
            budget: 100,
        },
    );
    assert!(too_small.batches.is_empty());
    assert_eq!(
        too_small.oversize[0].needed_tokens,
        120 + PARAGRAPH + ANSWER
    );
}

#[test]
fn the_paragraph_is_the_one_holding_the_quote_or_the_quote_itself() {
    let text = "First paragraph.\n\nWater boils at 100 °C. Salt raises it.\n\nLast.";
    assert_eq!(
        paragraph(text, "Water boils at 100 °C."),
        "Water boils at 100 °C. Salt raises it."
    );
    assert_eq!(paragraph(text, "not in the text"), "not in the text");
    assert_eq!(paragraph(text, ""), "");
}
