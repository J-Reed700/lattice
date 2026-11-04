//! A pass recorded in two steps: `identify` records which claim each statement
//! is and what is new about it, `record_checks` what was checked, and the two
//! give what `observe` gives (acceptance design §2.10).
use super::claim_ledger_tests::{pass_report, passage_finding, CELL};
use super::*;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, RepairOutcome,
};

const ATP_SENTENCE: &str =
    "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
const CELL_SOURCE: &str = "Mitochondria generate most cellular ATP by oxidative phosphorylation; the cristae of their inner membrane enlarge its surface.";
const ATP: &str = "Mitochondria make most of a cell's ATP by oxidative phosphorylation.";
const OVERSTATED: &str = "Mitochondria produce all of a cell's ATP.";
const DNA: &str = "Mitochondria carry their own circular DNA, passed down from the mother.";
const TWELVE: &str = "A typical human cell contains exactly twelve mitochondria.";
const CRISTAE: &str = "The inner membrane folds into cristae that enlarge its surface.";
/// The first sentence of `CELL` as its rewrite words it, and the statement
/// that sentence then states beside the first.
const REWRITTEN: &str =
    "Mitochondria make most, though not all, of a cell's ATP by oxidative phosphorylation.";
const BOTH: &str = "Mitochondria make most, though not all, of a cell's ATP.";

fn cell_lesson(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Mitochondria","body":body,"rubric":[]}],"questions":[]})
}

/// The events a pass added to a claim's log.
fn added(entry: &ClaimLedgerEntry, from: usize) -> Vec<&ClaimEvent> {
    entry.events()[from..]
        .iter()
        .map(|record| &record.event)
        .collect()
}

/// A ledger that found a verified claim and a failed one on the first
/// sentence of `CELL`, with a repair of the failed one sent: the two claims,
/// the verified one first.
async fn repair_sent() -> Result<(claim_ledger::ClaimLedger, ClaimId, ClaimId)> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let before = cell_lesson(CELL);
    let report = pass_report(vec![
        passage_finding(CELL, ATP, true, CELL_SOURCE),
        passage_finding(CELL, OVERSTATED, false, CELL_SOURCE),
    ]);
    ledger.observe(&before, &report).await?;
    let claims = ledger.finding_claims().to_vec();
    let (verified, failed) = (claims[0].clone(), claims[1].clone());
    ledger
        .begin_repairs(&before, std::slice::from_ref(&failed))
        .await?;
    Ok((ledger, verified, failed))
}

/// The lesson once the repair rewrote the first sentence, and what the pass
/// that follows finds: both statements, failing on the new sentence.
fn after_the_rewrite() -> (Value, LessonVerificationReport) {
    let body = CELL.replace(ATP_SENTENCE, REWRITTEN);
    let report = pass_report(vec![
        passage_finding(&body, ATP, false, CELL_SOURCE),
        passage_finding(&body, BOTH, false, CELL_SOURCE),
    ]);
    (cell_lesson(&body), report)
}

#[tokio::test]
async fn a_pass_is_matched_without_recording_anything_and_matched_the_same_again() -> Result<()> {
    let (ledger, verified, failed) = repair_sent().await?;
    let (lesson, report) = after_the_rewrite();
    let extracted = ledger.extracted(&lesson, &report);
    let entries = ledger.entries().to_vec();
    let findings = ledger.finding_claims().to_vec();
    let pending = serde_json::to_value(&ledger.pending)?;
    assert_eq!(ledger.pending.repairs.len(), 2);

    let matched = ledger.match_pass(&lesson, &extracted);
    assert_eq!(ledger.match_pass(&lesson, &extracted), matched);

    assert_eq!(matched.continues(0), Some(&verified));
    assert_eq!(matched.continues(1), Some(&failed));
    assert_eq!(matched.continues(2), None);
    // The repairs and the lesson before them wait for `identify` to take them.
    assert_eq!(ledger.entries(), entries.as_slice());
    assert_eq!(ledger.finding_claims(), findings.as_slice());
    assert_eq!(serde_json::to_value(&ledger.pending)?, pending);
    Ok(())
}

#[tokio::test]
async fn identifying_a_new_claim_records_where_it_stands_and_its_class_and_no_check() -> Result<()>
{
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let mut finding = passage_finding(CELL, ATP, true, CELL_SOURCE);
    finding.class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    let report = pass_report(vec![finding]);

    let extracted = ledger.extracted(&lesson, &report);
    let matched = ledger.match_pass(&lesson, &extracted);
    let ids = ledger.identify(&lesson, extracted, matched).await?;

    assert_eq!(ids.len(), 1);
    assert_eq!(ledger.finding_claims(), ids.as_slice());
    let entry = ledger.entry(&ids[0]).unwrap();
    let events = added(entry, 0);
    assert!(
        matches!(
            events.as_slice(),
            [
                ClaimEvent::Extracted { .. },
                ClaimEvent::Classified {
                    class: Class::Sourced,
                    by: ClassifiedBy::Writer
                }
            ]
        ),
        "{events:?}"
    );
    assert!(matches!(entry.state(), ClaimState::Unchecked));
    Ok(())
}

#[tokio::test]
async fn identifying_a_repaired_claim_that_continues_records_its_repair_and_no_check() -> Result<()>
{
    let (mut ledger, verified, failed) = repair_sent().await?;
    let (lesson, report) = after_the_rewrite();
    let from: Vec<usize> = [&verified, &failed]
        .iter()
        .map(|id| ledger.entry(id).unwrap().events().len())
        .collect();

    let extracted = ledger.extracted(&lesson, &report);
    let matched = ledger.match_pass(&lesson, &extracted);
    let ids = ledger.identify(&lesson, extracted, matched).await?;

    assert_eq!(ids, vec![verified.clone(), failed.clone()]);
    assert_eq!(ledger.finding_claims(), ids.as_slice());
    for (id, from) in [&verified, &failed].into_iter().zip(from) {
        let events = added(ledger.entry(id).unwrap(), from);
        assert!(
            matches!(
                events.as_slice(),
                [
                    ClaimEvent::Extracted { .. },
                    ClaimEvent::RepairAttempted {
                        attempt: 1,
                        escalated: false,
                        outcome: RepairOutcome::Rewritten,
                        ..
                    }
                ]
            ),
            "{events:?}"
        );
    }
    // The pass took the repairs it decided, and the lesson before them.
    assert!(ledger.pending.repairs.is_empty());
    assert!(ledger.pending.previous.is_none());
    Ok(())
}

#[tokio::test]
async fn identifying_a_claim_no_statement_continues_records_that_it_is_gone_and_no_check(
) -> Result<()> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let before = cell_lesson(CELL);
    let supported = pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]);
    ledger.observe(&before, &supported).await?;
    let id = ledger.finding_claims()[0].clone();
    let from = ledger.entry(&id).unwrap().events().len();

    let cut = cell_lesson(&CELL.replace(&format!("{ATP_SENTENCE} "), ""));
    let report = pass_report(vec![]);
    let extracted = ledger.extracted(&cut, &report);
    let matched = ledger.match_pass(&cut, &extracted);
    let ids = ledger.identify(&cut, extracted, matched).await?;

    assert!(ids.is_empty());
    assert!(ledger.finding_claims().is_empty());
    let entry = ledger.entry(&id).unwrap();
    let events = added(entry, from);
    assert!(
        matches!(
            events.as_slice(),
            [ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine
            }]
        ),
        "{events:?}"
    );
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    Ok(())
}

/// A ledger that found four claims in `CELL` (the first sentence's two among
/// them) and has had a repair of the failed one sent.
async fn four_claims_and_a_repair_sent() -> Result<claim_ledger::ClaimLedger> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let before = cell_lesson(CELL);
    let report = pass_report(vec![
        passage_finding(CELL, ATP, true, CELL_SOURCE),
        passage_finding(CELL, OVERSTATED, false, CELL_SOURCE),
        passage_finding(CELL, DNA, true, CELL_SOURCE),
        passage_finding(CELL, TWELVE, true, CELL_SOURCE),
    ]);
    ledger.observe(&before, &report).await?;
    let failed = ledger.finding_claims()[1].clone();
    ledger
        .begin_repairs(&before, std::slice::from_ref(&failed))
        .await?;
    Ok(ledger)
}

/// Each claim's events, in the order the ledger holds its claims.
fn events_of(ledger: &claim_ledger::ClaimLedger) -> Vec<Vec<ClaimEvent>> {
    ledger
        .entries()
        .iter()
        .map(|entry| {
            entry
                .events()
                .iter()
                .map(|record| record.event.clone())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn identifying_then_recording_the_checks_gives_the_events_observing_gives() -> Result<()> {
    // The pass that follows the rewrite finds the two statements of the first
    // sentence again, the twelve-mitochondria claim unchanged, a claim new to
    // the ledger, and none for the DNA sentence.
    let body = CELL.replace(ATP_SENTENCE, REWRITTEN);
    let lesson = cell_lesson(&body);
    let report = pass_report(vec![
        passage_finding(&body, ATP, false, CELL_SOURCE),
        passage_finding(&body, BOTH, false, CELL_SOURCE),
        passage_finding(&body, TWELVE, true, CELL_SOURCE),
        passage_finding(&body, CRISTAE, true, CELL_SOURCE),
    ]);

    let mut observed = four_claims_and_a_repair_sent().await?;
    observed.observe(&lesson, &report).await?;

    let mut split = four_claims_and_a_repair_sent().await?;
    let extracted = split.extracted(&lesson, &report);
    let matched = split.match_pass(&lesson, &extracted);
    let ids = split.identify(&lesson, extracted, matched).await?;
    split.record_checks(&lesson, &report, &ids).await?;

    assert_eq!(events_of(&split), events_of(&observed));
    let kinds: HashSet<&str> = split
        .entries()
        .iter()
        .flat_map(ClaimLedgerEntry::events)
        .map(|record| record.event.kind_name())
        .collect();
    for kind in ["extracted", "checked", "repair_attempted", "superseded"] {
        assert!(kinds.contains(kind), "no {kind} among {kinds:?}");
    }
    assert_eq!(split.entries().len(), 5);
    assert_eq!(
        split.finding_claims().len(),
        observed.finding_claims().len()
    );
    assert_eq!(
        serde_json::to_value(&split.pending)?,
        serde_json::to_value(&observed.pending)?
    );
    assert_eq!(
        split.bind(report.clone()).issues,
        observed.bind(report.clone()).issues
    );
    Ok(())
}
