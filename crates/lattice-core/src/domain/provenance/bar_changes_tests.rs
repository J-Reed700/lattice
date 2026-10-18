use super::*;
use crate::domain::provenance::calibration::{effective_bars, Calibration, Outcome};
use crate::domain::provenance::origin::OriginKey;
use crate::domain::provenance::policy::POLICY;

fn record(id: usize, level: TrustLevel, held: bool, at: i64) -> OutcomeRecord {
    OutcomeRecord {
        id: format!("o{id}"),
        program_id: "p1".into(),
        lesson_id: "l1".into(),
        claim_id: format!("c{id}"),
        source_version_id: "v1".into(),
        origin_key: OriginKey("web:example.org".into()),
        // A queued claim the user confirmed held; a verified one removed
        // did not.
        outcome: Outcome {
            level,
            kind: ClaimKind::Specification,
            decision_was_sufficient: !held,
            right: false,
            at,
        },
        label_source: if held {
            LabelSource::UserConfirm
        } else {
            LabelSource::UserRemove
        },
        policy: POLICY.version.into(),
    }
}

fn run(level: TrustLevel, held: bool, n: usize, from: i64) -> Vec<OutcomeRecord> {
    (0..n)
        .map(|i| record(from as usize + i, level, held, from + i as i64))
        .collect()
}

fn bars_of(records: &[OutcomeRecord]) -> crate::domain::provenance::EffectiveBars {
    let outcomes: Vec<Outcome> = records.iter().map(|r| r.outcome).collect();
    effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(&outcomes),
        &POLICY,
    )
}

#[test]
fn no_verdicts_no_changes_and_distrusted_ones_never_count() {
    assert!(bar_changes(&StaticBars::table(), &[], &POLICY).is_empty());
    let distrusted = run(TrustLevel::Distrusted, true, 40, 0);
    assert!(bar_changes(&StaticBars::table(), &distrusted, &POLICY).is_empty());
}

#[test]
fn a_relaxed_bar_names_the_verdict_that_moved_it_and_why() {
    let records = run(TrustLevel::Established, true, 30, 1_790_000_000);
    let changes = bar_changes(&StaticBars::table(), &records, &POLICY);
    assert_eq!(changes.len(), 1, "{changes:?}");
    let change = &changes[0];
    assert_eq!(change.kind, ClaimKind::Specification);
    assert_eq!(change.level, TrustLevel::Established);
    assert!(change.accepted_alone);
    assert!(change.moves_bar());
    assert_eq!(change.lowest_before, Some(TrustLevel::Authoritative));
    assert_eq!(change.lowest_after, Some(TrustLevel::Established));
    assert!(change.probability_reliable >= POLICY.bar_move_probability);
    assert_eq!(change.failed, 0);
    let tipping = records
        .iter()
        .find(|record| record.id == change.outcome_id)
        .unwrap();
    assert_eq!(change.at, tipping.outcome.at);
    assert_eq!(
        change.held as usize,
        records
            .iter()
            .position(|r| r.id == change.outcome_id)
            .unwrap()
            + 1
    );
    assert_eq!(change.label_source, LabelSource::UserConfirm);
    assert!(change
        .reason
        .text
        .starts_with("Established sources on specification claims"));
    assert!(change.reason.text.contains("accepted alone since"));
}

#[test]
fn a_bar_that_moves_back_records_the_return() {
    let mut records = run(TrustLevel::Established, true, 30, 0);
    records.extend(run(TrustLevel::Established, false, 6, 100));
    let changes = bar_changes(&StaticBars::table(), &records, &POLICY);
    assert!(changes.len() >= 2, "{changes:?}");
    let back = changes.last().unwrap();
    assert!(!back.accepted_alone);
    assert_eq!(back.lowest_after, Some(TrustLevel::Authoritative));
    assert!(back.reason.text.contains("back to the static bar"));
}

#[test]
fn the_history_ends_where_the_bars_are() {
    let mut records = run(TrustLevel::Established, true, 25, 0);
    records.extend(run(TrustLevel::Authoritative, false, 3, 50));
    records.extend(run(TrustLevel::Primary, false, 2, 60));
    records.extend(run(TrustLevel::Primary, true, 12, 70));
    let changes = bar_changes(&StaticBars::table(), &records, &POLICY);
    let bars = bars_of(&records);
    let bar = bars.bar(ClaimKind::Specification).unwrap();
    for level_bar in &bar.levels {
        let last = changes
            .iter()
            .rev()
            .find(|change| change.level == level_bar.level);
        if let Some(last) = last {
            assert_eq!(
                last.accepted_alone, level_bar.accepted_alone,
                "{:?}",
                level_bar.level
            );
        }
    }
    let last_bar = changes.iter().rev().find(|change| change.moves_bar());
    assert_eq!(
        last_bar.map_or(Some(TrustLevel::Authoritative), |change| change
            .lowest_after),
        bar.lowest_accepted
    );
}
