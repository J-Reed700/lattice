//! The calibrated bars as the gate and Settings read them (§13, decision 2):
//! recomputed on read from every recorded outcome, global across courses,
//! with every change, the verdict that made it and why.

use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{
    bar_changes, effective_bars, BarChange, Calibration, ClaimKind, EffectiveBars, OutcomeRecord,
    StaticBars, POLICY,
};
use lattice_core::shared::error::Result;
use serde::Serialize;
use sqlx::SqlitePool;

use super::SqliteSourceTrustStore;

/// What Settings shows for calibration: the bars in force with each
/// level's basis, every change oldest first, how many verdicts were
/// counted, and the policy's stated parameters (`policy.rs` documents each).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationView {
    pub bars: EffectiveBars,
    pub changes: Vec<BarChange>,
    pub verdicts: usize,
    /// The reliability a level must show to be accepted alone.
    pub accept_reliability: f64,
    /// The probability at which a measured difference moves a bar.
    pub bar_move_probability: f64,
    /// The verdicts the static table is worth in each cell.
    pub prior_strength: f64,
    pub policy: &'static str,
}

/// The bars `outcomes` calibrate the static table to.
pub fn bars_of(outcomes: &[OutcomeRecord]) -> EffectiveBars {
    let outcomes: Vec<_> = outcomes.iter().map(|record| record.outcome).collect();
    effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(&outcomes),
        &POLICY,
    )
}

/// The bars in force now.
pub async fn bars(pool: &SqlitePool) -> Result<EffectiveBars> {
    let outcomes = SqliteSourceTrustStore::new(pool.clone()).outcomes().await?;
    Ok(bars_of(&outcomes))
}

/// Calibration as Settings shows it.
pub async fn view(pool: &SqlitePool) -> Result<CalibrationView> {
    let outcomes = SqliteSourceTrustStore::new(pool.clone()).outcomes().await?;
    Ok(CalibrationView {
        bars: bars_of(&outcomes),
        changes: bar_changes(&StaticBars::table(), &outcomes, &POLICY),
        verdicts: outcomes.len(),
        accept_reliability: POLICY.accept_reliability,
        bar_move_probability: POLICY.bar_move_probability,
        prior_strength: POLICY.prior_strength,
        policy: POLICY.version,
    })
}

/// The kinds whose lowest level accepted alone differs between `before`
/// and `after`: the claims a move of the bars re-decides.
pub fn moved_kinds(before: &EffectiveBars, after: &EffectiveBars) -> Vec<ClaimKind> {
    after
        .bars
        .iter()
        .filter(|bar| before.lowest_accepted(bar.kind) != bar.lowest_accepted)
        .map(|bar| bar.kind)
        .collect()
}
