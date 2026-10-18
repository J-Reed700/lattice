//! Calibration from the user's verdicts (§13): the bars move with the
//! measured reliability of each level for each kind of claim, in both
//! directions, and every move carries its numbers.

use serde::{Deserialize, Serialize};

use super::authority::TrustLevel;
use super::claim_profile::ClaimKind;
use super::origin::OriginKey;
use super::reason::Reason;

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
