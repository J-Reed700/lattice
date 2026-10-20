//! The DTOs' JSON contract, pinned by committed fixtures: every fixture
//! deserializes into its DTO and serializes back unchanged, and the domain
//! mirrors produce exactly the fixture from domain values.

use lattice_core::domain::learning::claim_review::{
    Actor, AttachOutcome, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLocation,
    ClaimState, EventProvenance, EvidenceFingerprint, EvidenceSeen, HumanBinding, Provenance,
    RecordedEvent, ReopenCause, RepairOutcome, RequestFacts, StopReason, SupportedBy,
    TrustAssessment, TrustCause, UnitKind, UserAction, VerdictKind,
};
use lattice_core::domain::provenance::{
    AddedBy, Bar, ClaimKind, ClaimProfile, ClusterId, CountedSupport, Decision, Missing,
    PassageKind, SourceDates, Support, TemporalScope, TemporalValidity, Trust, TrustLevel,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::dto::{
    ClaimActionRequestDto, ClaimEventDto, ClaimStateDto, LessonClaimReviewDto, VerdictKindDto,
};

/// Parses `fixture`, reads it as `T`, writes it back and requires the same
/// JSON: a renamed, missing or extra field fails.
fn round_trip<T: Serialize + DeserializeOwned>(fixture: &str) -> Value {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let dto: T = serde_json::from_value(expected.clone()).unwrap();
    let actual = serde_json::to_value(&dto).unwrap();
    assert_eq!(
        actual,
        expected,
        "{}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
    expected
}

fn assert_mirrors<T: Serialize>(dtos: &[T], fixture: &Value) {
    let actual = serde_json::to_value(dtos).unwrap();
    assert_eq!(
        &actual,
        fixture,
        "{}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

#[test]
fn the_review_fixture_round_trips() {
    round_trip::<LessonClaimReviewDto>(include_str!("fixtures/lesson_claim_review.json"));
}

#[test]
fn every_action_request_round_trips() {
    let requests = round_trip::<Vec<ClaimActionRequestDto>>(include_str!(
        "fixtures/claim_action_requests.json"
    ));
    let actions: Vec<&str> = requests
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|request| request["action"].as_str())
        .collect();
    assert_eq!(
        actions,
        [
            "confirm",
            "unconfirm",
            "edit",
            "remove",
            "general_knowledge",
            "accept_all_general_knowledge",
            "research_with_hint",
            "attach_saved_source",
            "attach_link",
            "attach_document",
            "attach_text",
            "undo",
        ]
    );
}

#[test]
fn claim_states_mirror_the_domain() {
    let fixture = round_trip::<Vec<ClaimStateDto>>(include_str!("fixtures/claim_states.json"));
    let confirmed = ClaimState::Confirmed {
        at: 1_760_000_300_000,
        note: Some("Measured in the lab.".into()),
        bound: HumanBinding::new("Water boils at 100 °C.", vec!["v1".into()]),
    };
    let states = [
        ClaimState::Unchecked,
        ClaimState::Open {
            kind: VerdictKind::Contradicted,
            reason: "The passage says 99.97 °C.".into(),
            machine_attempts_left: 1,
        },
        ClaimState::Queued {
            kind: VerdictKind::NoSource,
            reason: "No passage mentions boiling.".into(),
        },
        ClaimState::ResolvedByEvidence {
            source_version_id: "v1".into(),
            supporting_quote: "boils at 100 °C".into(),
        },
        confirmed.clone(),
        ClaimState::Removed {
            at: 1_760_000_400_000,
            by: Actor::Machine,
        },
        ClaimState::Edited {
            at: 1_760_000_500_000,
            successor: Some(ClaimId("claim-2".into())),
        },
        ClaimState::GeneralKnowledge {
            at: 1_760_000_600_000,
            by: Actor::User,
        },
        ClaimState::Reopened {
            cause: ReopenCause::SourceReplaced {
                from: "v1".into(),
                to: "v2".into(),
            },
            previous: Box::new(confirmed),
        },
        ClaimState::Reopened {
            cause: ReopenCause::TrustChanged(TrustCause::Bar {
                kind: ClaimKind::Specification,
            }),
            previous: Box::new(ClaimState::ResolvedByEvidence {
                source_version_id: "v1".into(),
                supporting_quote: "boils at 100 °C".into(),
            }),
        },
    ];
    let dtos: Vec<ClaimStateDto> = states.iter().map(ClaimStateDto::from).collect();
    assert_mirrors(&dtos, &fixture);
}

#[test]
fn verdict_kinds_mirror_the_domain() {
    let fixture = round_trip::<Vec<VerdictKindDto>>(include_str!("fixtures/verdict_kinds.json"));
    let kinds = [
        VerdictKind::Unsupported,
        VerdictKind::Contradicted,
        VerdictKind::NoSource,
        VerdictKind::CouldNotJudge,
        VerdictKind::GeneralKnowledgeProposed,
        VerdictKind::Disputed {
            verifier: Box::new(VerdictKind::Contradicted),
        },
        VerdictKind::InsufficientStanding {
            bar: Bar::Primary,
            have: counted(),
        },
    ];
    let dtos: Vec<VerdictKindDto> = kinds.iter().map(VerdictKindDto::from).collect();
    assert_mirrors(&dtos, &fixture);
}

/// An established blog counted, and a distrusted page excluded.
fn counted() -> Vec<CountedSupport> {
    vec![
        CountedSupport {
            source_version_id: "v1".into(),
            origin_cluster: ClusterId("v1".into()),
            counted: Some(TrustLevel::Established),
        },
        CountedSupport {
            source_version_id: "v2".into(),
            origin_cluster: ClusterId("v2".into()),
            counted: None,
        },
    ]
}

/// The gate's assessment of a specification claim whose one support, an
/// established blog, is below the bar.
fn below_the_bar() -> TrustAssessment {
    TrustAssessment {
        profile: ClaimProfile {
            kind: ClaimKind::Specification,
            subject: None,
            temporal: TemporalScope::Current,
            measurement_origin: None,
            attributed_to: None,
            reasons: vec![],
            policy: "claim-profile-v1".into(),
            model: "qwen3.5-27b".into(),
        },
        supports: vec![Support {
            source_version_id: "v1".into(),
            origin_cluster: ClusterId("v1".into()),
            trust: Trust {
                level: TrustLevel::Established,
                reasons: vec![],
                override_: None,
            },
            dates: SourceDates::default(),
            passage: PassageKind::Direct,
            temporal: TemporalValidity::Valid,
            added_by: AddedBy::Research,
            cited_urls: vec![],
            owner_hints: vec![],
        }],
        contrary: vec![],
        decision: Decision::Insufficient {
            bar: Bar::Primary,
            have: counted(),
            missing: Missing::NoPrimaryForSubject,
            next: vec![],
            reasons: vec![],
        },
        supported: SupportedBy {
            source_version_id: "v1".into(),
            supporting_quote: "boils at 100 °C".into(),
        },
        policy: "source-trust-v1".into(),
    }
}

fn provenance(actor: Actor, at: i64) -> EventProvenance {
    EventProvenance {
        actor,
        at,
        job_id: (actor == Actor::Machine).then(|| "job-1".into()),
        app_version: "1.0.0".into(),
        checker_model: Some("qwen3.5-27b".into()),
    }
}

fn check(checker: Checker, result: CheckResult) -> Check {
    let evidence = vec![EvidenceSeen {
        source_version_id: "v1".into(),
        start_byte: 120,
        end_byte: 184,
        text_sha256: "4f2a".into(),
    }];
    Check {
        checker,
        result,
        reason: "The passage gives 99.97 °C at one atmosphere.".into(),
        fingerprint: EvidenceFingerprint("fp-1".into()),
        evidence,
        window_tokens: 32_768,
    }
}

#[test]
fn claim_events_mirror_the_domain() {
    let fixture = round_trip::<Vec<ClaimEventDto>>(include_str!("fixtures/claim_events.json"));
    let local = Checker::Local {
        model: "qwen3.5-27b".into(),
    };
    let events = [
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                2,
                UnitKind::AnswerKey,
                "explanation",
                "boils at 100 °C",
                "Water boils at 100 °C at sea level.",
            ),
            statement: "Water boils at 100 °C at sea level.".into(),
        },
        ClaimEvent::Checked(check(
            local.clone(),
            CheckResult::Failed {
                kind: VerdictKind::Contradicted,
            },
        )),
        ClaimEvent::Researched {
            queries: vec!["boiling point of water at sea level".into()],
            sources_added: vec!["v2".into()],
            passages_found: 2,
            round_ms_per_claim: None,
        },
        ClaimEvent::RepairAttempted {
            attempt: 1,
            escalated: false,
            before: "Water boils at 100 °C.".into(),
            after: Some("Water boils at about 100 °C.".into()),
            outcome: RepairOutcome::Rejected {
                reason: "The rewrite dropped its citation.".into(),
            },
        },
        ClaimEvent::Executed {
            observation_id: "obs-1".into(),
            passed: false,
            evidence: "Could not run: no python runtime on this machine.".into(),
        },
        ClaimEvent::UserAction {
            action: UserAction::Confirm {
                bound: HumanBinding::new("Water boils at 100 °C.", vec!["v1".into()]),
            },
            note: Some("Measured in the lab.".into()),
        },
        ClaimEvent::EvidenceAttached {
            source_version_id: "v3".into(),
            provenance: Provenance::Link {
                url: "https://example.org/steam".into(),
            },
            result: AttachOutcome::Checked(check(
                local,
                CheckResult::Supported {
                    source_version_id: "v3".into(),
                    supporting_quote: "boils at 100 °C".into(),
                },
            )),
        },
        ClaimEvent::Superseded {
            successor: Some(ClaimId("claim-2".into())),
        },
        ClaimEvent::Reopened {
            cause: ReopenCause::SourceDeleted("v3".into()),
        },
        ClaimEvent::Undone { event_ordinal: 5 },
        ClaimEvent::Stopped {
            reason: StopReason {
                code: "model_rejected".into(),
                message: "llama.cpp rejected this request twice.".into(),
                fix: "Lower the context window in Settings to match the server.".into(),
                at: 1_760_000_900_000,
                request: Some(RequestFacts {
                    prompt_tokens_exact: Some(118_994),
                    prompt_tokens_estimated: None,
                    window_tokens: 131_072,
                    output_reserved: 8_192,
                    server_kind: "llama_cpp".into(),
                    server_message_sanitized: Some("(500 server_error) […]".into()),
                    attempts: 2,
                    step: "verify · pass 2".into(),
                }),
            },
        },
        ClaimEvent::Checked(check(
            Checker::Verifier {
                model: "claude-verifier".into(),
            },
            CheckResult::Failed {
                kind: VerdictKind::Disputed {
                    verifier: Box::new(VerdictKind::Unsupported),
                },
            },
        )),
        ClaimEvent::StandingAssessed {
            trust: Box::new(below_the_bar()),
            reason: "Its support does not come from the subject's owner.".into(),
            cause: Some(TrustCause::Override {
                source_version_ids: vec!["v1".into()],
            }),
        },
    ];
    let records: Vec<RecordedEvent> = events
        .into_iter()
        .zip(0_u32..)
        .map(|(event, ordinal)| {
            let actor = match event {
                ClaimEvent::UserAction { .. }
                | ClaimEvent::EvidenceAttached { .. }
                | ClaimEvent::Undone { .. } => Actor::User,
                _ => Actor::Machine,
            };
            RecordedEvent {
                ordinal,
                provenance: provenance(actor, 1_760_000_000_000 + i64::from(ordinal)),
                event,
            }
        })
        .collect();
    let dtos: Vec<ClaimEventDto> = records.iter().map(ClaimEventDto::from).collect();
    assert_mirrors(&dtos, &fixture);
}
