//! Source trust in the ledger (source provenance §13, §18.7): the gate's
//! assessment rides with the check it decided, a trust change re-decides
//! a claim purely and replays, a human verdict is never re-decided, and the
//! outcome table calibration learns from.

use super::*;
use crate::domain::provenance::{
    decide, effective_bars, AddedBy, Available, Calibration, ClaimKind, ClaimProfile, ClaimSubject,
    ClusterId, Decision, EffectiveBars, LabelSource, Outcome, PassageKind, SourceDates, StaticBars,
    SubjectKind, Support, TemporalScope, TemporalValidity, Trust, TrustLevel, POLICY,
};
use serde_json::json;

const SUPPORTING_QUOTE: &str = "boils at 100 °C";

fn profile() -> ClaimProfile {
    ClaimProfile {
        kind: ClaimKind::Specification,
        subject: Some(ClaimSubject::new(
            "IUPAC standard conditions",
            SubjectKind::Work,
            vec![],
            vec![],
        )),
        temporal: TemporalScope::Current,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "claim-profile-v1".into(),
        model: MODEL.into(),
    }
}

fn support(version: &str, level: TrustLevel) -> Support {
    Support {
        source_version_id: version.into(),
        origin_cluster: ClusterId(version.into()),
        trust: Trust {
            level,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: vec![],
        owner_hints: vec![],
    }
}

fn static_bars() -> EffectiveBars {
    EffectiveBars::unmoved(&StaticBars::table(), &POLICY)
}

/// Bars under which established sources are accepted alone for
/// specification claims: thirty confirmations of claims the static bar
/// queued, far more than the twenty-one an empty cell needs.
fn relaxed_bars() -> EffectiveBars {
    let outcomes: Vec<Outcome> = (0..30)
        .map(|at| Outcome {
            level: TrustLevel::Established,
            kind: ClaimKind::Specification,
            decision_was_sufficient: false,
            right: false,
            at,
        })
        .collect();
    let bars = effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(&outcomes),
        &POLICY,
    );
    assert_eq!(
        bars.lowest_accepted(ClaimKind::Specification),
        Some(TrustLevel::Established)
    );
    bars
}

fn assessment(supports: Vec<Support>, bars: &EffectiveBars) -> TrustAssessment {
    let profile = profile();
    TrustAssessment {
        decision: decide(&profile, &supports, &[], bars, &Available::default()),
        profile,
        supports,
        contrary: vec![],
        supported: SupportedBy {
            source_version_id: "v1".into(),
            supporting_quote: SUPPORTING_QUOTE.into(),
        },
        policy: "source-trust-v1".into(),
        available: None,
    }
}

/// An established blog alone, under the static bars: below the bar.
fn insufficient() -> TrustAssessment {
    assessment(vec![support("v1", TrustLevel::Established)], &static_bars())
}

fn assessed(trust: TrustAssessment, cause: Option<TrustCause>) -> ClaimEvent {
    ClaimEvent::StandingAssessed {
        reason: "Its support comes from 1 independent origin.".into(),
        trust: Box::new(trust),
        cause,
    }
}

fn failure(trust: &TrustAssessment) -> VerdictKind {
    match trust.verdict() {
        GateVerdict::Fails(kind) => kind,
        GateVerdict::Sufficient => panic!("expected a failing decision"),
    }
}

fn bar_moved() -> TrustCause {
    TrustCause::Bar {
        kind: ClaimKind::Specification,
    }
}

/// A claim whose support the gate held below its bar, as the loop records
/// it: the failing check, then the assessment.
fn gated_claim(id: &str) -> ClaimLedgerEntry {
    let trust = insufficient();
    let mut entry = claim(id);
    entry
        .append(machine(2), checked(Some(failure(&trust)), "v1"))
        .unwrap();
    entry.append(machine(3), assessed(trust, None)).unwrap();
    entry
}

/// The same claim, handed to the user.
fn gated_queued_claim(id: &str) -> ClaimLedgerEntry {
    let mut entry = gated_claim(id);
    entry
        .append(
            user(4),
            act(
                UserAction::ResearchWithHint {
                    hint: "IUPAC".into(),
                },
                None,
            ),
        )
        .unwrap();
    assert!(matches!(entry.state(), ClaimState::Queued { .. }));
    entry
}

/// A claim the checker supported and the gate let stand under `bars`.
fn gated_verified_claim(
    id: &str,
    supports: Vec<Support>,
    bars: &EffectiveBars,
) -> ClaimLedgerEntry {
    let trust = assessment(supports, bars);
    assert_eq!(trust.verdict(), GateVerdict::Sufficient);
    let mut entry = claim(id);
    entry.append(machine(2), checked(None, "v1")).unwrap();
    entry.append(machine(3), assessed(trust, None)).unwrap();
    entry
}

fn replayed(entry: &ClaimLedgerEntry) -> ClaimLedgerEntry {
    ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap()
}

#[test]
fn a_support_below_its_bar_is_the_gates_failure_researched_but_never_repaired() {
    let entry = gated_claim("c1");
    let ClaimState::Open {
        kind: VerdictKind::InsufficientStanding { bar, have },
        machine_attempts_left,
        ..
    } = entry.state()
    else {
        panic!("{:?}", entry.state())
    };
    assert_eq!(*machine_attempts_left, MACHINE_ATTEMPTS);
    assert_eq!(have.len(), 1);
    assert_eq!(have[0].counted, Some(TrustLevel::Established));
    assert_eq!(
        serde_json::to_value(bar).unwrap(),
        json!("primary"),
        "a specification is held to the owner's documentation"
    );
    assert!(entry.trust().is_some());
    assert!(entry.redecidable());
    // Design 2026-10-21 §10.4: a lack of standing is researched, never
    // rewritten or removed by the machine.
    assert!(entry.may_research());
    assert!(!entry.may_repair());
    assert!(!entry.allows(ClaimActionKind::GeneralKnowledge));
}

#[test]
fn a_relaxed_bar_verifies_a_queued_claim_without_a_model_and_replays() {
    let mut entry = gated_queued_claim("c1");
    let trust = entry.trust().unwrap().clone();
    let next = trust.redecide(trust.supports.clone(), &relaxed_bars());
    assert_eq!(next.verdict(), GateVerdict::Sufficient);
    entry
        .append(machine(5), assessed(next.clone(), Some(bar_moved())))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "v1".into(),
            supporting_quote: SUPPORTING_QUOTE.into(),
        }
    );
    assert_eq!(entry.latest_failure(), None);
    assert_eq!(entry.trust(), Some(&next));
    let replay = replayed(&entry);
    assert_eq!(replay.state(), entry.state());
    assert_eq!(replay.trust(), entry.trust());
}

#[test]
fn a_still_failing_redecision_keeps_the_owner_and_the_attempts() {
    let mut entry = gated_claim("c1");
    let trust = entry.trust().unwrap().clone();
    let mut community = trust.supports.clone();
    community[0].trust.level = TrustLevel::Community;
    let next = trust.redecide(community, &static_bars());
    entry
        .append(
            machine(4),
            assessed(
                next.clone(),
                Some(TrustCause::Override {
                    source_version_ids: vec!["v1".into()],
                }),
            ),
        )
        .unwrap();
    let ClaimState::Open {
        kind,
        machine_attempts_left,
        ..
    } = entry.state()
    else {
        panic!("{:?}", entry.state())
    };
    assert_eq!(*kind, failure(&next));
    assert_eq!(*machine_attempts_left, MACHINE_ATTEMPTS);
}

#[test]
fn a_tightened_bar_reopens_a_verified_claim_and_a_relaxed_one_restores_it() {
    let relaxed = relaxed_bars();
    let mut entry =
        gated_verified_claim("c1", vec![support("v1", TrustLevel::Established)], &relaxed);
    let verified = entry.state().clone();
    let trust = entry.trust().unwrap().clone();

    let tightened = trust.redecide(trust.supports.clone(), &static_bars());
    entry
        .append(machine(4), assessed(tightened.clone(), Some(bar_moved())))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Reopened {
            cause: ReopenCause::TrustChanged(bar_moved()),
            previous: Box::new(verified.clone()),
        }
    );
    assert!(entry.state().awaits_user());
    assert_eq!(entry.latest_failure(), Some(&failure(&tightened)));
    assert!(entry.redecidable(), "a trust reopen is the gate's to undo");

    let relaxed_again = tightened.redecide(tightened.supports.clone(), &relaxed);
    entry
        .append(machine(5), assessed(relaxed_again, Some(bar_moved())))
        .unwrap();
    assert_eq!(entry.state(), &verified);
    assert_eq!(replayed(&entry).state(), &verified);
}

#[test]
fn a_trust_change_never_moves_a_human_verdict() {
    let mut entry = gated_queued_claim("c1");
    entry.append(user(5), act(confirm(), None)).unwrap();
    assert!(!entry.redecidable());
    let trust = entry.trust().unwrap().clone();
    let next = trust.redecide(trust.supports.clone(), &relaxed_bars());
    let refused = entry.append(machine(6), assessed(next, Some(bar_moved())));
    assert!(matches!(refused, Err(LedgerError::NotAllowed { .. })));
    assert!(matches!(entry.state(), ClaimState::Confirmed { .. }));

    let bound = HumanBinding::new(QUOTE, vec!["v1".into()]);
    assert_eq!(relitigate(&bound, &NewInformation::DecisionChanged), None);
    assert_eq!(
        reopen_cause(entry.state(), &NewInformation::DecisionChanged),
        None
    );
}

#[test]
fn a_later_check_the_gate_did_not_decide_clears_the_assessment() {
    let mut entry = gated_claim("c1");
    entry
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert_eq!(entry.trust(), None);
    assert!(!entry.redecidable());
    let refused = entry.append(machine(5), assessed(insufficient(), Some(bar_moved())));
    assert!(refused.is_err());

    // The Verifier's audit is not a local check: the assessment stays.
    let mut verified = gated_verified_claim(
        "c2",
        vec![support("v1", TrustLevel::Primary)],
        &static_bars(),
    );
    verified
        .append(machine(4), verifier(Some(VerdictKind::Unsupported)))
        .unwrap();
    assert!(verified.trust().is_some());
}

#[test]
fn undo_returns_the_assessment_the_claim_had() {
    let mut entry = gated_queued_claim("c1");
    let before = entry.trust().cloned();
    entry.append(user(5), act(confirm(), None)).unwrap();
    let ordinal = entry.undo_target().unwrap();
    entry
        .append(
            user(6),
            ClaimEvent::Undone {
                event_ordinal: ordinal,
            },
        )
        .unwrap();
    assert_eq!(entry.trust().cloned(), before);
    assert!(entry.redecidable());
}

#[test]
fn a_cause_touches_only_the_claims_it_can_change() {
    let trust = insufficient();
    assert!(bar_moved().affects(&trust));
    assert!(!TrustCause::Bar {
        kind: ClaimKind::Historical
    }
    .affects(&trust));
    assert!(TrustCause::Override {
        source_version_ids: vec!["v1".into()]
    }
    .affects(&trust));
    assert!(!TrustCause::Override {
        source_version_ids: vec!["v9".into()]
    }
    .affects(&trust));
}

#[test]
fn an_override_that_raises_the_support_verifies_on_redecision() {
    let trust = insufficient();
    let mut raised = trust.supports.clone();
    raised[0].trust.level = TrustLevel::Primary;
    assert_eq!(
        trust.redecide(raised, &static_bars()).verdict(),
        GateVerdict::Sufficient
    );
}

#[test]
fn new_information_counts_a_changed_decision() {
    let entry = gated_claim("c1");
    let basis = entry.basis().unwrap();
    let mut now = Observation {
        quote: entry.location().quote.clone(),
        fingerprint: basis.fingerprint.clone(),
        checker_model: basis.checker_model.clone(),
        window_tokens: basis.window_tokens,
        source_changes: Vec::new(),
        hint: None,
        attached_source_version_id: None,
        gate: Some(entry.trust().unwrap().verdict()),
    };
    assert_eq!(new_information(&entry, &now), None);
    now.gate = Some(GateVerdict::Sufficient);
    assert_eq!(
        new_information(&entry, &now),
        Some(NewInformation::DecisionChanged)
    );
    now.gate = None;
    assert_eq!(new_information(&entry, &now), None, "not weighed this pass");
}

#[test]
fn the_assessment_is_stored_under_its_kind() {
    let event = assessed(insufficient(), Some(bar_moved()));
    let stored = serde_json::to_value(&event).unwrap();
    assert_eq!(stored["kind"], json!("standing_assessed"));
    assert_eq!(stored["cause"], json!({"bar": {"kind": "specification"}}));
    assert_eq!(serde_json::from_value::<ClaimEvent>(stored).unwrap(), event);
    let reopened = ReopenCause::TrustChanged(bar_moved());
    let stored = serde_json::to_value(&reopened).unwrap();
    assert_eq!(
        serde_json::from_value::<ReopenCause>(stored).unwrap(),
        reopened
    );
    assert_eq!(failure(&insufficient()).as_str(), "insufficient_standing");
}

/// §13's outcome table, row by row: (claim before the verdict, verdict) →
/// (decision was sufficient, right, label).
#[test]
fn verdicts_label_outcomes_as_the_table_says() {
    let primary = || vec![support("v1", TrustLevel::Primary)];
    let disputed = || {
        let mut entry = gated_verified_claim("c1", primary(), &static_bars());
        entry
            .append(machine(4), verifier(Some(VerdictKind::Unsupported)))
            .unwrap();
        entry
    };
    let rows: Vec<(ClaimLedgerEntry, TrustVerdict, bool, bool, LabelSource)> = vec![
        (
            gated_queued_claim("c1"),
            TrustVerdict::Confirm,
            false,
            false,
            LabelSource::UserConfirm,
        ),
        (
            gated_queued_claim("c1"),
            TrustVerdict::Remove,
            false,
            true,
            LabelSource::UserRemove,
        ),
        (
            gated_queued_claim("c1"),
            TrustVerdict::Edit,
            false,
            true,
            LabelSource::UserEdit,
        ),
        (
            gated_queued_claim("c1"),
            TrustVerdict::EvidenceAdded,
            false,
            false,
            LabelSource::EvidenceAdded,
        ),
        (
            gated_verified_claim("c1", primary(), &static_bars()),
            TrustVerdict::Remove,
            true,
            false,
            LabelSource::UserRemove,
        ),
        (
            disputed(),
            TrustVerdict::Remove,
            true,
            false,
            LabelSource::VerifierDisagree,
        ),
        (
            disputed(),
            TrustVerdict::Edit,
            true,
            false,
            LabelSource::VerifierDisagree,
        ),
        (
            disputed(),
            TrustVerdict::Confirm,
            true,
            true,
            LabelSource::UserConfirm,
        ),
        (
            gated_verified_claim("c1", primary(), &static_bars()),
            TrustVerdict::VerifierAgreed,
            true,
            true,
            LabelSource::VerifierAgree,
        ),
    ];
    for (entry, verdict, sufficient, right, label) in rows {
        let outcomes = trust_outcomes(&entry, verdict, 1_760_000_000);
        assert_eq!(outcomes.len(), 1, "{verdict:?}");
        let outcome = &outcomes[0];
        assert_eq!(outcome.source_version_id, "v1");
        assert_eq!(outcome.label_source, label, "{verdict:?}");
        assert_eq!(
            (
                outcome.outcome.decision_was_sufficient,
                outcome.outcome.right
            ),
            (sufficient, right),
            "{verdict:?} on {:?}",
            entry.state()
        );
        assert_eq!(outcome.outcome.kind, ClaimKind::Specification);
        assert_eq!(outcome.outcome.at, 1_760_000_000);
    }
}

#[test]
fn only_the_gates_claims_and_supports_the_user_did_not_add_are_calibrated() {
    // The checker's own flag, with no assessment: nothing to calibrate.
    let flagged = queued_claim("c1", VerdictKind::Unsupported);
    assert!(trust_outcomes(&flagged, TrustVerdict::Confirm, 1).is_empty());

    // The user's evidence met the bar: its level is shown, never learned.
    let mut mine = support("v1", TrustLevel::Established);
    mine.added_by = AddedBy::User;
    let entry = gated_verified_claim("c2", vec![mine], &static_bars());
    assert!(trust_outcomes(&entry, TrustVerdict::Remove, 1).is_empty());

    // The Verifier judges entailment, not standing: agreeing with a claim
    // the gate held back says nothing about the bar.
    let held_back = gated_queued_claim("c3");
    assert!(trust_outcomes(&held_back, TrustVerdict::VerifierAgreed, 1).is_empty());
}

#[test]
fn each_supporting_cluster_is_one_outcome_at_its_counted_level() {
    let mut mirror = support("v2", TrustLevel::Community);
    mirror.origin_cluster = ClusterId("v1".into());
    let supports = vec![
        support("v1", TrustLevel::Unknown),
        mirror,
        support("v3", TrustLevel::Community),
    ];
    let trust = assessment(supports, &static_bars());
    assert!(matches!(trust.decision, Decision::Insufficient { .. }));
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(failure(&trust)), "v1"))
        .unwrap();
    entry.append(machine(3), assessed(trust, None)).unwrap();
    let mut outcomes: Vec<(String, TrustLevel)> = trust_outcomes(&entry, TrustVerdict::Confirm, 1)
        .into_iter()
        .map(|outcome| (outcome.source_version_id, outcome.outcome.level))
        .collect();
    outcomes.sort();
    assert_eq!(
        outcomes,
        vec![
            ("v2".to_string(), TrustLevel::Community),
            ("v3".to_string(), TrustLevel::Community),
        ],
        "the v1/v2 cluster counts once, at its best support"
    );
}
