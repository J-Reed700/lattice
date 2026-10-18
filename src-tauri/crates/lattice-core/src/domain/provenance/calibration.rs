//! Calibration from the user's verdicts (§13): the bars move with the
//! measured reliability of each level for each kind of claim, in both
//! directions, and every move carries its numbers.

use serde::{Deserialize, Serialize};

use super::authority::TrustLevel;
use super::claim_profile::ClaimKind;
use super::origin::OriginKey;
use super::policy::Policy;
use super::reason::{Reason, ReasonCode};
use super::sufficiency::StaticBars;
use super::temporal::day;

/// Where a labelled outcome came from (the `label_source` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelSource {
    UserAcceptSource,
    UserConfirm,
    UserRemove,
    UserEdit,
    VerifierAgree,
    VerifierDisagree,
    ContraryEvidence,
}

wire_names!(LabelSource {
    UserAcceptSource => "user_accept_source",
    UserConfirm => "user_confirm",
    UserRemove => "user_remove",
    UserEdit => "user_edit",
    VerifierAgree => "verifier_agree",
    VerifierDisagree => "verifier_disagree",
    ContraryEvidence => "contrary_evidence",
});

/// One supporting cluster of one closed claim: what the gate decided and
/// whether that decision was right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub level: TrustLevel,
    pub kind: ClaimKind,
    pub decision_was_sufficient: bool,
    pub right: bool,
    /// Unix seconds.
    pub at: i64,
}

/// An outcome as stored (§16 `source_trust_outcomes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub id: String,
    pub program_id: String,
    pub lesson_id: String,
    pub claim_id: String,
    pub source_version_id: String,
    pub origin_key: OriginKey,
    pub outcome: Outcome,
    pub label_source: LabelSource,
    pub policy: String,
}

/// Verdict counts for one (level, kind) cell. `held` counts claims the
/// user's verdict showed true, `failed` claims it showed false, whatever the
/// gate decided: a sufficient decision that was right and an insufficient
/// one that was wrong both mean the claim held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posterior {
    pub held: u32,
    pub failed: u32,
}

/// One cell's verdicts in time order, `(at, held)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationCell {
    pub level: TrustLevel,
    pub kind: ClaimKind,
    pub posterior: Posterior,
    pub observations: Vec<(i64, bool)>,
}

/// Every cell with verdicts. Global: the user's judgement does not vary per
/// course.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calibration {
    pub cells: Vec<CalibrationCell>,
}

/// Why a level is or is not accepted alone for a kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "snake_case")]
pub enum BarBasis {
    /// The §6.1 table, unmoved.
    Static,
    /// Below the static bar, accepted alone because its claims are measured
    /// reliable enough.
    Relaxed {
        probability: f64,
        held: u32,
        failed: u32,
        since: i64,
    },
    /// At or above the static bar, no longer accepted alone because its
    /// claims are measured not reliable enough.
    Tightened {
        probability: f64,
        held: u32,
        failed: u32,
        since: i64,
    },
}

/// One level's standing under one kind's bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelBar {
    pub level: TrustLevel,
    pub accepted_alone: bool,
    pub basis: BarBasis,
}

/// One kind's bar: the lowest level accepted as the only support, with
/// every level's basis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveBar {
    pub kind: ClaimKind,
    pub static_lowest: TrustLevel,
    /// `None` when calibration has tightened even Primary: no level is
    /// accepted alone and only corroboration meets the bar.
    #[serde(default)]
    pub lowest_accepted: Option<TrustLevel>,
    pub levels: Vec<LevelBar>,
    pub reasons: Vec<Reason>,
}

/// Every kind's bar, as `decide` reads it and Settings shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveBars {
    pub bars: Vec<EffectiveBar>,
    pub policy: String,
}

/// Levels a bar can move through, highest first. Distrusted is never
/// accepted: it is the user's or the injection rule's verdict, not a
/// reliability.
const CALIBRATED_LEVELS: [TrustLevel; 5] = [
    TrustLevel::Primary,
    TrustLevel::Authoritative,
    TrustLevel::Established,
    TrustLevel::Community,
    TrustLevel::Unknown,
];

/// Lentz's continued fraction for the incomplete beta function stops when a
/// term changes the value by less than this relative amount: a few units
/// in the last place of an `f64`.
const CONTINUED_FRACTION_TOLERANCE: f64 = 4.0 * f64::EPSILON;

/// The fraction converges in about √max(a, b) terms; this bound only stops
/// a non-finite input from looping. It allows a million verdicts per cell.
const CONTINUED_FRACTION_TERMS: usize = 1_000;

/// Keeps Lentz's denominators away from zero.
const CONTINUED_FRACTION_FLOOR: f64 = 1e-300;

impl Outcome {
    /// Whether the claim held: a sufficient decision that was right, or an
    /// insufficient one that was wrong (the user confirmed the claim).
    pub fn held(&self) -> bool {
        self.decision_was_sufficient == self.right
    }
}

impl Calibration {
    /// The cells of a set of outcomes, each cell's verdicts in time order.
    pub fn from_outcomes(outcomes: &[Outcome]) -> Self {
        let mut sorted: Vec<&Outcome> = outcomes
            .iter()
            .filter(|outcome| outcome.level != TrustLevel::Distrusted)
            .collect();
        sorted.sort_by_key(|outcome| outcome.at);
        let mut cells: Vec<CalibrationCell> = Vec::new();
        for outcome in sorted {
            let held = outcome.held();
            let cell = match cells
                .iter_mut()
                .position(|cell| cell.level == outcome.level && cell.kind == outcome.kind)
            {
                Some(index) => cells.get_mut(index),
                None => {
                    cells.push(CalibrationCell {
                        level: outcome.level,
                        kind: outcome.kind,
                        posterior: Posterior::default(),
                        observations: Vec::new(),
                    });
                    cells.last_mut()
                }
            };
            if let Some(cell) = cell {
                if held {
                    cell.posterior.held += 1;
                } else {
                    cell.posterior.failed += 1;
                }
                cell.observations.push((outcome.at, held));
            }
        }
        cells.sort_by(|a, b| (a.kind, b.level).cmp(&(b.kind, a.level)));
        Self { cells }
    }

    pub fn cell(&self, level: TrustLevel, kind: ClaimKind) -> Option<&CalibrationCell> {
        self.cells
            .iter()
            .find(|cell| cell.level == level && cell.kind == kind)
    }
}

/// Halvings that exhaust an `f64` mantissa: the prior's bisection is then
/// exact to the last bit.
const BISECTION_STEPS: usize = 64;

/// A Beta prior's parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prior {
    pub alpha: f64,
    pub beta: f64,
}

impl Prior {
    /// The prior every cell starts from: `policy.prior_strength`
    /// pseudo-verdicts with the median at `policy.accept_reliability`, so
    /// that before any verdict a level is as likely above the target as
    /// below it and the static bar stands.
    pub fn of(policy: &Policy) -> Self {
        let (target, weight) = (policy.accept_reliability, policy.prior_strength);
        let (mut low, mut high) = (0.0, weight);
        for _ in 0..BISECTION_STEPS {
            let alpha = (low + high) / 2.0;
            if regularized_beta(target, alpha, weight - alpha) > 0.5 {
                low = alpha;
            } else {
                high = alpha;
            }
        }
        let alpha = (low + high) / 2.0;
        Self {
            alpha,
            beta: weight - alpha,
        }
    }
}

impl Posterior {
    /// `P(θ ≥ policy.accept_reliability)`: the probability that claims at
    /// this level hold at least as often as the target, under the cell's
    /// Beta posterior.
    pub fn probability_reliable(&self, policy: &Policy) -> f64 {
        self.reliable_under(Prior::of(policy), policy.accept_reliability)
    }

    fn reliable_under(&self, prior: Prior, target: f64) -> f64 {
        let a = prior.alpha + f64::from(self.held);
        let b = prior.beta + f64::from(self.failed);
        1.0 - regularized_beta(target, a, b)
    }
}

/// The bars in force (§13): for each kind and each level, whether the level
/// is accepted alone. A level below the static bar is accepted once its
/// claims are shown, at `policy.bar_move_probability`, to hold at least
/// `policy.accept_reliability` of the time; a level at or above it stops
/// being accepted once they are shown to hold less. Recomputed on read,
/// global across courses, and every move carries its numbers.
pub fn effective_bars(
    static_: &StaticBars,
    calibration: &Calibration,
    policy: &Policy,
) -> EffectiveBars {
    let prior = Prior::of(policy);
    let bars = static_
        .entries
        .iter()
        .map(|(kind, static_lowest)| {
            let mut levels = Vec::new();
            let mut reasons = Vec::new();
            for level in CALIBRATED_LEVELS {
                let statically = level >= *static_lowest;
                let cell = calibration.cell(level, *kind);
                let basis = cell
                    .and_then(|cell| moved(cell, statically, prior, policy))
                    .unwrap_or(BarBasis::Static);
                let accepted_alone = match basis {
                    BarBasis::Static => statically,
                    BarBasis::Relaxed { .. } => true,
                    BarBasis::Tightened { .. } => false,
                };
                if let Some(text) = describe(level, *kind, &basis, policy) {
                    reasons.push(Reason::new(ReasonCode::CalibratedBar, text));
                }
                levels.push(LevelBar {
                    level,
                    accepted_alone,
                    basis,
                });
            }
            let lowest_accepted = levels
                .iter()
                .take_while(|bar| bar.accepted_alone)
                .map(|bar| bar.level)
                .last();
            EffectiveBar {
                kind: *kind,
                static_lowest: *static_lowest,
                lowest_accepted,
                levels,
                reasons,
            }
        })
        .collect();
    EffectiveBars {
        bars,
        policy: policy.version.to_owned(),
    }
}

impl EffectiveBars {
    /// The static table with no verdicts: the bars before calibration.
    pub fn unmoved(static_: &StaticBars, policy: &Policy) -> Self {
        effective_bars(static_, &Calibration::default(), policy)
    }

    pub fn bar(&self, kind: ClaimKind) -> Option<&EffectiveBar> {
        self.bars.iter().find(|bar| bar.kind == kind)
    }

    /// The lowest level accepted alone for `kind`; `None` when no level is
    /// (calibration tightened even Primary, or the kind is unknown).
    pub fn lowest_accepted(&self, kind: ClaimKind) -> Option<TrustLevel> {
        self.bar(kind).and_then(|bar| bar.lowest_accepted)
    }

    /// Why the bar for `kind` is not the static one, when it is not.
    pub fn moved_reason(&self, kind: ClaimKind) -> Option<Reason> {
        let bar = self.bar(kind)?;
        if bar.lowest_accepted == Some(bar.static_lowest) {
            return None;
        }
        let now = bar
            .lowest_accepted
            .map_or("no single source", TrustLevel::as_str);
        let because = bar
            .reasons
            .iter()
            .map(|reason| reason.text.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        Some(Reason::new(
            ReasonCode::CalibratedBar,
            format!(
                "Your verdicts moved this bar from {} to {now}: {because}",
                bar.static_lowest.as_str()
            ),
        ))
    }
}

/// The move a cell's verdicts make, if any, with the time since which they
/// have made it without interruption.
fn moved(
    cell: &CalibrationCell,
    statically: bool,
    prior: Prior,
    policy: &Policy,
) -> Option<BarBasis> {
    let mut running = Posterior::default();
    let mut since = None;
    for (at, held) in &cell.observations {
        if *held {
            running.held += 1;
        } else {
            running.failed += 1;
        }
        let reliable = running.reliable_under(prior, policy.accept_reliability);
        let moves = if statically {
            1.0 - reliable >= policy.bar_move_probability
        } else {
            reliable >= policy.bar_move_probability
        };
        since = match (moves, since) {
            (true, None) => Some(*at),
            (true, kept) => kept,
            (false, _) => None,
        };
    }
    let since = since?;
    let reliable = cell
        .posterior
        .reliable_under(prior, policy.accept_reliability);
    let (held, failed) = (cell.posterior.held, cell.posterior.failed);
    Some(if statically {
        BarBasis::Tightened {
            probability: 1.0 - reliable,
            held,
            failed,
            since,
        }
    } else {
        BarBasis::Relaxed {
            probability: reliable,
            held,
            failed,
            since,
        }
    })
}

fn describe(
    level: TrustLevel,
    kind: ClaimKind,
    basis: &BarBasis,
    policy: &Policy,
) -> Option<String> {
    let (verb, probability, held, failed, since) = match basis {
        BarBasis::Static => return None,
        BarBasis::Relaxed {
            probability,
            held,
            failed,
            since,
        } => ("accepted alone", probability, held, failed, since),
        BarBasis::Tightened {
            probability,
            held,
            failed,
            since,
        } => ("no longer accepted alone", probability, held, failed, since),
    };
    let total = held + failed;
    let share = if total == 0 {
        0.0
    } else {
        f64::from(*held) * 100.0 / f64::from(total)
    };
    Some(format!(
        "{} sources on {} claims: {total} verdicts, {share:.0}% held; {verb} since {} (probability {:.2} that they hold {} the {:.0}% target)",
        capitalised(level.as_str()),
        kind.as_str().replace('_', "-"),
        day(*since),
        probability,
        if matches!(basis, BarBasis::Relaxed { .. }) { "at least" } else { "below" },
        policy.accept_reliability * 100.0
    ))
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The regularized incomplete beta function `I_x(a, b)`: the Beta(a, b)
/// distribution function at `x`. Exact to floating-point precision (the
/// continued fraction of Numerical Recipes §6.4, evaluated where it
/// converges fastest).
pub(super) fn regularized_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front =
        (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_fraction(x, a, b) / a
    } else {
        1.0 - front * beta_fraction(1.0 - x, b, a) / b
    }
}

/// Lentz's evaluation of the incomplete beta continued fraction.
fn beta_fraction(x: f64, a: f64, b: f64) -> f64 {
    let floor = |value: f64| {
        if value.abs() < CONTINUED_FRACTION_FLOOR {
            CONTINUED_FRACTION_FLOOR
        } else {
            value
        }
    };
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 / floor(1.0 - qab * x / qap);
    let mut h = d;
    for m in 1..=CONTINUED_FRACTION_TERMS {
        let m = m as f64;
        let m2 = 2.0 * m;
        let even = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 / floor(1.0 + even * d);
        c = floor(1.0 + even / c);
        h *= d * c;
        let odd = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 / floor(1.0 + odd * d);
        c = floor(1.0 + odd / c);
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < CONTINUED_FRACTION_TOLERANCE {
            break;
        }
    }
    h
}

/// `ln Γ(x)` for `x > 0`: Lanczos' approximation with g = 7 and nine
/// coefficients (accurate to about 15 significant digits).
fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection: Γ(x)Γ(1 − x) = π / sin(πx).
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = 0.0;
    for (index, coefficient) in COEFFICIENTS.iter().enumerate() {
        sum += if index == 0 {
            *coefficient
        } else {
            coefficient / (x + index as f64)
        };
    }
    let t = x + G + 0.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

#[cfg(test)]
#[path = "calibration_tests.rs"]
mod tests;
