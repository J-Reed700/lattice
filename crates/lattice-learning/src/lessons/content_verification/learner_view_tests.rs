use super::*;
use crate::content_verification::{tests::attest, CheckedPart, Classed, EvidencePassage};
use crate::dto::{LearningBlockDto, LearningBlockKind, PreparedLearningLesson};
use crate::reference_collection::PassageKind;
use lattice_core::domain::learning::claim_review::ClassifiedBy;
use lattice_core::domain::provenance::Label;

fn fixture() -> (LearningLessonDto, LessonVerificationReport) {
    let mut lesson = crate::tests::fixture().modules.remove(0).lessons.remove(0);
    lesson.blocks = vec![LearningBlockDto::new(
        LearningBlockKind::Explanation,
        "Title".into(),
        "An exact lesson statement.".into(),
        vec![],
        vec![],
    )];
    let prepared = PreparedLearningLesson {
        blocks: lesson.blocks.clone(),
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let report = attest(&lesson.id, &prepared);
    (lesson, report)
}

#[test]
fn projection_preserves_the_typed_claim_and_rejects_a_changed_anchor() {
    let (mut lesson, report) = fixture();
    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();
    assert_eq!(view.teaching_claims[0].content_quote, lesson.blocks[0].body);
    assert_eq!(view.teaching_claims[0].verdict, "supported");
    lesson.blocks[0].body = "Different lesson content.".into();
    assert!(report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .is_err());
}

#[test]
fn a_statement_reaches_the_learner_with_its_class_its_label_and_the_parts_it_was_read_as() {
    let (lesson, mut report) = fixture();
    let plain = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();
    // A report from before statements were sorted says none of it.
    assert_eq!(plain.teaching_claims[0].class, None);
    assert_eq!(plain.teaching_claims[0].label, None);
    assert!(plain.teaching_claims[0].parts.is_empty());

    report.findings[0].class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    report.findings[0].label = Some(Label::Attributed);
    report.findings[0].two_pass.parts = vec![
        CheckedPart {
            part: "Quillet is a lexer".into(),
            kind: PartKind::Stated,
            source_id: Some("version-1".into()),
            anchors: Vec::new(),
        },
        CheckedPart {
            part: "a lexer turns characters into tokens".into(),
            kind: PartKind::Common,
            source_id: None,
            anchors: Vec::new(),
        },
    ];
    let claim = &report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap()
        .teaching_claims[0];

    assert_eq!(claim.class, Some(LearningStatementClassDto::Sourced));
    assert_eq!(claim.label.as_deref(), Some("attributed"));
    assert_eq!(claim.parts.len(), 2);
    assert_eq!(claim.parts[0].kind, LearningClaimPartKindDto::Stated);
    assert_eq!(
        claim.parts[0].source_version_id.as_deref(),
        Some("version-1")
    );
    assert_eq!(claim.parts[1].kind, LearningClaimPartKindDto::Common);
    assert_eq!(claim.parts[1].source_version_id, None);

    // The other classes.
    for (class, expected) in [
        (Class::Reasoning, LearningStatementClassDto::Reasoning),
        (Class::Common, LearningStatementClassDto::Common),
    ] {
        report.findings[0].class = Some(Classed {
            class,
            by: ClassifiedBy::Model,
        });
        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();
        assert_eq!(view.teaching_claims[0].class, Some(expected));
    }
}

#[test]
fn a_statement_the_model_called_internal_is_not_shown_or_counted_as_a_checked_claim() {
    let (lesson, mut report) = fixture();
    report.findings.push(report.findings[0].clone());
    report.findings[1].class = Some(Classed {
        class: Class::Internal,
        by: ClassifiedBy::Model,
    });

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    assert_eq!(view.teaching_claims.len(), 1);
    assert_eq!(view.teaching_claims[0].class, None);
    assert_eq!(view.claim_count, 1);
}

#[test]
fn the_report_counts_the_claims_it_lists() {
    let (lesson, mut report) = fixture();
    let mut question = report.findings[0].clone();
    question.unit = lesson.blocks.len();
    report.findings.push(question);

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    assert_eq!(view.claim_count, view.teaching_claims.len());
    assert_eq!(view.claim_count, 1);
}

fn not_verified(reason: UnverifiedReason) -> Option<Resolution> {
    Some(Resolution::Unverified { reason })
}

#[test]
fn a_claim_published_not_verified_reads_so_with_its_reason_and_is_never_verified() {
    let (lesson, mut report) = fixture();
    report.findings[0].verdict = ClaimVerdict::Unsupported;
    report.findings[0].resolution = not_verified(UnverifiedReason::Unsupported);

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    let claim = &view.teaching_claims[0];
    assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
    assert_eq!(
        claim.unverified_reason,
        Some(UnverifiedReasonDto::Unsupported)
    );
    assert_eq!(view.claim_count, 1);
    assert_eq!(view.confirmed_count, 0);
    let wire = serde_json::to_value(claim).unwrap();
    assert_eq!(wire["resolution"], "not_verified");
    assert_eq!(wire["unverifiedReason"], "unsupported");
}

#[test]
fn a_report_counts_the_statements_it_publishes_not_verified_and_says_so() {
    use crate::content_verification::not_verified_line;
    let (_, mut report) = fixture();
    assert_eq!(report.unverified_count(), 0);
    report.findings[0].verdict = ClaimVerdict::Unsupported;
    report.findings[0].resolution = not_verified(UnverifiedReason::Unchecked);
    assert_eq!(report.unverified_count(), 1);

    assert_eq!(not_verified_line(0), None);
    assert_eq!(
        not_verified_line(1).as_deref(),
        Some("1 statement not verified")
    );
    assert_eq!(
        not_verified_line(2).as_deref(),
        Some("2 statements not verified")
    );
}

#[test]
fn a_verified_claim_carries_no_reason_on_the_wire() {
    let (lesson, report) = fixture();

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    let wire = serde_json::to_value(&view.teaching_claims[0]).unwrap();
    assert_eq!(wire["resolution"], "verified");
    assert!(wire["unverifiedReason"].is_null(), "{wire}");
}

#[test]
fn a_sentence_that_carries_an_unverified_claim_reads_not_verified_beside_a_verified_one() {
    let (lesson, mut report) = fixture();
    report.findings.push(report.findings[0].clone());
    report.findings[1].verdict = ClaimVerdict::Contradicted;
    report.findings[1].resolution = not_verified(UnverifiedReason::Contradicted);

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    for claim in &view.teaching_claims {
        assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
        assert_eq!(
            claim.unverified_reason,
            Some(UnverifiedReasonDto::Contradicted)
        );
    }
}

#[test]
fn a_claim_the_user_confirmed_stays_confirmed_beside_an_unverified_one() {
    let (lesson, mut report) = fixture();
    report.findings.push(report.findings[0].clone());
    report.findings[0].verdict = ClaimVerdict::Unsupported;
    report.findings[0].resolution = Some(Resolution::Confirmed {
        at: 1,
        note: Some("Checked by hand.".into()),
    });
    report.findings[1].verdict = ClaimVerdict::Unsupported;
    report.findings[1].resolution = not_verified(UnverifiedReason::Unsupported);

    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();

    assert_eq!(
        view.teaching_claims[0].resolution,
        LearningClaimResolutionDto::ConfirmedByYou
    );
    assert_eq!(
        view.teaching_claims[1].resolution,
        LearningClaimResolutionDto::NotVerified
    );
}

#[test]
fn a_claim_no_passage_supports_and_nobody_resolved_reads_not_verified_never_verified_or_unavailable(
) {
    for (verdict, reason) in [
        (ClaimVerdict::Unsupported, UnverifiedReasonDto::Unsupported),
        (
            ClaimVerdict::Contradicted,
            UnverifiedReasonDto::Contradicted,
        ),
        (ClaimVerdict::Unverified, UnverifiedReasonDto::Unchecked),
    ] {
        let (lesson, mut report) = fixture();
        report.findings[0].verdict = verdict;

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap_or_else(|error| panic!("{verdict:?} makes the lesson unavailable: {error}"));

        let claim = &view.teaching_claims[0];
        assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
        assert_eq!(claim.unverified_reason, Some(reason), "{verdict:?}");
        assert_eq!(view.claim_count, 1);
        let wire = serde_json::to_value(claim).unwrap();
        assert_eq!(wire["resolution"], "not_verified");
    }
}

#[test]
fn a_missing_reference_is_not_relabelled_as_executed_evidence() {
    let (lesson, mut report) = fixture();
    report.findings[0].evidence.push(EvidencePassage {
        source_id: "missing-version".into(),
        text: "Saved quote".into(),
        start_byte: 0,
        end_byte: 11,
        retrieval_kind: "lexical".into(),
        score: 1.0,
    });
    assert!(report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .is_err());
    report.findings[0].evidence[0].retrieval_kind = PassageKind::Run.label().into();
    let view = report
        .learner_view(&lesson, &[], &ClaimStates::default())
        .unwrap();
    assert_eq!(
        view.teaching_claims[0].passages[0].title,
        "Executed example"
    );
}

#[test]
fn a_lesson_reads_with_numbered_citations_its_statements_stand_on() {
    use lattice_core::application::contracts::replies::{
        RelationDto, ReplyPieceDto, ReplyReferenceDto, SupportDto,
    };

    let (mut lesson, mut report) = fixture();
    lesson.blocks[0].body = "Quillet reads source text. It then emits tokens.".into();
    let source = |kind: &str, id: &str| EvidencePassage {
        source_id: id.into(),
        text: "Quillet reads source text.".into(),
        start_byte: 0,
        end_byte: 26,
        retrieval_kind: kind.into(),
        score: 1.0,
    };
    let cited = source(PassageKind::Cited.label(), "version-1");
    report.findings[0].quote = "Quillet reads source text.".into();
    report.findings[0].evidence = vec![cited.clone()];
    report.findings[0].class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    report.findings[0].two_pass.parts = vec![CheckedPart {
        part: "Quillet reads source text".into(),
        kind: PartKind::Stated,
        source_id: Some("version-1".into()),
        anchors: Vec::new(),
    }];
    let mut second = report.findings[0].clone();
    second.quote = "It then emits tokens.".into();
    second.class = Some(Classed {
        class: Class::Reasoning,
        by: ClassifiedBy::Model,
    });
    second.two_pass.parts.clear();
    second.evidence = vec![source(PassageKind::Run.label(), "run-1"), cited];
    report.findings.push(second);
    let versions = [SavedEvidenceVersion {
        id: "version-1".into(),
        source_id: "source-1".into(),
        title: "The Quillet Guide".into(),
        resolved_url: None,
        requested_url: Some("https://example.com/quillet".into()),
        content_sha256: "x".into(),
        active_version_id: Some("version-1".into()),
        deleted_at: None,
    }];

    let view = report
        .learner_view(&lesson, &versions, &ClaimStates::default())
        .unwrap();

    assert_eq!(view.citations.len(), 2);
    assert_eq!(view.citations[0].number, 1);
    assert_eq!(view.citations[0].source_id.as_deref(), Some("source-1"));
    assert_eq!(view.citations[0].passage.title, "The Quillet Guide");
    assert_eq!(view.citations[0].passage.source_version_id, "version-1");
    assert!(view.citations[0].passage.trust.is_none());
    assert_eq!(view.citations[1].number, 2);
    assert_eq!(view.citations[1].source_id, None);
    assert_eq!(view.citations[1].passage.title, "Executed example");
    assert_eq!(view.teaching_claims[0].cites, vec![1]);
    assert_eq!(view.teaching_claims[1].cites, vec![1]);
    let mark = |relation| ReplyPieceDto::Reference {
        reference: ReplyReferenceDto {
            written: "[1]".into(),
            kind: "source".into(),
            number: 1,
            lines: None,
            name: None,
            support: Some(SupportDto {
                relation,
                anchors: Vec::new(),
            }),
        },
    };
    let text = |text: &str| ReplyPieceDto::Text { text: text.into() };
    assert_eq!(view.sections.len(), 1);
    assert_eq!(view.sections[0].section_index, 0);
    assert_eq!(
        view.sections[0].pieces,
        vec![
            text("Quillet reads source text."),
            mark(RelationDto::Backs),
            text(" It then emits tokens."),
            mark(RelationDto::Agrees),
        ]
    );
}

#[test]
fn a_checks_statements_get_no_chip_and_are_numbered_after_every_placed_one() {
    use crate::dto::LearningCheckDto;
    use lattice_core::application::contracts::replies::ReplyPieceDto;

    let (mut lesson, mut report) = fixture();
    lesson.blocks[0].body = "Quillet reads source text.".into();
    lesson.blocks[0].check = Some(LearningCheckDto {
        question: "Quillet reads source text.".into(),
        reveal: "It does.".into(),
    });
    lesson.blocks.push(LearningBlockDto::new(
        LearningBlockKind::Explanation,
        "Second".into(),
        "It then emits tokens.".into(),
        vec![],
        vec![],
    ));
    let passage = |text: &str, start: usize| EvidencePassage {
        source_id: "version-1".into(),
        text: text.into(),
        start_byte: start,
        end_byte: start + text.len(),
        retrieval_kind: PassageKind::Cited.label().into(),
        score: 1.0,
    };
    report.findings[0].unit = 0;
    report.findings[0].quote = "Quillet reads source text.".into();
    report.findings[0].evidence = vec![passage("Quillet reads source text.", 0)];
    report.findings[0].class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    report.findings[0].two_pass.parts = vec![CheckedPart {
        part: "Quillet reads source text".into(),
        kind: PartKind::Stated,
        source_id: Some("version-1".into()),
        anchors: Vec::new(),
    }];
    let mut second = report.findings[0].clone();
    second.unit = 1;
    second.quote = "It then emits tokens.".into();
    second.evidence = vec![passage("It then emits tokens.", 100)];
    report.findings.push(second);
    let versions = [SavedEvidenceVersion {
        id: "version-1".into(),
        source_id: "source-1".into(),
        title: "The Quillet Guide".into(),
        resolved_url: None,
        requested_url: Some("https://example.com/quillet".into()),
        content_sha256: "x".into(),
        active_version_id: Some("version-1".into()),
        deleted_at: None,
    }];

    let view = report
        .learner_view(&lesson, &versions, &ClaimStates::default())
        .unwrap();

    assert_eq!(view.citations.len(), 2);
    assert_eq!(view.citations[0].passage.start_byte, 100);
    assert_eq!(view.citations[1].passage.start_byte, 0);
    assert_eq!(view.teaching_claims[0].cites, vec![2]);
    assert_eq!(view.teaching_claims[1].cites, vec![1]);
    let pieces = |index: usize| {
        &view
            .sections
            .iter()
            .find(|section| section.section_index == index)
            .unwrap()
            .pieces
    };
    assert!(!pieces(0)
        .iter()
        .any(|piece| matches!(piece, ReplyPieceDto::Reference { .. })));
    assert!(pieces(1).iter().any(|piece| matches!(
        piece,
        ReplyPieceDto::Reference { reference } if reference.number == 1
    )));
}

#[test]
fn malformed_findings_cannot_become_section_zero_or_an_unknown_verdict() {
    let (_, report) = fixture();
    let mut raw = serde_json::to_value(&report).unwrap();
    raw["findings"][0].as_object_mut().unwrap().remove("unit");
    assert!(serde_json::from_value::<LessonVerificationReport>(raw).is_err());
    let mut raw = serde_json::to_value(&report).unwrap();
    raw["findings"][0]["verdict"] = serde_json::json!("made_up");
    assert!(serde_json::from_value::<LessonVerificationReport>(raw).is_err());
}
