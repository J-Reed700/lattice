//! The teaching review after a claim repair or a user edit repairs the
//! sentence of a machine's claim that has attempts left, and holds the rest for
//! the user (claim sources design §10.3).
use super::acceptance_switch_tests::{claim, identified, lesson_of, node_of, open, walk, REASONED};
use super::class_checks_tests::Classes;
use super::*;
use lattice_core::domain::learning::acceptance::{Outcome, Reading, Step};
use lattice_core::domain::learning::claim_review::{Actor, ClaimState};

const FIRST: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const SECOND: &str =
    "The reader hands over one row at a time, so it never holds the whole file in memory.";
const THIRD: &str = "A row is handed over and dropped before the next one is read from the file.";
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
        .begin_teaching(&lesson_of(from), std::slice::from_ref(id))
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
    assert_eq!(ledger.releasable(&policy, &[]), [id.clone()]);

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
    assert_eq!(ledger.releasable(&policy, &[]), [id.clone()]);

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

#[tokio::test]
async fn the_claims_the_user_decided_and_the_claims_a_repair_is_for_are_not_released() -> Result<()>
{
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let machine = verified(&mut ledger, "reader", FIRST).await?;
    let common = claim(
        &mut ledger,
        "common",
        (COMMON, COMMON),
        COMMON,
        (Class::Common, ClassifiedBy::Model),
    )
    .await?;
    walk(
        &mut ledger,
        &common,
        Class::Common,
        &[(Step::Read(Reading::Knowledge), Outcome::Stands)],
    )
    .await?;
    assert_eq!(ledger.releasable(&policy, &[]), [machine.clone()]);
    assert!(ledger.releasable(&policy, &[machine]).is_empty());
    Ok(())
}
