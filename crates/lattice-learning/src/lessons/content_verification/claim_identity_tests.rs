//! A pass identified with the ledger: `identify` records which claim each
//! statement is and what is new about it, and records no check (acceptance
//! design §2.10).
use super::acceptance_switch_tests::{claim, walk, REWRITING, SOURCED};
use super::claim_ledger_tests::{
    check_findings, identify_findings, pass_report, passage_finding, sourced, CELL,
};
use super::*;
use lattice_core::domain::learning::acceptance::Step;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, RepairOutcome,
    UnitKind, UnverifiedReason,
};

const ATP_SENTENCE: &str =
    "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
const CELL_SOURCE: &str = "Mitochondria generate most cellular ATP by oxidative phosphorylation; the cristae of their inner membrane enlarge its surface.";
const ATP: &str = "Mitochondria make most of a cell's ATP by oxidative phosphorylation.";
const OVERSTATED: &str = "Mitochondria produce all of a cell's ATP.";
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

#[tokio::test]
async fn a_statement_the_user_wrote_that_no_pass_continues_is_not_ended_while_its_sentence_stands(
) -> Result<()> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let id = ClaimId("written".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(0, UnitKind::Block, "body", ATP_SENTENCE, CELL),
        statement: ATP_SENTENCE.into(),
    };
    ledger.append(&id, extracted).await?;
    let published = ClaimEvent::PublishedUnverified {
        reason: UnverifiedReason::Unchecked,
    };
    ledger.append(&id, published).await?;
    let from = ledger.entry(&id).unwrap().events().len();

    identify_findings(&mut ledger, &lesson, &pass_report(vec![])).await?;

    let entry = ledger.entry(&id).unwrap();
    assert!(added(entry, from).is_empty(), "{:?}", added(entry, from));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    Ok(())
}

/// A ledger that found a verified claim and a failed one on the first
/// sentence of `CELL`, with a repair of the failed one sent: the two claims,
/// the verified one first.
async fn repair_sent() -> Result<(claim_ledger::ClaimLedger, ClaimId, ClaimId)> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let before = cell_lesson(CELL);
    let report = pass_report(sourced(vec![
        passage_finding(CELL, ATP, true, CELL_SOURCE),
        passage_finding(CELL, OVERSTATED, false, CELL_SOURCE),
    ]));
    let claims = identify_findings(&mut ledger, &before, &report).await?;
    check_findings(&mut ledger, &claims, &report.findings).await?;
    let (verified, failed) = (claims[0].clone(), claims[1].clone());
    walk(&mut ledger, &failed, Class::Sourced, &REWRITING).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &before,
            std::slice::from_ref(&failed),
        )
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
    let claims: Vec<(usize, Claim)> = report
        .findings
        .iter()
        .map(|finding| {
            let (quote, statement) = (finding.quote.clone(), finding.statement.clone());
            (finding.unit, Claim { quote, statement })
        })
        .collect();
    let extracted = ledger.extracted(&lesson, &claims, &[]);
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

    let ids = identify_findings(&mut ledger, &lesson, &report).await?;

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

    let ids = identify_findings(&mut ledger, &lesson, &report).await?;

    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1], failed);
    assert_ne!(ids[0], verified);
    assert_eq!(ledger.finding_claims(), ids.as_slice());
    // The verified claim shares the rewritten sentence: a machine successor
    // replaces it, and the failed target continues.
    assert!(matches!(
        ledger.entry(&verified).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == ids[0]
    ));
    let events = added(ledger.entry(&failed).unwrap(), from[1]);
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
    let checked = |entry: &ClaimLedgerEntry, from: usize| {
        added(entry, from)
            .iter()
            .any(|event| matches!(event, ClaimEvent::Checked(_)))
    };
    assert!(!checked(ledger.entry(&verified).unwrap(), from[0]));
    assert!(!checked(ledger.entry(&failed).unwrap(), from[1]));
    assert!(!checked(ledger.entry(&ids[0]).unwrap(), 0));
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
    let ids = identify_findings(&mut ledger, &before, &supported).await?;
    check_findings(&mut ledger, &ids, &supported.findings).await?;
    let id = ids[0].clone();
    let from = ledger.entry(&id).unwrap().events().len();

    let cut = cell_lesson(&CELL.replace(&format!("{ATP_SENTENCE} "), ""));
    let report = pass_report(vec![]);
    let ids = identify_findings(&mut ledger, &cut, &report).await?;

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

#[tokio::test]
async fn a_claim_no_statement_continues_whose_sentence_stands_is_a_fault_that_stops_the_run_not_a_removal(
) -> Result<()> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let id = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let from = ledger.entry(&id).unwrap().events().len();

    // The pass names no statement for a sentence the lesson still holds, and
    // no repair awaits the claim.
    let ids = identify_findings(&mut ledger, &lesson, &pass_report(vec![])).await?;

    assert!(ids.is_empty());
    let fault = ledger.fault(&StandardPolicy::new());
    assert!(
        fault
            .as_ref()
            .is_some_and(|stop| stop.message.contains(ATP)),
        "the run does not stop on the statement the pass left out: {fault:?}"
    );
    let entry = ledger.entry(&id).unwrap();
    assert!(
        added(entry, from).is_empty(),
        "the claim was not left as it was: {:?}",
        added(entry, from)
    );
    assert!(
        !matches!(
            entry.state(),
            ClaimState::Removed { .. } | ClaimState::Edited { .. }
        ),
        "{:?}",
        entry.state()
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_in_a_section_that_goes_is_ended_removed_though_its_sentence_stands() -> Result<()>
{
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let id = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    ledger.cuts.go(0, "Mitochondria".into());

    identify_findings(&mut ledger, &lesson, &pass_report(vec![])).await?;

    // The section stays in the draft until the lesson is final, with no
    // statement of its own; its claim goes with it.
    assert_eq!(ledger.fault(&StandardPolicy::new()), None);
    assert!(
        matches!(
            ledger.entry(&id).unwrap().state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        ledger.entry(&id).unwrap().state()
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_a_repair_awaits_is_decided_by_the_pass_that_finds_no_statement_for_it(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let id = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let to_rewrite = [
        (Step::Read(Reading::Parts), Outcome::Unproven),
        (Step::Research, Outcome::FoundNothing),
        (Step::Read(Reading::Page), Outcome::Refuted),
    ];
    walk(&mut ledger, &id, Class::Sourced, &to_rewrite).await?;
    ledger
        .begin_repairs(&policy, &lesson, std::slice::from_ref(&id))
        .await?;

    identify_findings(&mut ledger, &lesson, &pass_report(vec![])).await?;

    assert_eq!(ledger.fault(&policy), None);
    assert!(
        ledger
            .entry(&id)
            .unwrap()
            .events()
            .iter()
            .any(|record| matches!(record.event, ClaimEvent::RepairAttempted { .. })),
        "the pass records what became of the repair"
    );
    Ok(())
}
