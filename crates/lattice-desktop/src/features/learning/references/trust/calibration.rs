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
    /// Each cell's verdicts and posterior, which the bars were moved by.
    pub calibration: Calibration,
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

/// The bars in force and the calibration that moved them.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibrated {
    pub bars: EffectiveBars,
    pub calibration: Calibration,
}

fn calibrated_of(outcomes: &[OutcomeRecord]) -> Calibrated {
    let outcomes: Vec<_> = outcomes.iter().map(|record| record.outcome).collect();
    let calibration = Calibration::from_outcomes(&outcomes);
    Calibrated {
        bars: effective_bars(&StaticBars::table(), &calibration, &POLICY),
        calibration,
    }
}

/// The bars `outcomes` calibrate the static table to.
pub fn bars_of(outcomes: &[OutcomeRecord]) -> EffectiveBars {
    calibrated_of(outcomes).bars
}

/// The bars in force now.
pub async fn bars(pool: &SqlitePool) -> Result<EffectiveBars> {
    Ok(calibrated(pool).await?.bars)
}

/// The bars in force now, with each cell's verdicts.
pub async fn calibrated(pool: &SqlitePool) -> Result<Calibrated> {
    let outcomes = SqliteSourceTrustStore::new(pool.clone()).outcomes().await?;
    Ok(calibrated_of(&outcomes))
}

/// Calibration as Settings shows it.
pub async fn view(pool: &SqlitePool) -> Result<CalibrationView> {
    let outcomes = SqliteSourceTrustStore::new(pool.clone()).outcomes().await?;
    let Calibrated { bars, calibration } = calibrated_of(&outcomes);
    Ok(CalibrationView {
        bars,
        calibration,
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
