//! The ledger's record of a reading: the check it observed named by the
//! reading, the policy's decision on it and the state that follows, and a
//! change of class.
use super::super::class_checks::finding_of;
use super::super::readings::driver::ReadStep;
use super::super::readings::Read;
use super::bridge_tests::{
    citing, half_stated, parts_reply, sources, Reading as ScriptedBridge, STATEMENT,
};
use super::readings_tests::{read_one, world_of};
use super::resume_upgrade_tests::assessment;
use super::*;
use lattice_core::application::services::claim_verification::{
    CONSISTENCY_POLICY, KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::{
    decide, rewrite_ahead, start, AcceptancePolicy, End, Outcome, Position, Reading, Step, Taken,
    Then, Walked,
};
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, EvidenceFingerprint, EvidenceSeen, UnitKind, VerdictKind,
};
use lattice_core::domain::provenance::ClaimKind;

const SENTENCE: &str = "The reader hands over one row at a time.";
const PASSAGE: &str = "The reader hands over one row at a time, as a dict.";

async fn open() -> Result<claim_ledger::ClaimLedger> {
    claim_ledger::ClaimLedger::open(&Model::new(), "{}").await
}

/// A claim of section 0 the ledger holds, sorted into `class` by `by`.
async fn claim(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    class: Class,
    by: ClassifiedBy,
) -> Result<ClaimId> {
    let id = ClaimId(name.into());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", SENTENCE, SENTENCE);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: SENTENCE.into(),
    };
    ledger.append(&id, extracted).await?;
    ledger
        .append(&id, ClaimEvent::Classified { class, by })
        .await?;
    Ok(id)
}

fn passage(source: &str) -> EvidencePassage {
    EvidencePassage {
        source_id: source.into(),
        text: PASSAGE.into(),
        start_byte: 0,
        end_byte: PASSAGE.len(),
        retrieval_kind: "lexical".into(),
        score: 1.0,
    }
}

fn seen(passage: &EvidencePassage) -> EvidenceSeen {
    EvidenceSeen {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        text_sha256: text_sha256(&passage.text),
    }
}

/// A finding of `verdict` on `evidence`, as a reader of a statement of
/// `class` leaves it.
fn finding(
    (class, by): (Class, ClassifiedBy),
    verdict: ClaimVerdict,
    reason: &str,
    evidence: &[EvidencePassage],
    quote: Option<&str>,
) -> Finding {
    let sentence = Claim {
        quote: SENTENCE.into(),
        statement: SENTENCE.into(),
    };
    finding_of(
        (0, &sentence, Classed { class, by }),
        verdict,
        reason.into(),
        evidence,
        quote.map(str::to_owned),
    )
}

/// Where a statement of `class` begins.
fn entry(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    start(policy, class)
}

/// Where a statement of `class` stands at `node`.
fn at(policy: &dyn AcceptancePolicy, class: Class, node: &str) -> Position {
    Position {
        node: policy.graph(class).id_of(node).unwrap(),
        ..entry(policy, class)
    }
}

/// The step the driver makes of `read` at `at`, and where the statement is
/// then.
fn step_of(
    policy: &dyn AcceptancePolicy,
    at: Position,
    reading: Reading,
    read: Read,
) -> (ReadStep, Option<Position>) {
    let taken = Taken {
        step: Step::Read(reading),
        outcome: read.outcome(),
    };
    let (then, next) = decide(policy, at, taken).unwrap();
    let step = ReadStep {
        reading,
        rewrite_ahead: next.is_some_and(|next| rewrite_ahead(policy, next)),
        class: next
            .map(|next| next.class)
            .filter(|class| *class != at.class),
        then,
        read,
    };
    (step, next)
}

fn node(name: &str, step: Step) -> Then {
    Then::Node {
        name: name.into(),
        step,
    }
}

fn kinds(entry: &ClaimLedgerEntry) -> Vec<&'static str> {
    entry
        .events()
        .iter()
        .map(|record| record.event.kind_name())
        .collect()
}

/// The checks the claim recorded, in order.
fn checks(entry: &ClaimLedgerEntry) -> Vec<&Check> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Checked(check) => Some(check),
            _ => None,
        })
        .collect()
}

fn last(entry: &ClaimLedgerEntry) -> Option<&ClaimEvent> {
    entry.events().last().map(|record| &record.event)
}

fn stepped(
    step: Step,
    outcome: Outcome,
    then: Then,
    reason: &str,
    rewrite_ahead: bool,
) -> ClaimEvent {
    ClaimEvent::Stepped {
        policy: "acceptance-v2".into(),
        step,
        outcome,
        then,
        reason: reason.into(),
        rewrite_ahead,
    }
}

const SOURCED: (Class, ClassifiedBy) = (Class::Sourced, ClassifiedBy::Writer);
const REASONED: (Class, ClassifiedBy) = (Class::Reasoning, ClassifiedBy::Model);
const COMMON: (Class, ClassifiedBy) = (Class::Common, ClassifiedBy::Model);

#[tokio::test]
async fn a_reading_that_stands_resolves_the_claim_on_the_passage_that_states_it() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let evidence = [passage("source-1")];
    let read = Read::new(
        Outcome::Stands,
        finding(
            SOURCED,
            ClaimVerdict::Supported,
            "Stated.",
            &evidence,
            Some(PASSAGE),
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        kinds(held),
        ["extracted", "classified", "checked", "stepped"]
    );
    let recorded = checks(held);
    let [check] = recorded.as_slice() else {
        panic!("one check: {recorded:?}");
    };
    let supported = CheckResult::Supported {
        source_version_id: "source-1".into(),
        supporting_quote: PASSAGE.into(),
    };
    assert_eq!(check.result, supported);
    assert_eq!(
        check.fingerprint,
        EvidenceFingerprint::of(
            evidence_policy(),
            Model::new().model_name(),
            &[seen(&evidence[0])]
        )
    );
    assert_eq!(
        last(held),
        Some(&stepped(
            Step::Read(Reading::Parts),
            Outcome::Stands,
            Then::End(End::Resolved),
            "Stated.",
            false
        ))
    );
    assert_eq!(
        held.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "source-1".into(),
            supporting_quote: PASSAGE.into(),
        }
    );
    Ok(())
}

#[tokio::test]
async fn a_reading_that_sends_the_statement_to_a_remedy_leaves_it_open_with_a_rewrite_ahead(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let read = Read::new(
        Outcome::Unproven,
        finding(
            SOURCED,
            ClaimVerdict::Unsupported,
            "No passage states it.",
            &[passage("source-1")],
            None,
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        checks(held)[0].result,
        CheckResult::Failed {
            kind: VerdictKind::Unsupported
        }
    );
    assert_eq!(held.decided(), Some(&node("s.research", Step::Research)));
    assert_eq!(
        held.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "No passage states it.".into(),
            machine_attempts_left: 1,
        }
    );
    Ok(())
}

#[tokio::test]
async fn a_passed_veto_records_not_false_and_ends_the_claim_consistent() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "reasoned", REASONED.0, REASONED.1).await?;
    let evidence = [passage("source-1")];
    let consistent = Read::new(
        Outcome::Stands,
        finding(
            REASONED,
            ClaimVerdict::Supported,
            "Consistent.",
            &evidence,
            None,
        ),
        false,
    );
    let (step, next) = step_of(
        &policy,
        entry(&policy, Class::Reasoning),
        Reading::Consistency,
        consistent,
    );
    ledger.record_reading(&policy, &id, &step).await?;

    // A reading node is next: the claim waits for the machine's next reading.
    let held = ledger.entry(&id).unwrap();
    assert_eq!(held.state(), &ClaimState::Unchecked);
    assert_eq!(
        held.decided(),
        Some(&node("r.veto", Step::Read(Reading::Veto)))
    );

    let not_called_false = Read::new(
        Outcome::Stands,
        finding(
            REASONED,
            ClaimVerdict::Supported,
            "Asked on its own, the model does not call this false (0 of 3).",
            &[],
            None,
        ),
        false,
    );
    let next = next.unwrap();
    let (step, _) = step_of(&policy, next, Reading::Veto, not_called_false);
    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    let model = Model::new();
    let recorded = checks(held);
    let [first, second] = recorded.as_slice() else {
        panic!("two checks: {recorded:?}");
    };
    assert_eq!(first.result, CheckResult::Consistent);
    assert_eq!(
        first.fingerprint,
        EvidenceFingerprint::of(
            CONSISTENCY_POLICY,
            model.model_name(),
            &[seen(&evidence[0])]
        )
    );
    assert_eq!(second.result, CheckResult::NotFalse);
    assert_eq!(
        second.fingerprint,
        EvidenceFingerprint::of(KNOWLEDGE_POLICY, model.model_name(), &[])
    );
    assert!(matches!(held.state(), ClaimState::Consistent { .. }));
    assert_eq!(held.decided(), Some(&Then::End(End::Consistent)));
    Ok(())
}

#[tokio::test]
async fn a_unanimous_knowledge_reading_records_known_common_and_ends_as_general_knowledge(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "common", COMMON.0, COMMON.1).await?;
    let read = Read::new(
        Outcome::Stands,
        finding(
            COMMON,
            ClaimVerdict::Supported,
            "Standard knowledge: it is how readers work.",
            &[],
            None,
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Common),
        Reading::Knowledge,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    let recorded = checks(held);
    let [check] = recorded.as_slice() else {
        panic!("one check: {recorded:?}");
    };
    assert_eq!(check.result, CheckResult::KnownCommon);
    assert_eq!(
        check.fingerprint,
        EvidenceFingerprint::of(KNOWLEDGE_POLICY, Model::new().model_name(), &[])
    );
    assert!(matches!(held.state(), ClaimState::GeneralKnowledge { .. }));
    Ok(())
}

#[tokio::test]
async fn a_page_that_supports_the_statement_on_another_source_records_supported_elsewhere(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let evidence = [passage("source-2")];
    let mut found = finding(
        SOURCED,
        ClaimVerdict::Supported,
        "Stated elsewhere.",
        &evidence,
        Some(PASSAGE),
    );
    found.two_pass.supported_by = Some(SupportedElsewhere {
        source_id: "source-2".into(),
        start_byte: 0,
        end_byte: PASSAGE.len(),
        cited: vec!["source-1".into()],
    });
    let read = Read::new(Outcome::StandsElsewhere, found, false);
    let (step, _) = step_of(
        &policy,
        at(&policy, Class::Sourced, "s.reread"),
        Reading::Page,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        checks(held)[0].result,
        CheckResult::SupportedElsewhere {
            cited: vec!["source-1".into()],
            source_version_id: "source-2".into(),
            supporting_quote: PASSAGE.into(),
        }
    );
    // A recite is next, so the claim waits for it unchecked.
    assert_eq!(held.decided(), Some(&node("s.recite", Step::Recite)));
    assert_eq!(held.state(), &ClaimState::Unchecked);
    Ok(())
}

#[tokio::test]
async fn the_gates_weighing_is_recorded_between_the_check_and_the_decision() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let weighed = assessment("source-1", PASSAGE);
    let mut found = finding(
        SOURCED,
        ClaimVerdict::Supported,
        "Stated.",
        &[passage("source-1")],
        Some(PASSAGE),
    );
    found.trust = Some(Box::new(provenance_gate::FindingTrust {
        profile: weighed.profile,
        supports: weighed.supports,
        contrary: vec![],
        decision: weighed.decision,
        available: None,
    }));
    let read = Read::new(Outcome::Stands, found, false);
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        kinds(held),
        [
            "extracted",
            "classified",
            "checked",
            "standing_assessed",
            "stepped"
        ]
    );
    assert!(held.trust().is_some());
    Ok(())
}

#[tokio::test]
async fn a_second_unreadable_answer_is_recorded_as_its_check_alone_so_the_claim_stays_at_its_node_with_its_re_ask_spent(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let unreadable = || {
        Read::new(
            Outcome::Unjudged,
            finding(
                SOURCED,
                ClaimVerdict::Unverified,
                "The answer to this statement's check could not be read.",
                &[],
                None,
            ),
            false,
        )
    };
    let (first, next) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        unreadable(),
    );
    ledger.record_reading(&policy, &id, &first).await?;
    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        held.decided(),
        Some(&node("s.read", Step::Read(Reading::Parts)))
    );
    assert_eq!(held.state(), &ClaimState::Unchecked);

    let (second, _) = step_of(&policy, next.unwrap(), Reading::Parts, unreadable());
    assert_eq!(second.then, Then::Stop);
    ledger.record_reading(&policy, &id, &second).await?;

    // A stop belongs to its run: its check is recorded and its decision is
    // not, so the claim is where its first answer left it, with its re-ask
    // spent, and a resumed run reads it once more.
    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        held.decided(),
        Some(&node("s.read", Step::Read(Reading::Parts)))
    );
    assert_eq!(held.state(), &ClaimState::Unchecked);
    assert_eq!(
        kinds(held),
        ["extracted", "classified", "checked", "stepped", "checked"]
    );
    assert_eq!(
        ledger.position(&policy, &id),
        Some(Walked::At(Position {
            reasked: true,
            ..entry(&policy, Class::Sourced)
        }))
    );
    let could_not_judge = CheckResult::Failed {
        kind: VerdictKind::CouldNotJudge,
    };
    let results: Vec<_> = checks(held)
        .into_iter()
        .map(|check| &check.result)
        .collect();
    assert_eq!(results, [&could_not_judge, &could_not_judge]);
    Ok(())
}

#[tokio::test]
async fn a_change_of_class_is_recorded_as_the_evidences_and_the_claim_takes_it() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "common", COMMON.0, COMMON.1).await?;
    let read = Read::new(
        Outcome::NeedsSource,
        finding(
            COMMON,
            ClaimVerdict::Unsupported,
            "Asked on its own, the model does not call this standard knowledge (2 of 3).",
            &[],
            None,
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Common),
        Reading::Knowledge,
        read,
    );
    assert_eq!(step.class, Some(Class::Sourced));

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(
        kinds(held),
        [
            "extracted",
            "classified",
            "checked",
            "stepped",
            "classified"
        ]
    );
    assert_eq!(
        last(held),
        Some(&ClaimEvent::Classified {
            class: Class::Sourced,
            by: ClassifiedBy::Evidence
        })
    );
    assert_eq!(held.class(), Some(Class::Sourced));
    assert_eq!(
        held.classified(),
        Some((Class::Sourced, ClassifiedBy::Evidence))
    );
    // The statement is read again as Sourced, so it waits unchecked.
    assert_eq!(held.state(), &ClaimState::Unchecked);
    Ok(())
}

#[tokio::test]
async fn a_reading_of_a_claim_the_ledger_does_not_hold_is_a_fault() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let read = Read::new(
        Outcome::Stands,
        finding(
            SOURCED,
            ClaimVerdict::Supported,
            "Stated.",
            &[passage("source-1")],
            Some(PASSAGE),
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        read,
    );

    let nobody = ClaimId("nobody".into());
    assert!(ledger
        .record_reading(&policy, &nobody, &step)
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn a_claim_the_user_settled_first_keeps_its_state_and_takes_no_class_change() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "common", COMMON.0, COMMON.1).await?;
    let gone = ClaimEvent::Superseded {
        successor: None,
        by: Actor::Machine,
    };
    ledger.append(&id, gone).await?;
    let removed = ledger.entry(&id).unwrap().state().clone();
    assert!(matches!(removed, ClaimState::Removed { .. }));
    let read = Read::new(
        Outcome::NeedsSource,
        finding(
            COMMON,
            ClaimVerdict::Unsupported,
            "Asked on its own, the model does not call this standard knowledge (2 of 3).",
            &[],
            None,
        ),
        false,
    );
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Common),
        Reading::Knowledge,
        read,
    );
    assert_eq!(step.class, Some(Class::Sourced));

    ledger.record_reading(&policy, &id, &step).await?;

    let held = ledger.entry(&id).unwrap();
    assert_eq!(held.state(), &removed);
    assert_eq!(held.decided(), None);
    assert_eq!(held.classified(), Some(COMMON));
    assert_eq!(
        kinds(held),
        ["extracted", "classified", "superseded", "checked"],
        "what it observed is kept, and nothing that follows from a decision"
    );
    Ok(())
}

const PART_KINDS: [&str; 5] = [
    "stated",
    "common",
    "own",
    "specific_missing",
    "contradicted",
];

/// The part-wise judge's reply with one part of each kind in `kinds`.
fn reply_of(kinds: &[&str]) -> String {
    let parts: Vec<Value> = kinds
        .iter()
        .map(|kind| {
            let cites = matches!(*kind, "stated" | "contradicted");
            let passage = if cites {
                json!("passage-0")
            } else {
                Value::Null
            };
            json!({"part": format!("a {kind} part"), "kind": kind, "passage": passage})
        })
        .collect();
    let word = if kinds.contains(&"contradicted") {
        "contradicted"
    } else if kinds.contains(&"specific_missing") || !kinds.contains(&"stated") {
        "unsupported"
    } else if kinds.contains(&"common") {
        "supported_with_common"
    } else {
        "supported"
    };
    parts_reply(Value::Array(parts), word)
}

#[tokio::test]
async fn every_bridge_answer_any_reader_gives_records_a_check_that_agrees_with_its_outcome(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let readers = [
        (Reading::Parts, Class::Sourced, "s.read"),
        (Reading::Ground, Class::Reasoning, "r.ground"),
        (Reading::Ground, Class::Common, "c.ground"),
    ];
    for mask in 1_usize..(1 << PART_KINDS.len()) {
        let kinds: Vec<&str> = PART_KINDS
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, kind)| *kind)
            .collect();
        for (reading, class, node) in readers {
            let model = ScriptedBridge::new(vec![reply_of(&kinds)]);
            let read = read_one(reading, &model, &world, STATEMENT).await?;
            let mut ledger = open().await?;
            let id = claim(&mut ledger, "read", class, ClassifiedBy::Model).await?;
            let (step, _) = step_of(&policy, at(&policy, class, node), reading, read);
            let stands = matches!(
                step.read.outcome(),
                Outcome::Stands | Outcome::StandsElsewhere
            );

            let recorded = ledger.record_reading(&policy, &id, &step).await;

            assert!(recorded.is_ok(), "{reading:?} of {kinds:?}: {recorded:?}");
            let held = ledger.entry(&id).unwrap();
            let recorded = checks(held);
            let [check] = recorded.as_slice() else {
                panic!("{reading:?} of {kinds:?}: one check");
            };
            let passed = !matches!(check.result, CheckResult::Failed { .. });
            assert_eq!(
                passed, stands,
                "{reading:?} of {kinds:?}: {:?}",
                check.result
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_ground_reading_of_a_stated_and_a_common_part_records_an_unsupported_check() -> Result<()>
{
    let policy = StandardPolicy::new();
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, citing(&references)?);
    let model = ScriptedBridge::new(vec![half_stated()]);
    let read = read_one(Reading::Ground, &model, &world, STATEMENT).await?;
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "grounded",
        Class::Reasoning,
        ClassifiedBy::Model,
    )
    .await?;
    let (step, _) = step_of(
        &policy,
        at(&policy, Class::Reasoning, "r.ground"),
        Reading::Ground,
        read,
    );

    let recorded = ledger.record_reading(&policy, &id, &step).await;

    assert!(recorded.is_ok(), "{recorded:?}");
    let held = ledger.entry(&id).unwrap();
    let failed = CheckResult::Failed {
        kind: VerdictKind::Unsupported,
    };
    assert_eq!(checks(held)[0].result, failed);
    assert!(held.events().iter().any(|record| matches!(
        record.event,
        ClaimEvent::Stepped {
            outcome: Outcome::Refuted,
            ..
        }
    )));
    Ok(())
}

#[tokio::test]
async fn the_check_of_a_supported_finding_names_the_strongest_holder_of_its_quotation() -> Result<()>
{
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "cited", SOURCED.0, SOURCED.1).await?;
    let mut held_by_both = finding(
        SOURCED,
        ClaimVerdict::Supported,
        "Stated.",
        &[passage("web"), passage("book")],
        Some(PASSAGE),
    );
    held_by_both.trust = Some(Box::new(FindingTrust {
        profile: ClaimProfile {
            kind: ClaimKind::Specification,
            subject: None,
            temporal: TemporalScope::Timeless,
            measurement_origin: None,
            attributed_to: None,
            reasons: Vec::new(),
            policy: "test".into(),
            model: "test".into(),
        },
        supports: Vec::new(),
        contrary: Vec::new(),
        decision: Decision::Sufficient {
            met_by: vec!["book".to_owned()],
            label: None,
            reasons: Vec::new(),
        },
        available: None,
    }));
    let read = Read::new(Outcome::Stands, held_by_both, false);
    let (step, _) = step_of(
        &policy,
        entry(&policy, Class::Sourced),
        Reading::Parts,
        read,
    );

    ledger.record_reading(&policy, &id, &step).await?;

    let supported = CheckResult::Supported {
        source_version_id: "book".into(),
        supporting_quote: PASSAGE.into(),
    };
    assert_eq!(checks(ledger.entry(&id).unwrap())[0].result, supported);
    Ok(())
}
