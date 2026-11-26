use super::*;
use crate::content_verification::{tests::attest, CheckedPart, Classed, EvidencePassage};
use crate::dto::{LearningBlockDto, LearningBlockKind, PreparedLearningLesson};
use crate::reference_collection::PassageKind;
use lattice_core::application::contracts::replies::{AnchorDto, ByteSpanDto, RelationDto};
use lattice_core::domain::citation::Anchor;
use lattice_core::domain::learning::claim_review::ClassifiedBy;

const SOURCE: &str = "Quillet reads source text. It then emits tokens.";

fn fixture() -> (LearningLessonDto, LessonVerificationReport) {
    let mut lesson = crate::tests::fixture().modules.remove(0).lessons.remove(0);
    lesson.blocks = vec![LearningBlockDto::new(
        LearningBlockKind::Explanation,
        "Title".into(),
        "Quillet lexes its input.".into(),
        vec![],
        vec![],
    )];
    let prepared = PreparedLearningLesson {
        blocks: lesson.blocks.clone(),
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let mut report = attest(&lesson.id, &prepared);
    report.findings[0].quote = "Quillet lexes its input.".into();
    report.findings[0].evidence = vec![EvidencePassage {
        source_id: "version-1".into(),
        text: SOURCE.into(),
        start_byte: 100,
        end_byte: 100 + SOURCE.len(),
        retrieval_kind: PassageKind::Cited.label().into(),
        score: 1.0,
    }];
    report.findings[0].class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    (lesson, report)
}

fn versions() -> [SavedEvidenceVersion; 1] {
    [SavedEvidenceVersion {
        id: "version-1".into(),
        source_id: "source-1".into(),
        title: "The Quillet Guide".into(),
        resolved_url: None,
        requested_url: Some("https://example.com/quillet".into()),
        content_sha256: "x".into(),
        active_version_id: Some("version-1".into()),
        deleted_at: None,
    }]
}

fn part(kind: PartKind, anchors: &[(usize, usize)]) -> CheckedPart {
    CheckedPart {
        part: "a part".into(),
        kind,
        source_id: None,
        anchors: anchors
            .iter()
            .map(|&(start_byte, end_byte)| Anchor {
                source_version_id: "version-1".into(),
                start_byte,
                end_byte,
            })
            .collect(),
    }
}

fn claim(
    lesson: &LearningLessonDto,
    report: &LessonVerificationReport,
) -> crate::lesson_evidence::LearningClaimEvidenceDto {
    report
        .learner_view(lesson, &versions(), &ClaimStates::default())
        .unwrap()
        .teaching_claims
        .remove(0)
}

#[test]
fn a_restating_statement_says_restates_on_the_wire() {
    let (lesson, mut report) = fixture();
    report.findings[0].two_pass.parts = vec![part(PartKind::Stated, &[(100, 126)])];

    assert_eq!(
        claim(&lesson, &report).relation,
        Some(RelationDto::Restates)
    );
}

#[test]
fn an_inferring_statement_says_infers_on_the_wire() {
    let (lesson, mut report) = fixture();
    report.findings[0].two_pass.parts = vec![
        part(PartKind::Stated, &[(100, 126)]),
        part(PartKind::Follows, &[(127, 148)]),
    ];

    let claim = claim(&lesson, &report);

    assert_eq!(claim.relation, Some(RelationDto::Infers));
    assert_eq!(claim.parts[1].kind, LearningClaimPartKindDto::Follows);
}

#[test]
fn a_statement_that_cites_nothing_has_no_relation() {
    let (lesson, mut report) = fixture();
    report.findings[0].class = Some(Classed {
        class: Class::Common,
        by: ClassifiedBy::Writer,
    });

    assert_eq!(claim(&lesson, &report).relation, None);
}

#[test]
fn a_part_carries_the_words_of_each_anchor_cut_from_its_passage() {
    let (lesson, mut report) = fixture();
    report.findings[0].two_pass.parts = vec![
        part(PartKind::Stated, &[(127, 148), (100, 126)]),
        part(PartKind::Common, &[]),
    ];

    let claim = claim(&lesson, &report);

    assert_eq!(
        claim.parts[0].anchors,
        vec![
            AnchorDto {
                exact: "It then emits tokens.".into(),
                span: Some(ByteSpanDto {
                    start: 127,
                    end: 148
                }),
            },
            AnchorDto {
                exact: "Quillet reads source text.".into(),
                span: Some(ByteSpanDto {
                    start: 100,
                    end: 126
                }),
            },
        ]
    );
    assert!(claim.parts[1].anchors.is_empty());
}

#[test]
fn an_anchor_outside_the_evidence_passages_is_left_out_of_the_part() {
    let (lesson, mut report) = fixture();
    report.findings[0].two_pass.parts = vec![part(PartKind::Stated, &[(0, 10), (100, 126)])];

    let claim = claim(&lesson, &report);

    assert_eq!(claim.parts[0].anchors.len(), 1);
    assert_eq!(
        claim.parts[0].anchors[0].exact,
        "Quillet reads source text."
    );
}

#[test]
fn a_part_names_its_source_version_or_else_the_version_of_its_first_anchor() {
    let (lesson, mut report) = fixture();
    let mut old = part(PartKind::Stated, &[]);
    old.source_id = Some("version-9".into());
    report.findings[0].two_pass.parts = vec![
        old,
        part(PartKind::Stated, &[(100, 126)]),
        part(PartKind::Common, &[]),
    ];

    let claim = claim(&lesson, &report);

    assert_eq!(
        claim.parts[0].source_version_id.as_deref(),
        Some("version-9")
    );
    assert_eq!(
        claim.parts[1].source_version_id.as_deref(),
        Some("version-1")
    );
    assert_eq!(claim.parts[2].source_version_id, None);
}
