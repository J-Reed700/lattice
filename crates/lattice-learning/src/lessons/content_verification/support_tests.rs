use super::*;
use crate::content_verification::{CheckedPart, ClaimVerdict, Classed, EvidencePassage, Finding};
use crate::reference_collection::PassageKind;
use lattice_core::application::contracts::replies::{AnchorDto, ByteSpanDto};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::learning::claim_review::{Class, ClassifiedBy};
use lattice_core::domain::provenance::Label;

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

fn finding(class: Option<Class>, quote: &str, evidence: Vec<EvidencePassage>) -> Finding {
    Finding {
        unit: 0,
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

fn anchor(source: &str, start_byte: usize, end_byte: usize) -> Anchor {
    Anchor {
        source_version_id: source.into(),
        start_byte,
        end_byte,
    }
}

fn part(kind: PartKind, anchors: Vec<Anchor>) -> CheckedPart {
    CheckedPart {
        part: "a part".into(),
        kind,
        source_id: None,
        anchors,
    }
}

fn old_part(kind: PartKind, source: &str) -> CheckedPart {
    CheckedPart {
        part: "a part".into(),
        kind,
        source_id: Some(source.into()),
        anchors: Vec::new(),
    }
}

fn starts<'a>(cited: &'a Cited<'_>) -> Vec<(&'a str, usize)> {
    cited
        .on
        .iter()
        .map(|(passage, _)| (passage.source_id.as_str(), passage.start_byte))
        .collect()
}

#[test]
fn an_unsupported_statement_cites_nothing_even_with_stated_parts() {
    for verdict in [
        ClaimVerdict::Unsupported,
        ClaimVerdict::Contradicted,
        ClaimVerdict::Unverified,
    ] {
        let mut found = finding(
            Some(Class::Sourced),
            "q",
            vec![passage("a", 0, "Alpha says one.")],
        );
        found.verdict = verdict;
        found.two_pass.parts = vec![part(PartKind::Stated, vec![anchor("a", 0, 15)])];
        assert!(cited(&found).is_none(), "{verdict:?}");
        found.class = Some(Classed {
            class: Class::Reasoning,
            by: ClassifiedBy::Model,
        });
        assert!(cited(&found).is_none(), "{verdict:?}");
    }
}

#[test]
fn a_common_statement_cites_nothing() {
    let found = finding(Some(Class::Common), "q", vec![passage("a", 0, "text")]);
    assert!(cited(&found).is_none());
}

#[test]
fn a_reasoning_statement_agrees_with_every_passage_whole() {
    let found = finding(
        Some(Class::Reasoning),
        "q",
        vec![
            passage("a", 0, "one"),
            run("run-1", "output"),
            passage("b", 5, "two"),
            passage("a", 0, "one"),
        ],
    );
    let agreed = cited(&found).unwrap();

    assert_eq!(agreed.relation, Relation::Agrees);
    assert!(!agreed.quoted);
    assert_eq!(starts(&agreed), vec![("a", 0), ("b", 5)]);
    assert!(agreed.on.iter().all(|(_, anchors)| anchors.is_empty()));
}

#[test]
fn a_reasoning_statement_with_only_runs_cites_nothing() {
    let found = finding(Some(Class::Reasoning), "q", vec![run("run-1", "output")]);
    assert!(cited(&found).is_none());
}

#[test]
fn a_statement_that_quotes_a_passage_quotes_it_at_the_words_found() {
    let quote = "The quick brown fox jumps over.";
    let found = finding(
        Some(Class::Sourced),
        quote,
        vec![
            run("run-1", quote),
            passage("a", 0, "Nothing of the kind is said here."),
            passage("b", 40, "Intro line. The quick brown fox jumps over. End."),
        ],
    );
    let quotes = cited(&found).unwrap();

    assert_eq!(quotes.relation, Relation::Quotes);
    assert!(quotes.quoted);
    assert_eq!(starts(&quotes), vec![("b", 40)]);
    assert_eq!(quotes.on[0].1, vec![anchor("b", 52, 83)]);
}

#[test]
fn a_quotation_is_found_apart_from_case_and_curly_quotes() {
    let found = finding(
        Some(Class::Sourced),
        "the quick brown \u{201C}fox\u{201D} jumps over.",
        vec![passage("a", 0, "The Quick Brown \"Fox\" Jumps Over.")],
    );
    let quotes = cited(&found).unwrap();

    assert_eq!(quotes.relation, Relation::Quotes);
    assert!(quotes.quoted);
}

#[test]
fn a_quote_too_short_to_stand_as_one_is_not_a_quotation() {
    let found = finding(
        Some(Class::Sourced),
        "The quick brown fox.",
        vec![passage("a", 0, "The quick brown fox.")],
    );
    assert!(cited(&found).is_none());
}

#[test]
fn a_quotation_is_read_before_the_parts() {
    let quote = "The quick brown fox jumps over.";
    let mut found = finding(Some(Class::Sourced), quote, vec![passage("a", 0, quote)]);
    found.two_pass.parts = vec![part(PartKind::Follows, vec![anchor("a", 0, 5)])];
    let quotes = cited(&found).unwrap();

    assert_eq!(quotes.relation, Relation::Quotes);
    assert!(quotes.quoted);
}

#[test]
fn an_attributed_statement_quotes_at_the_anchors_of_its_stated_parts_and_is_not_drawn_quoted() {
    let mut found = finding(
        Some(Class::Sourced),
        "As the guide puts it, tokens come first.",
        vec![passage("a", 10, "Tokens come first. Parsing follows.")],
    );
    found.label = Some(Label::Attributed);
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("a", 10, 28)]),
        part(PartKind::Common, vec![anchor("a", 29, 44)]),
    ];
    let quotes = cited(&found).unwrap();

    assert_eq!(quotes.relation, Relation::Quotes);
    assert!(!quotes.quoted);
    assert_eq!(quotes.on[0].1, vec![anchor("a", 10, 28)]);
}

#[test]
fn an_attributed_statement_from_before_anchors_quotes_at_its_supporting_quote() {
    let mut found = finding(
        Some(Class::Sourced),
        "The guide says it plainly.",
        vec![
            run("run-1", "second passage"),
            passage("a", 10, "Intro. The second passage here."),
        ],
    );
    found.label = Some(Label::Attributed);
    found.supporting_quote = Some("second passage".into());
    found.two_pass.parts = vec![old_part(PartKind::Stated, "a")];
    let quotes = cited(&found).unwrap();

    assert_eq!(quotes.relation, Relation::Quotes);
    assert!(!quotes.quoted);
    assert_eq!(starts(&quotes), vec![("a", 10)]);
    assert_eq!(quotes.on[0].1, vec![anchor("a", 21, 35)]);
}

#[test]
fn a_statement_whose_parts_rest_on_one_sentence_restates_it() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 100, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("a", 100, 115)]),
        part(
            PartKind::Stated,
            vec![anchor("a", 100, 115), anchor("a", 116, 132)],
        ),
    ];
    let restates = cited(&found).unwrap();

    assert_eq!(restates.relation, Relation::Restates);
    assert!(!restates.quoted);
    assert_eq!(restates.on[0].1, vec![anchor("a", 100, 115)]);
}

#[test]
fn a_single_stated_part_on_two_sentences_restates_with_both() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 0, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![part(
        PartKind::Stated,
        vec![anchor("a", 16, 32), anchor("a", 0, 15)],
    )];
    let restates = cited(&found).unwrap();

    assert_eq!(restates.relation, Relation::Restates);
    assert_eq!(
        restates.on[0].1,
        vec![anchor("a", 0, 15), anchor("a", 16, 32)]
    );
}

#[test]
fn stated_parts_on_different_sentences_summarize() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 0, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("a", 16, 32)]),
        part(PartKind::Stated, vec![anchor("a", 0, 15)]),
    ];
    let summarizes = cited(&found).unwrap();

    assert_eq!(summarizes.relation, Relation::Summarizes);
    assert_eq!(
        summarizes.on[0].1,
        vec![anchor("a", 0, 15), anchor("a", 16, 32)]
    );
}

#[test]
fn a_follows_part_makes_the_statement_an_inference_on_every_anchor() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 0, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("a", 0, 15)]),
        part(PartKind::Follows, vec![anchor("a", 16, 32)]),
    ];
    let infers = cited(&found).unwrap();

    assert_eq!(infers.relation, Relation::Infers);
    assert_eq!(
        infers.on[0].1,
        vec![anchor("a", 0, 15), anchor("a", 16, 32)]
    );
}

#[test]
fn the_anchors_of_other_kinds_of_part_are_not_the_statements() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 0, "First sentence. Second sentence.")],
    );
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("a", 0, 15)]),
        part(PartKind::Contradicted, vec![anchor("a", 16, 32)]),
    ];
    let restates = cited(&found).unwrap();

    assert_eq!(restates.relation, Relation::Restates);
    assert_eq!(restates.on[0].1, vec![anchor("a", 0, 15)]);
}

#[test]
fn a_statement_whose_anchors_fall_in_two_passages_gets_an_entry_for_each() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![
            passage("b", 50, "Beta says two. Beta adds three."),
            run("run-1", "output"),
            passage("a", 0, "Alpha says one."),
        ],
    );
    found.two_pass.parts = vec![
        part(PartKind::Stated, vec![anchor("b", 65, 81)]),
        part(PartKind::Stated, vec![anchor("a", 0, 15)]),
        part(PartKind::Stated, vec![anchor("b", 50, 64)]),
        part(PartKind::Stated, vec![anchor("c", 0, 4)]),
    ];
    let summarizes = cited(&found).unwrap();

    assert_eq!(summarizes.relation, Relation::Summarizes);
    assert_eq!(starts(&summarizes), vec![("a", 0), ("b", 50)]);
    assert_eq!(summarizes.on[0].1, vec![anchor("a", 0, 15)]);
    assert_eq!(
        summarizes.on[1].1,
        vec![anchor("b", 50, 64), anchor("b", 65, 81)]
    );
}

#[test]
fn anchors_in_no_passage_leave_nothing_to_cite() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![passage("a", 0, "Alpha says one.")],
    );
    found.two_pass.parts = vec![part(PartKind::Stated, vec![anchor("a", 40, 60)])];
    assert!(cited(&found).is_none());
}

#[test]
fn a_supporting_quote_inside_a_passage_backs_the_sentences_it_lies_in() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![
            run("run-1", "first. Two is"),
            passage("a", 30, "One is first. Two is second. Three is third."),
        ],
    );
    found.supporting_quote = Some("first. Two is".into());
    let backs = cited(&found).unwrap();

    assert_eq!(backs.relation, Relation::Backs);
    assert!(!backs.quoted);
    assert_eq!(starts(&backs), vec![("a", 30)]);
    assert_eq!(backs.on[0].1, vec![anchor("a", 30, 58)]);
}

#[test]
fn a_finding_from_before_anchors_backs_the_passages_of_its_stated_parts_whole() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![
            passage("a", 0, "Alpha says one."),
            passage("b", 0, "Beta says two."),
            passage("a", 40, "Alpha says more."),
        ],
    );
    found.two_pass.parts = vec![
        old_part(PartKind::Stated, "b"),
        old_part(PartKind::Common, "a"),
        old_part(PartKind::Stated, "a"),
        old_part(PartKind::Stated, "b"),
    ];
    let backs = cited(&found).unwrap();

    assert_eq!(backs.relation, Relation::Backs);
    assert!(!backs.quoted);
    assert_eq!(starts(&backs), vec![("b", 0), ("a", 0)]);
    assert!(backs.on.iter().all(|(_, anchors)| anchors.is_empty()));
}

#[test]
fn a_supporting_quote_equal_to_a_whole_passage_backs_that_passage_whole() {
    let mut found = finding(
        Some(Class::Sourced),
        "The lesson's own words.",
        vec![
            passage("a", 0, "Alpha says one."),
            passage("a", 40, "Alpha says more."),
        ],
    );
    found.supporting_quote = Some("Alpha says more.".into());
    let backs = cited(&found).unwrap();

    assert_eq!(backs.relation, Relation::Backs);
    assert_eq!(starts(&backs), vec![("a", 40)]);
    assert!(backs.on[0].1.is_empty());
}

#[test]
fn a_statement_the_check_named_nothing_for_cites_nothing() {
    let mut found = finding(
        None,
        "The lesson's own words.",
        vec![passage("a", 0, "Alpha says one.")],
    );
    assert!(cited(&found).is_none());
    found.supporting_quote = Some("nowhere".into());
    found.two_pass.parts = vec![part(PartKind::Common, Vec::new())];
    assert!(cited(&found).is_none());
}

#[test]
fn a_run_is_never_cited() {
    let quote = "The quick brown fox jumps over.";
    for class in [None, Some(Class::Sourced), Some(Class::Reasoning)] {
        let mut found = finding(class, quote, vec![run("run-1", quote)]);
        found.two_pass.parts = vec![
            old_part(PartKind::Stated, "run-1"),
            part(PartKind::Stated, vec![anchor("run-1", 0, 5)]),
        ];
        found.supporting_quote = Some(quote.into());
        assert!(cited(&found).is_none(), "{class:?}");
    }
}

#[test]
fn an_anchor_is_drawn_with_the_words_of_its_passage_and_its_version_bytes() {
    let held = passage("a", 100, "First sentence. Second sentence.");
    assert_eq!(
        anchor_dto(&held, &anchor("a", 116, 132)),
        Some(AnchorDto {
            exact: "Second sentence.".into(),
            span: Some(ByteSpanDto {
                start: 116,
                end: 132
            }),
        })
    );
}

#[test]
fn an_anchor_outside_the_passage_has_no_words() {
    let held = passage("a", 100, "First sentence. Second sentence.");
    assert_eq!(anchor_dto(&held, &anchor("b", 100, 115)), None);
    assert_eq!(anchor_dto(&held, &anchor("a", 90, 115)), None);
    assert_eq!(anchor_dto(&held, &anchor("a", 116, 140)), None);
}
