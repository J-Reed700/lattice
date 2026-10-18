//! Every move of the bars, replayed from the verdicts in time order (§13,
//! decision 2): when a level started or stopped being accepted alone for a
//! kind of claim, the verdict that tipped it, and why, in the words Settings
//! shows. Recomputed on read like the bars themselves, so the history always
//! agrees with [`super::effective_bars`]: the last change of each level is
//! the level's state there.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    capitalised, day, describe, BarBasis, ClaimKind, LabelSource, OutcomeRecord, Policy, Posterior,
    Prior, Reason, ReasonCode, StaticBars, TrustLevel, CALIBRATED_LEVELS,
};

/// One level of one kind's bar changing whether it is accepted alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarChange {
    pub kind: ClaimKind,
    pub level: TrustLevel,
    /// Unix seconds: the time of the verdict that moved it.
    pub at: i64,
    /// Whether the level is accepted as a claim's only support afterwards.
    pub accepted_alone: bool,
    /// The lowest level accepted alone for the kind before and after the
    /// verdict. Equal when the change does not move the bar `decide` reads
    /// (a level below one that is not accepted).
    pub lowest_before: Option<TrustLevel>,
    pub lowest_after: Option<TrustLevel>,
    /// The cell's verdicts up to and including this one.
    pub held: u32,
    pub failed: u32,
    /// `P(θ ≥ policy.accept_reliability)` after the verdict.
    pub probability_reliable: f64,
    /// The verdict that tipped it.
    pub outcome_id: String,
    pub label_source: LabelSource,
    pub lesson_id: String,
    pub claim_id: String,
    pub reason: Reason,
}

impl BarChange {
    /// Whether the change moved the lowest level accepted alone: the bar
    /// claims of this kind are decided against.
    pub fn moves_bar(&self) -> bool {
        self.lowest_before != self.lowest_after
    }
}

/// One cell's running verdicts and whether its level is accepted alone.
struct Running {
    posterior: Posterior,
    accepted: bool,
}

/// The changes `outcomes` made to the static bars, oldest first. A cell
/// changes when its level crosses `policy.bar_move_probability` in either
/// direction (§13 as built: relax below the static bar, tighten at or
/// above it), and again when it crosses back.
pub fn bar_changes(
    static_: &StaticBars,
    outcomes: &[OutcomeRecord],
    policy: &Policy,
) -> Vec<BarChange> {
    let prior = Prior::of(policy);
    let mut sorted: Vec<&OutcomeRecord> = outcomes
        .iter()
        .filter(|record| record.outcome.level != TrustLevel::Distrusted)
        .collect();
    // Stable, as `Calibration::from_outcomes` orders a cell's verdicts.
    sorted.sort_by_key(|record| record.outcome.at);
    let mut cells: HashMap<(ClaimKind, TrustLevel), Running> = HashMap::new();
    let mut changes = Vec::new();
    for record in sorted {
        let (kind, level) = (record.outcome.kind, record.outcome.level);
        let Some(static_lowest) = static_.lowest(kind) else {
            continue;
        };
        let statically = level >= static_lowest;
        let lowest_before = lowest(&cells, kind, static_lowest);
        let cell = cells.entry((kind, level)).or_insert(Running {
            posterior: Posterior::default(),
            accepted: statically,
        });
        if record.outcome.held() {
            cell.posterior.held += 1;
        } else {
            cell.posterior.failed += 1;
        }
        let reliable = cell
            .posterior
            .reliable_under(prior, policy.accept_reliability);
        let moves = if statically {
            1.0 - reliable >= policy.bar_move_probability
        } else {
            reliable >= policy.bar_move_probability
        };
        let accepted = statically != moves;
        if accepted == cell.accepted {
            continue;
        }
        cell.accepted = accepted;
        let (held, failed) = (cell.posterior.held, cell.posterior.failed);
        let text = explain(
            level,
            kind,
            (statically, moves),
            (held, failed, reliable),
            record.outcome.at,
            policy,
        );
        changes.push(BarChange {
            kind,
            level,
            at: record.outcome.at,
            accepted_alone: accepted,
            lowest_before,
            lowest_after: lowest(&cells, kind, static_lowest),
            held,
            failed,
            probability_reliable: reliable,
            outcome_id: record.id.clone(),
            label_source: record.label_source,
            lesson_id: record.lesson_id.clone(),
            claim_id: record.claim_id.clone(),
            reason: Reason::new(ReasonCode::CalibratedBar, text),
        });
    }
    changes
}

/// The lowest level accepted alone for `kind`, walking down from Primary
/// while each level is accepted, as `effective_bars` does.
fn lowest(
    cells: &HashMap<(ClaimKind, TrustLevel), Running>,
    kind: ClaimKind,
    static_lowest: TrustLevel,
) -> Option<TrustLevel> {
    CALIBRATED_LEVELS
        .iter()
        .copied()
        .take_while(|level| {
            cells
                .get(&(kind, *level))
                .map_or(*level >= static_lowest, |cell| cell.accepted)
        })
        .last()
}

/// The change in Settings' words: the move's own sentence (the one the
/// bars carry), or the return to the static table.
fn explain(
    level: TrustLevel,
    kind: ClaimKind,
    (statically, moves): (bool, bool),
    (held, failed, reliable): (u32, u32, f64),
    at: i64,
    policy: &Policy,
) -> String {
    let basis = match (statically, moves) {
        (false, true) => BarBasis::Relaxed {
            probability: reliable,
            held,
            failed,
            since: at,
        },
        (true, true) => BarBasis::Tightened {
            probability: 1.0 - reliable,
            held,
            failed,
            since: at,
        },
        _ => BarBasis::Static,
    };
    if let Some(text) = describe(level, kind, &basis, policy) {
        return text;
    }
    let total = held + failed;
    let share = if total == 0 {
        0.0
    } else {
        f64::from(held) * 100.0 / f64::from(total)
    };
    format!(
        "{} sources on {} claims: {total} verdicts, {share:.0}% held; back to the static bar since {} (probability {:.2} that they hold at least the {:.0}% target)",
        capitalised(level.as_str()),
        kind.as_str().replace('_', "-"),
        day(at),
        reliable,
        policy.accept_reliability * 100.0
    )
}

#[cfg(test)]
#[path = "bar_changes_tests.rs"]
mod tests;
