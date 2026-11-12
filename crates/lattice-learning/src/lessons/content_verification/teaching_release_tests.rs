//! The teaching review after a claim repair or a user edit repairs the
//! sentence of a machine's claim that has attempts left, and holds the rest for
//! the user (claim sources design §10.3).
use super::acceptance_switch_tests::{claim, identified, lesson_of, node_of, open, walk, REASONED};
use super::class_checks_tests::Classes;
use super::round::Unsettled;
use super::*;
use crate::lessons::call_plan::CallPlan;
use lattice_core::domain::learning::acceptance::{Goal, Outcome, Reading, Step};
use lattice_core::domain::learning::claim_review::{
    Actor, AttachOutcome, Check, CheckResult, Checker, ClaimEvent, ClaimState, EvidenceFingerprint,
    HumanBinding, Provenance, UserAction,
};

const FIRST: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const SECOND: &str =
    "The reader hands over one row at a time, so it never holds the whole file in memory.";
const THIRD: &str = "A row is handed over and dropped before the next one is read from the file.";
const FOURTH: &str = "Each row is a list of the values the file holds on one line.";
const COMMON: &str = "A page number identifies the page of a document that a value appears on.";

/// A Reasoning statement the consistency reading passed and the veto let stand.
async fn verified(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    sentence: &str,
) -> Result<ClaimId> {
    let id = claim(ledger, name, (sentence, sentence), sentence, REASONED).await?;
    walk(
        ledger,
        &id,
        Class::Reasoning,
        &[
            (Step::Read(Reading::Consistency), Outcome::Stands),
            (Step::Read(Reading::Veto), Outcome::Stands),
        ],
    )
    .await?;
    Ok(id)
}

/// The teaching review rewrote `from` into `to`; the pass after it, and the
/// claim that continues the sentence.
async fn taught(
    ledger: &mut claim_ledger::ClaimLedger,
    id: &ClaimId,
    (from, to): (&str, &str),
) -> Result<ClaimId> {
    ledger
        .begin_teaching(
            &StandardPolicy::new(),
            &lesson_of(from),
            std::slice::from_ref(id),
        )
        .await?;
    let pass = identified(ledger, &Classes::new(&[]), &lesson_of(to), &[(to, to)], &[]).await?;
    let [kept] = pass.statements.as_slice() else {
        panic!("one statement: {:?}", pass.statements.len());
    };
    Ok(kept.id.clone())
}

#[tokio::test]
async fn a_machine_claim_with_attempts_left_is_released_and_its_successor_is_read_again_after_a_rewrite(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = verified(&mut ledger, "reader", FIRST).await?;
    assert_eq!(ledger.releasable(&policy, &[]), std::slice::from_ref(&id));

    let successor = taught(&mut ledger, &id, (FIRST, SECOND)).await?;

    assert_ne!(successor, id);
    assert_eq!(node_of(&ledger, &successor), Some("r.final"));
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::Edited { successor: Some(next), by: Actor::Machine, .. } if *next == successor
    ));
    Ok(())
}

#[tokio::test]
async fn a_sentence_the_teaching_review_left_as_it_was_costs_its_claim_nothing() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = verified(&mut ledger, "reader", FIRST).await?;
    assert_eq!(ledger.releasable(&policy, &[]), std::slice::from_ref(&id));

    let kept = taught(&mut ledger, &id, (FIRST, FIRST)).await?;

    assert_eq!(kept, id);
    assert_eq!(ledger.releasable(&policy, &[]), [id]);
    Ok(())
}

#[tokio::test]
async fn a_sentence_the_teaching_review_keeps_rewriting_is_held_once_its_claims_rewrites_are_spent(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let mut current = verified(&mut ledger, "reader", FIRST).await?;
    for pair in [(FIRST, SECOND), (SECOND, THIRD)] {
        assert_eq!(ledger.releasable(&policy, &[]), [current.clone()]);
        current = taught(&mut ledger, &current, pair).await?;
    }
    assert!(ledger.releasable(&policy, &[]).is_empty());
    Ok(())
}

/// A Reasoning statement the consistency reading refuted, whose rewrite was
/// not applied at first and then was, and which then passed both readings:
/// its own repair took both rewrite steps its graph gives.
async fn repaired_with_its_retry(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    sentence: &str,
) -> Result<ClaimId> {
    let id = claim(ledger, name, (sentence, sentence), sentence, REASONED).await?;
    let rewrite = Step::Rewrite(Goal::AgreeWithThePassages);
    walk(
        ledger,
        &id,
        Class::Reasoning,
        &[
            (Step::Read(Reading::Consistency), Outcome::Refuted),
            (rewrite, Outcome::NotApplied),
            (rewrite, Outcome::Rewritten),
            (Step::Read(Reading::Consistency), Outcome::Stands),
            (Step::Read(Reading::Veto), Outcome::Stands),
        ],
    )
    .await?;
    Ok(id)
}

#[tokio::test]
async fn a_statement_whose_own_repair_took_both_rewrite_steps_is_not_released() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = repaired_with_its_retry(&mut ledger, "reader", FIRST).await?;
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::Consistent { .. }
    ));

    assert!(ledger.releasable(&policy, &[]).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_statement_with_one_applied_rewrite_and_nothing_else_spent_is_released() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "reader", (FIRST, FIRST), FIRST, REASONED).await?;
    walk(
        &mut ledger,
        &id,
        Class::Reasoning,
        &[
            (Step::Read(Reading::Consistency), Outcome::Refuted),
            (
                Step::Rewrite(Goal::AgreeWithThePassages),
                Outcome::Rewritten,
            ),
            (Step::Read(Reading::Consistency), Outcome::Stands),
            (Step::Read(Reading::Veto), Outcome::Stands),
        ],
    )
    .await?;

    assert_eq!(ledger.releasable(&policy, &[]), [id]);
    Ok(())
}

#[tokio::test]
async fn the_plan_holds_the_reading_of_a_lone_statements_teaching_rewrite() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "reader", (FIRST, FIRST), FIRST, REASONED).await?;
    let lone = std::slice::from_ref(&id);
    let mut plan = CallPlan::new(
        &policy,
        &ledger.planned(&policy, &lesson_of(FIRST), lone),
        0,
    );
    walk(
        &mut ledger,
        &id,
        Class::Reasoning,
        &[
            (Step::Read(Reading::Consistency), Outcome::Stands),
            (Step::Read(Reading::Veto), Outcome::Stands),
        ],
    )
    .await?;
    assert_eq!(
        plan.moved(&policy, &ledger.planned(&policy, &lesson_of(FIRST), lone)),
        0
    );

    // The teaching review rewrites its sentence: the successor is read again,
    // and the ceiling held that cost.
    let successor = taught(&mut ledger, &id, (FIRST, SECOND)).await?;
    assert_ne!(successor, id);
    let moved = ledger.planned(&policy, &lesson_of(SECOND), lone);
    assert_eq!(plan.moved(&policy, &moved), 0, "the ceiling rises");
    Ok(())
}

#[tokio::test]
async fn a_machines_general_knowledge_and_published_not_verified_claims_are_released() -> Result<()>
{
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let common = common_claim(&mut ledger).await?;
    let unverified = claim(&mut ledger, "unsettled", (FIRST, FIRST), FIRST, REASONED).await?;
    assert_eq!(
        ledger
            .publish_unsettled(Unsettled::Waiting(vec![unverified.clone()]), &json!({}))
            .await?,
        1
    );
    assert!(matches!(
        ledger.entry(&common).unwrap().state(),
        ClaimState::GeneralKnowledge { .. }
    ));
    assert!(matches!(
        ledger.entry(&unverified).unwrap().state(),
        ClaimState::Unverified { .. }
    ));

    assert_eq!(ledger.releasable(&policy, &[]), [common, unverified]);
    Ok(())
}

#[tokio::test]
async fn the_claims_the_user_decided_and_the_claims_a_repair_is_for_are_not_released() -> Result<()>
{
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let machine = verified(&mut ledger, "reader", FIRST).await?;

    // Confirmed by the user.
    let confirmed = claim(&mut ledger, "confirmed", (SECOND, SECOND), SECOND, REASONED).await?;
    ledger
        .publish_unsettled(Unsettled::Waiting(vec![confirmed.clone()]), &json!({}))
        .await?;
    let confirm = UserAction::Confirm {
        bound: HumanBinding::new(SECOND, vec![]),
    };
    let action = ClaimEvent::UserAction {
        action: confirm,
        note: Some("Checked by hand.".into()),
    };
    assert!(ledger.record_by_user(&confirmed, action)?);
    assert!(matches!(
        ledger.entry(&confirmed).unwrap().state(),
        ClaimState::Confirmed { .. }
    ));

    // Checked against evidence the user attached.
    let attached = claim(&mut ledger, "attached", (THIRD, THIRD), THIRD, REASONED).await?;
    ledger
        .publish_unsettled(Unsettled::Waiting(vec![attached.clone()]), &json!({}))
        .await?;
    let check = Check {
        checker: Checker::Local { model: "m".into() },
        result: CheckResult::Supported {
            source_version_id: "v9".into(),
            supporting_quote: THIRD.into(),
        },
        reason: "The source states it.".into(),
        fingerprint: EvidenceFingerprint::of("p", "m", &[]),
        evidence: vec![],
        window_tokens: 0,
    };
    let evidence = ClaimEvent::EvidenceAttached {
        source_version_id: "v9".into(),
        provenance: Provenance::PastedText,
        result: AttachOutcome::Checked(check),
    };
    assert!(ledger.record_by_user(&attached, evidence)?);
    assert!(matches!(
        ledger.entry(&attached).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // Text the user wrote: the claim that continues their edit.
    let before = claim(&mut ledger, "before", (COMMON, COMMON), COMMON, REASONED).await?;
    let edited = claim(&mut ledger, "edited", (FOURTH, FOURTH), FOURTH, REASONED).await?;
    let edit = ClaimEvent::Superseded {
        successor: Some(edited.clone()),
        by: Actor::User,
    };
    assert!(ledger.append(&before, edit).await?);

    assert_eq!(
        ledger.releasable(&policy, &[]),
        std::slice::from_ref(&machine)
    );
    assert!(ledger.releasable(&policy, &[machine]).is_empty());
    Ok(())
}

/// A Common statement the knowledge readers passed.
async fn common_claim(ledger: &mut claim_ledger::ClaimLedger) -> Result<ClaimId> {
    let id = claim(
        ledger,
        "common",
        (COMMON, COMMON),
        COMMON,
        (Class::Common, ClassifiedBy::Model),
    )
    .await?;
    walk(
        ledger,
        &id,
        Class::Common,
        &[(Step::Read(Reading::Knowledge), Outcome::Stands)],
    )
    .await?;
    Ok(id)
}
