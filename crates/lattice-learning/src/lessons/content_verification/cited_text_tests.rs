use super::*;
use crate::content_verification::support::{cited, Cited};
use crate::content_verification::{CheckedPart, ClaimVerdict, Classed, EvidencePassage, Finding};
use crate::reference_collection::PassageKind;
use lattice_core::application::contracts::replies::{
    AnchorDto, ByteSpanDto, RelationDto, ReplyPieceDto, ReplyReferenceDto, SupportDto,
};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::citation::{Anchor, Relation};
use lattice_core::domain::learning::claim_review::{Class, ClassifiedBy};

fn passage(source: &str, start: usize, text: &str) -> EvidencePassage {
    EvidencePassage {
        source_id: source.into(),
        text: text.into(),
        start_byte: start,
        end_byte: start + text.len(),
        retrieval_kind: PassageKind::Cited.label().into(),
        score: 1.0,
    }
}

fn run(id: &str, text: &str) -> EvidencePassage {
    EvidencePassage {
        retrieval_kind: PassageKind::Run.label().into(),
        ..passage(id, 0, text)
    }
}

fn finding(
    unit: usize,
    quote: &str,
    class: Option<Class>,
    evidence: Vec<EvidencePassage>,
) -> Finding {
    Finding {
        unit,
        quote: quote.into(),
        statement: quote.into(),
        verdict: ClaimVerdict::Supported,
        reason: String::new(),
        evidence,
        supporting_quote: None,
        resolution: None,
        claim_id: None,
        trust: None,
        label: None,
        class: class.map(|class| Classed {
            class,
            by: ClassifiedBy::Model,
        }),
        two_pass: Default::default(),
    }
}

fn stated(source: &str) -> CheckedPart {
    CheckedPart {
        part: "a part".into(),
        kind: PartKind::Stated,
        source_id: Some(source.into()),
        anchors: Vec::new(),
    }
}

fn anchored(source: &str, kind: PartKind, spans: &[(usize, usize)]) -> CheckedPart {
    CheckedPart {
        part: "a part".into(),
        kind,
        source_id: None,
        anchors: spans
            .iter()
            .map(|&(start_byte, end_byte)| Anchor {
                source_version_id: source.into(),
                start_byte,
                end_byte,
            })
            .collect(),
    }
}

fn places(found: Option<Cited<'_>>) -> Vec<(&str, usize)> {
    found
        .map(|found| found.on)
        .unwrap_or_default()
        .into_iter()
        .map(|(passage, _)| (passage.source_id.as_str(), passage.start_byte))
        .collect()
}

fn ids<'a>(numbered: &'a Numbered<'_>) -> Vec<(&'a str, usize)> {
    numbered
        .passages
        .iter()
        .map(|passage| (passage.source_id.as_str(), passage.start_byte))
        .collect()
}

fn numbering<'f>(bodies: &[&str], findings: &'f [Finding]) -> Numbered<'f> {
    let all: Vec<&Finding> = findings.iter().collect();
    number(bodies, &all)
}

fn text(body: &str) -> ReplyPieceDto {
    ReplyPieceDto::Text { text: body.into() }
}

fn cite_as(n: u32, relation: RelationDto, anchors: Vec<AnchorDto>) -> ReplyPieceDto {
    ReplyPieceDto::Reference {
        reference: ReplyReferenceDto {
            written: format!("[{n}]"),
            kind: "source".into(),
            number: n,
            lines: None,
            name: None,
            support: Some(SupportDto { relation, anchors }),
        },
    }
}

fn cite(n: u32) -> ReplyPieceDto {
    cite_as(n, RelationDto::Agrees, Vec::new())
}

fn words(exact: &str, start: usize, end: usize) -> AnchorDto {
    AnchorDto {
        exact: exact.into(),
        span: Some(ByteSpanDto { start, end }),
    }
}

fn quoted(body: &str) -> ReplyPieceDto {
    ReplyPieceDto::Quoted { text: body.into() }
}

fn rebuilt(pieces: &[ReplyPieceDto]) -> String {
    pieces
        .iter()
        .filter_map(|piece| match piece {
            ReplyPieceDto::Text { text } | ReplyPieceDto::Quoted { text } => Some(text.as_str()),
            ReplyPieceDto::Reference { .. } => None,
        })
        .collect()
}

#[test]
fn a_citation_goes_after_the_sentence_it_backs() {
    assert_eq!(
        citation_point(
            "Rust has ownership. It frees memory.",
            "Rust has ownership."
        ),
        Some((0, 19))
    );
}

#[test]
fn a_quote_that_is_not_in_the_body_or_is_blank_has_no_point() {
    assert_eq!(citation_point("Rust has ownership.", "Go has a GC."), None);
    assert_eq!(citation_point("Rust has ownership.", ""), None);
    assert_eq!(citation_point("Rust has ownership.", "   "), None);
}

#[test]
fn a_quote_inside_a_fenced_block_has_no_point() {
    let body = "Intro.\n\n```text\nThe value is 4.\n```\n";
    assert_eq!(citation_point(body, "The value is 4."), None);
}

#[test]
fn a_quote_that_ends_right_before_a_fence_keeps_its_end() {
    let body = "Intro.\n\n```text\ncode\n```\n";
    assert_eq!(citation_point(body, "Intro."), Some((0, 6)));
}

#[test]
fn a_quote_after_a_fence_is_placed_at_its_end() {
    let body = "```text\ncode\n```\nAfter it.";
    assert_eq!(
        citation_point(body, "After it."),
        Some((body.len() - 9, body.len()))
    );
}

#[test]
fn a_quote_that_ends_inside_a_code_span_moves_to_after_its_closing_backticks() {
    let body = "Use `x.y` to read it.";
    assert_eq!(citation_point(body, "Use `x."), Some((0, 9)));
    let body = "Call ``a`b`` now.";
    assert_eq!(citation_point(body, "Call ``a"), Some((0, 12)));
}

#[test]
fn a_quote_that_ends_right_before_a_code_span_keeps_its_end() {
    let body = "Use `x.y` to read it.";
    assert_eq!(citation_point(body, "Use "), Some((0, 4)));
}

#[test]
fn a_quote_that_ends_right_after_a_code_span_keeps_its_end() {
    let body = "Use `x.y` to read it.";
    assert_eq!(citation_point(body, "Use `x.y`"), Some((0, 9)));
}

#[test]
fn a_common_statement_cites_nothing() {
    let found = finding(0, "q", Some(Class::Common), vec![passage("a", 0, "text")]);
    assert!(cited(&found).is_none());
}

#[test]
fn a_reasoning_statement_cites_every_passage_but_runs_each_once() {
    let found = finding(
        0,
        "q",
        Some(Class::Reasoning),
        vec![
            passage("a", 0, "one"),
            run("run-1", "output"),
            passage("b", 5, "two"),
            passage("a", 0, "one"),
        ],
    );
    let agreed = cited(&found).unwrap();
    assert_eq!(agreed.relation, Relation::Agrees);
    assert!(agreed.on.iter().all(|(_, anchors)| anchors.is_empty()));
    assert_eq!(places(Some(agreed)), vec![("a", 0), ("b", 5)]);
}

#[test]
fn a_sourced_statement_cites_the_first_passage_of_each_source_it_states_in_part_order() {
    let mut found = finding(
        0,
        "q",
        Some(Class::Sourced),
        vec![
            passage("a", 0, "a first"),
            passage("b", 0, "b first"),
            passage("a", 10, "a second"),
            passage("c", 0, "c first"),
        ],
    );
    found.two_pass.parts = vec![
        stated("b"),
        CheckedPart {
            part: "common".into(),
            kind: PartKind::Common,
            source_id: None,
            anchors: Vec::new(),
        },
        stated("a"),
        stated("b"),
    ];
    assert_eq!(places(cited(&found)), vec![("b", 0), ("a", 0)]);
}

#[test]
fn a_sourced_statement_without_stated_parts_cites_the_sentence_holding_its_supporting_quote() {
    let mut found = finding(
        0,
        "q",
        Some(Class::Sourced),
        vec![
            passage("a", 0, "the first passage."),
            passage("a", 20, "the second passage. Another sentence."),
        ],
    );
    found.supporting_quote = Some("second passage".into());
    let backed = cited(&found).unwrap();
    assert_eq!(backed.relation, Relation::Backs);
    assert_eq!(backed.on.len(), 1);
    assert_eq!(backed.on[0].0.start_byte, 20);
    assert_eq!(
        backed.on[0].1,
        vec![Anchor {
            source_version_id: "a".into(),
            start_byte: 20,
            end_byte: 39,
        }]
    );
}

#[test]
fn a_sourced_statement_whose_quote_is_in_no_passage_cites_nothing() {
    let mut found = finding(
        0,
        "q",
        Some(Class::Sourced),
        vec![
            passage("a", 0, "the first passage"),
            passage("a", 20, "the second passage"),
        ],
    );
    found.supporting_quote = Some("nowhere".into());
    assert!(cited(&found).is_none());
    found.supporting_quote = None;
    assert!(cited(&found).is_none());
}

#[test]
fn a_statement_with_no_class_is_read_as_sourced() {
    let mut found = finding(
        0,
        "q",
        None,
        vec![passage("a", 0, "one"), passage("b", 0, "two")],
    );
    found.two_pass.parts = vec![stated("b")];
    assert_eq!(places(cited(&found)), vec![("b", 0)]);
    found.two_pass.parts.clear();
    assert!(cited(&found).is_none());
}

#[test]
fn a_run_is_never_cited() {
    for class in [None, Some(Class::Sourced), Some(Class::Reasoning)] {
        let mut found = finding(0, "q", class, vec![run("run-1", "output")]);
        found.two_pass.parts = vec![stated("run-1")];
        found.supporting_quote = Some("output".into());
        assert!(cited(&found).is_none(), "{class:?}");
    }
}

#[test]
fn numbers_follow_reading_order_across_blocks_even_when_report_order_differs() {
    let findings = vec![
        finding(
            1,
            "B two.",
            Some(Class::Reasoning),
            vec![passage("b", 20, "pb2")],
        ),
        finding(
            1,
            "B one.",
            Some(Class::Reasoning),
            vec![passage("b", 10, "pb1")],
        ),
        finding(
            0,
            "A one.",
            Some(Class::Reasoning),
            vec![passage("a", 0, "pa")],
        ),
    ];
    let numbered = numbering(&["A one.", "B one. B two."], &findings);

    assert_eq!(ids(&numbered), vec![("a", 0), ("b", 10), ("b", 20)]);
    assert_eq!(numbered.cites, vec![vec![3], vec![2], vec![1]]);
    assert_eq!(numbered.sections[0], vec![text("A one."), cite(1)]);
    assert_eq!(
        numbered.sections[1],
        vec![text("B one."), cite(2), text(" B two."), cite(3)]
    );
}

#[test]
fn a_passage_cited_by_two_statements_keeps_one_number() {
    let shared = passage("a", 0, "shared");
    let findings = vec![
        finding(0, "One fact.", Some(Class::Reasoning), vec![shared.clone()]),
        finding(0, "Another fact.", Some(Class::Reasoning), vec![shared]),
    ];
    let numbered = numbering(&["One fact. Another fact."], &findings);

    assert_eq!(numbered.passages.len(), 1);
    assert_eq!(numbered.cites, vec![vec![1], vec![1]]);
    assert_eq!(
        numbered.sections[0],
        vec![text("One fact."), cite(1), text(" Another fact."), cite(1)]
    );
}

#[test]
fn two_statements_on_one_sentence_share_one_point_with_ascending_references() {
    let (pa, pb, pc) = (
        passage("a", 0, "pa"),
        passage("b", 0, "pb"),
        passage("c", 0, "pc"),
    );
    let findings = vec![
        finding(
            0,
            "Rust has ownership.",
            Some(Class::Reasoning),
            vec![pb.clone(), pa.clone()],
        ),
        finding(
            0,
            "Rust has ownership.",
            Some(Class::Reasoning),
            vec![pa, pc],
        ),
    ];
    let numbered = numbering(&["Rust has ownership. Next."], &findings);

    assert_eq!(ids(&numbered), vec![("b", 0), ("a", 0), ("c", 0)]);
    assert_eq!(numbered.cites, vec![vec![1, 2], vec![2, 3]]);
    assert_eq!(
        numbered.sections[0],
        vec![
            text("Rust has ownership."),
            cite(1),
            cite(2),
            cite(3),
            text(" Next.")
        ]
    );
}

#[test]
fn compared_only_passages_are_numbered_after_every_cited_one_and_get_no_chip() {
    let mut first = finding(
        0,
        "One.",
        Some(Class::Sourced),
        vec![passage("a", 0, "p1"), passage("a", 10, "p2")],
    );
    first.two_pass.parts = vec![anchored("a", PartKind::Stated, &[(0, 2)])];
    let findings = vec![
        first,
        finding(
            1,
            "Two.",
            Some(Class::Reasoning),
            vec![passage("b", 0, "p3")],
        ),
    ];
    let numbered = numbering(&["One.", "Two."], &findings);

    assert_eq!(ids(&numbered), vec![("a", 0), ("b", 0), ("a", 10)]);
    assert_eq!(numbered.cites, vec![vec![1], vec![2]]);
    assert_eq!(
        numbered.sections[0],
        vec![
            text("One."),
            cite_as(1, RelationDto::States, vec![words("p1", 0, 2)]),
        ]
    );
    assert_eq!(numbered.sections[1], vec![text("Two."), cite(2)]);
}

#[test]
fn a_statement_without_a_chip_is_numbered_after_every_placed_one_in_the_lesson() {
    let findings = vec![
        finding(
            0,
            "Title text.",
            Some(Class::Reasoning),
            vec![passage("t", 0, "pt")],
        ),
        finding(
            0,
            "Body sentence.",
            Some(Class::Reasoning),
            vec![passage("b", 0, "pb")],
        ),
        finding(
            1,
            "Later.",
            Some(Class::Reasoning),
            vec![passage("l", 0, "pl")],
        ),
    ];
    let numbered = numbering(&["Body sentence. More.", "Later."], &findings);

    assert_eq!(ids(&numbered), vec![("b", 0), ("l", 0), ("t", 0)]);
    assert_eq!(numbered.cites, vec![vec![3], vec![1], vec![2]]);
    assert_eq!(
        numbered.sections[0],
        vec![text("Body sentence."), cite(1), text(" More.")]
    );
    assert_eq!(numbered.sections[1], vec![text("Later."), cite(2)]);
}

#[test]
fn a_quote_first_found_inside_a_fence_is_placed_at_its_copy_in_prose() {
    let body = "```text
Rust has ownership.
```
Rust has ownership. Next.";
    let quote = "Rust has ownership.";
    let start = body.rfind(quote).unwrap();
    assert_eq!(
        citation_point(body, quote),
        Some((start, start + quote.len()))
    );
}

#[test]
fn the_text_pieces_rebuild_every_body_exactly() {
    let bodies = [
        "First one. Second one.\n\n- item a.\n- item b.\n",
        "**Bold sentence.** Next.",
    ];
    let reason = Some(Class::Reasoning);
    let findings = vec![
        finding(0, "First one.", reason, vec![passage("a", 0, "p1")]),
        finding(0, "Second one.", reason, vec![passage("a", 10, "p2")]),
        finding(0, "item a.", reason, vec![passage("a", 20, "p3")]),
        finding(0, "item b.", reason, vec![passage("a", 30, "p4")]),
        finding(1, "Bold sentence.", reason, vec![passage("b", 0, "p5")]),
    ];
    let numbered = numbering(&bodies, &findings);

    assert_eq!(rebuilt(&numbered.sections[0]), bodies[0]);
    assert_eq!(rebuilt(&numbered.sections[1]), bodies[1]);
    let marks = |pieces: &[ReplyPieceDto]| {
        pieces
            .iter()
            .filter(|piece| matches!(piece, ReplyPieceDto::Reference { .. }))
            .count()
    };
    assert_eq!(marks(&numbered.sections[0]), 4);
    assert_eq!(marks(&numbered.sections[1]), 1);
}

#[test]
fn a_body_without_citations_is_one_text_piece_and_an_empty_body_has_none() {
    let findings = vec![finding(
        0,
        "Body.",
        Some(Class::Common),
        vec![passage("a", 0, "p")],
    )];
    let numbered = numbering(&["Body.", ""], &findings);

    assert_eq!(numbered.sections[0], vec![text("Body.")]);
    assert!(numbered.sections[1].is_empty());
    assert_eq!(numbered.cites, vec![Vec::<u32>::new()]);
    assert_eq!(ids(&numbered), vec![("a", 0)]);
}

#[test]
fn a_statement_that_is_not_supported_gets_no_chip() {
    let mut found = finding(
        0,
        "Rust has ownership.",
        Some(Class::Reasoning),
        vec![passage("a", 0, "pa")],
    );
    found.verdict = ClaimVerdict::Unsupported;
    let findings = [found];
    let numbered = numbering(&["Rust has ownership."], &findings);

    assert_eq!(numbered.cites, vec![Vec::<u32>::new()]);
    assert_eq!(numbered.sections[0], vec![text("Rust has ownership.")]);
}

#[test]
fn a_chip_carries_the_relation_and_the_anchors_of_its_own_passage() {
    let mut found = finding(
        0,
        "The lesson says the second.",
        Some(Class::Sourced),
        vec![passage("a", 100, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![anchored("a", PartKind::Stated, &[(116, 132)])];
    let findings = [found];
    let numbered = numbering(&["The lesson says the second."], &findings);

    assert_eq!(
        numbered.sections[0],
        vec![
            text("The lesson says the second."),
            cite_as(
                1,
                RelationDto::Restates,
                vec![words("Second sentence.", 116, 132)]
            ),
        ]
    );
    assert_eq!(numbered.cites, vec![vec![1]]);
}

#[test]
fn a_statement_on_two_passages_gets_two_chips_each_with_its_own_anchors() {
    let mut found = finding(
        0,
        "The lesson joins both.",
        Some(Class::Sourced),
        vec![
            passage("a", 0, "Alpha says one."),
            passage("b", 50, "Beta says two."),
        ],
    );
    found.two_pass.parts = vec![
        anchored("a", PartKind::Stated, &[(0, 15)]),
        anchored("b", PartKind::Stated, &[(50, 64)]),
    ];
    let findings = [found];
    let numbered = numbering(&["The lesson joins both."], &findings);

    assert_eq!(numbered.cites, vec![vec![1, 2]]);
    assert_eq!(
        numbered.sections[0],
        vec![
            text("The lesson joins both."),
            cite_as(
                1,
                RelationDto::Summarizes,
                vec![words("Alpha says one.", 0, 15)]
            ),
            cite_as(
                2,
                RelationDto::Summarizes,
                vec![words("Beta says two.", 50, 64)]
            ),
        ]
    );
}

#[test]
fn two_statements_at_one_point_keep_each_number_once_with_the_first_statements_support() {
    let shared = passage("a", 0, "Alpha says one.");
    let mut backed = finding(
        0,
        "Rust has ownership.",
        Some(Class::Sourced),
        vec![shared.clone()],
    );
    backed.two_pass.parts = vec![stated("a")];
    let findings = vec![
        finding(
            0,
            "Rust has ownership.",
            Some(Class::Reasoning),
            vec![shared],
        ),
        backed,
    ];
    let numbered = numbering(&["Rust has ownership."], &findings);

    assert_eq!(numbered.cites, vec![vec![1], vec![1]]);
    assert_eq!(
        numbered.sections[0],
        vec![text("Rust has ownership."), cite(1)]
    );
}

fn quotation_of(quote: &str, source: &str) -> Finding {
    finding(
        0,
        quote,
        Some(Class::Sourced),
        vec![passage("a", 0, source)],
    )
}

#[test]
fn a_quoted_statement_is_drawn_as_a_quoted_piece_with_its_chip_after_it() {
    let quote = "The quick brown fox jumps over.";
    let found = quotation_of(quote, quote);
    let body = format!("He wrote: {quote} Then left.");
    let findings = [found];
    let numbered = numbering(&[body.as_str()], &findings);

    assert_eq!(
        numbered.sections[0],
        vec![
            text("He wrote: "),
            quoted(quote),
            cite_as(1, RelationDto::Quotes, vec![words(quote, 0, 31)]),
            text(" Then left."),
        ]
    );
    assert_eq!(rebuilt(&numbered.sections[0]), body);
}

#[test]
fn a_quoted_statement_whose_point_moved_past_a_code_span_stays_unquoted() {
    let source = "Read the value of `config.name.first` aloud.";
    let quote = "Read the value of `config.name";
    let found = quotation_of(quote, source);
    let findings = [found];
    let numbered = numbering(&[source], &findings);

    assert_eq!(
        numbered.sections[0],
        vec![
            text("Read the value of `config.name.first`"),
            cite_as(1, RelationDto::Quotes, vec![words(quote, 0, 30)]),
            text(" aloud."),
        ]
    );
}

#[test]
fn a_quoted_span_that_overlaps_one_already_quoted_stays_text() {
    let first = "The quick brown fox jumps over";
    let second = "brown fox jumps over the lazy dog.";
    let body = "The quick brown fox jumps over the lazy dog.";
    let findings = vec![
        quotation_of(first, body),
        finding(
            0,
            second,
            Some(Class::Sourced),
            vec![passage("b", 0, second)],
        ),
    ];
    let numbered = numbering(&[body], &findings);

    assert_eq!(numbered.sections[0].len(), 4);
    assert_eq!(numbered.sections[0][0], quoted(first));
    assert!(matches!(
        numbered.sections[0][1],
        ReplyPieceDto::Reference { .. }
    ));
    assert_eq!(numbered.sections[0][2], text(" the lazy dog."));
    assert!(matches!(
        numbered.sections[0][3],
        ReplyPieceDto::Reference { .. }
    ));
    assert_eq!(rebuilt(&numbered.sections[0]), body);
}
