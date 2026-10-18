//! The bars in force, as Settings shows them (§13): for every claim kind,
//! each level with its verdicts, whether it is accepted alone and since
//! when, every automatic change with its numbers, and the stated policy
//! parameters with their derivations (`domain::provenance::policy`).
//!
//! The bars are lattice-core's `effective_bars` over every recorded outcome,
//! recomputed on read as the gate computes them.
//! INTEGRATION (P5/P7): when `references::trust::calibration` lands, read
//! the bars and outcomes through it so Settings and the gate share one
//! loader.

use super::dto_map;
use super::repository::SqliteSourceTrustStore;
use crate::features::learning::dto::{
    LearningSourceTrustCalibrationDto, LearningTrustPolicyParameterDto,
};
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{effective_bars, Calibration, Policy, StaticBars, POLICY};
use lattice_core::shared::error::Result;

/// The parameters calibration and the copy rule compare against, each with
/// the derivation `policy.rs` states for it.
fn parameters(policy: &Policy) -> Vec<LearningTrustPolicyParameterDto> {
    let parameter = |name: &str, value: f64, derivation: &str| LearningTrustPolicyParameterDto {
        name: name.to_owned(),
        value,
        derivation: derivation.to_owned(),
    };
    vec![
        parameter(
            "accept_reliability",
            policy.accept_reliability,
            "A level is accepted as a claim's only support when its claims hold this often. A wrong published claim costs a misconception to unlearn; a right claim held for review costs one confirmation. At 0.9 an error is weighed as nine confirmations.",
        ),
        parameter(
            "bar_move_probability",
            policy.bar_move_probability,
            "How sure calibration must be that a level's reliability is above (relax) or below (tighten) the target before it moves a bar. One number for both directions, so bars move on equally strong evidence either way.",
        ),
        parameter(
            "prior_strength",
            policy.prior_strength,
            "Each cell starts as if it had this many verdicts at exactly the target, so the design's table stands until your verdicts outweigh it: relaxing an empty cell takes 21 straight held claims, tightening it two straight failures.",
        ),
        parameter(
            "resemblance_copy",
            policy.resemblance_copy,
            "Two texts at or above this resemblance are copies of each other; the F1-optimal threshold on the labelled independence pairs.",
        ),
        parameter(
            "containment_copy",
            policy.containment_copy,
            "A text containing this much of another holds a copy of it; the F1-optimal threshold on the labelled independence pairs.",
        ),
    ]
}

pub async fn calibration(
    store: &SqliteSourceTrustStore,
) -> Result<LearningSourceTrustCalibrationDto> {
    let outcomes: Vec<_> = store
        .outcomes()
        .await?
        .into_iter()
        .map(|record| record.outcome)
        .collect();
    let calibration = Calibration::from_outcomes(&outcomes);
    let bars = effective_bars(&StaticBars::table(), &calibration, &POLICY);
    Ok(LearningSourceTrustCalibrationDto {
        policy_version: bars.policy.clone(),
        parameters: parameters(&POLICY),
        kinds: bars
            .bars
            .iter()
            .map(|bar| dto_map::kind_bar(bar, &calibration, &POLICY))
            .collect(),
        verdicts: u32::try_from(outcomes.len()).unwrap_or(u32::MAX),
    })
}
