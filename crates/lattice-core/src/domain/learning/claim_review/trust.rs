//! Source trust in the claim ledger (source provenance §6.2, §13, §18.7):
//! what the provenance gate weighed for a supported claim, the pure
//! re-decision when an override or a calibrated bar changes, and the
//! outcomes a verdict on such a claim gives calibration.
//!
//! A claim's assessment is recorded right after the local check the gate
//! decided ([`super::ClaimEvent::StandingAssessed`] with no cause), so the
//! ledger always knows whether the claim's current state is the gate's. A
//! trust change records the assessment again with its cause; replaying it
//! verifies an insufficient claim the change made sufficient, reopens a
//! verified one it made insufficient, and never touches a human verdict.
//! Both are decided by lattice-core's `decide`, with no model call.

use serde::{Deserialize, Serialize};

use super::{ClaimLedgerEntry, ClaimState, ReopenCause, VerdictKind};
use crate::domain::provenance::{
    decide, overturn, resolve_all, AddedBy, Available, ClaimKind, ClaimProfile, ClusterId,
    Contrary, Decision, EffectiveBars, LabelSource, Outcome, Overturn, Support, TrustLevel,
};

/// The checker's supported verdict the gate weighed: what verifies the
/// claim whenever the decision is sufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportedBy {
    pub source_version_id: String,
    pub supporting_quote: String,
}

/// What the provenance gate weighed for one supported claim: the claim's
/// profile, each supporting source levelled for its subject, the contrary
/// passages, the decision, and the trust policy it was decided under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustAssessment {
    pub profile: ClaimProfile,
    pub supports: Vec<Support>,
    #[serde(default)]
    pub contrary: Vec<Contrary>,
    pub decision: Decision,
    pub supported: SupportedBy,
    pub policy: String,
    /// What the course has on the claim's subject, when the standing reach
    /// weighed it (claim sources design §6.1); `None` decides as before it.
    #[serde(default)]
    pub available: Option<Available>,
}

/// What the gate makes of a supported claim: verified, or the failure the
/// claim is held to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    Sufficient,
    Fails(VerdictKind),
}

impl TrustAssessment {
    /// The overturn rule (§11) over this assessment.
    pub fn overturn(&self) -> Overturn {
        overturn(
            &self.decision,
            &resolve_all(&self.profile, &self.supports, &self.contrary),
        )
    }

    /// The ledger's reading of [`Self::overturn`]: supports that fail the
    /// bar are `InsufficientStanding`; a contrary that wins, or one nobody
    /// outranks, is a contradiction for the user.
    pub fn verdict(&self) -> GateVerdict {
        match self.overturn() {
            Overturn::None => GateVerdict::Sufficient,
            Overturn::InsufficientStanding { bar, have, .. } => {
                GateVerdict::Fails(VerdictKind::InsufficientStanding { bar, have })
            }
            Overturn::Contradicted { .. } | Overturn::Escalated { .. } => {
                GateVerdict::Fails(VerdictKind::Contradicted)
            }
        }
    }

    /// The gate's decision again, over `supports` (re-levelled for new
    /// overrides, or as recorded) and the bars in force. Pure: `decide`
    /// reads judgements already made, so a trust change costs no model call.
    pub fn redecide(&self, supports: Vec<Support>, bars: &EffectiveBars) -> Self {
        let decision = decide(
            &self.profile,
            &supports,
            &self.contrary,
            bars,
            &self.available.unwrap_or_default(),
        );
        Self {
            supports,
            decision,
            ..self.clone()
        }
    }

    /// Whether any supporting or contrary passage comes from one of
    /// `versions`.
    pub fn cites(&self, versions: &[String]) -> bool {
        let named = |id: &String| versions.contains(id);
        self.supports
            .iter()
            .any(|support| named(&support.source_version_id))
            || self
                .contrary
                .iter()
                .any(|contrary| named(&contrary.source_version_id))
    }
}

/// What moved the trust a gate decision rested on (§13, §18.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustCause {
    /// An override was set or cleared; the source versions it covers.
    Override { source_version_ids: Vec<String> },
    /// Calibration moved the lowest level accepted alone for a kind.
    Bar { kind: ClaimKind },
}

impl TrustCause {
    /// Whether the change can alter `trust`'s decision.
    pub fn affects(&self, trust: &TrustAssessment) -> bool {
        match self {
            Self::Override { source_version_ids } => trust.cites(source_version_ids),
            Self::Bar { kind } => trust.profile.kind == *kind,
        }
    }
}

/// A verdict on a claim that calibration learns from (§13's table). Each
/// says whether the claim held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustVerdict {
    /// The user confirmed the claim: it held.
    Confirm,
    /// The user removed the claim's sentence: as written, it did not hold.
    Remove,
    /// The user rewrote the claim: as written, it did not hold.
    Edit,
    /// Evidence the user attached verified the claim: it held.
    EvidenceAdded,
    /// The Verifier audit found the claim supported: it held.
    VerifierAgreed,
}

impl TrustVerdict {
    pub fn held(self) -> bool {
        matches!(
            self,
            Self::Confirm | Self::EvidenceAdded | Self::VerifierAgreed
        )
    }
}

/// One supporting cluster's outcome, for the `source_trust_outcomes` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationOutcome {
    /// The cluster's best support: the version the row names.
    pub source_version_id: String,
    pub outcome: Outcome,
    pub label_source: LabelSource,
}

/// §13's outcome table: the outcomes `verdict` on `entry` (the claim as it
/// stood before the verdict) gives, one per supporting cluster at the level
/// the gate counted it. Only a claim whose current state is the gate's
/// yields any; the user's own evidence is never calibrated.
///
/// | verdict | gate decided | outcome (`sufficient`, `right`) | label |
/// |---|---|---|---|
/// | Confirm | insufficient | (false, false) | `user_confirm` |
/// | Confirm | sufficient (Verifier disputed) | (true, true) | `user_confirm` |
/// | Remove / Edit | sufficient | (true, false) | `user_remove` / `user_edit`, `verifier_disagree` after a Verifier dispute |
/// | Remove / Edit | insufficient | (false, true) | as above |
/// | EvidenceAdded | insufficient | (false, false) | `evidence_added` |
/// | VerifierAgreed | sufficient | (true, true) | `verifier_agree` |
///
/// The Verifier agreeing with a claim the gate held back gives none: the
/// Verifier judges entailment, not standing, so it says nothing about the
/// bar (§13 has no such row).
pub fn trust_outcomes(
    entry: &ClaimLedgerEntry,
    verdict: TrustVerdict,
    at: i64,
) -> Vec<CalibrationOutcome> {
    let Some(trust) = entry.trust() else {
        return Vec::new();
    };
    let sufficient = matches!(trust.decision, Decision::Sufficient { .. });
    if verdict == TrustVerdict::VerifierAgreed && !sufficient {
        return Vec::new();
    }
    let label_source = match verdict {
        TrustVerdict::Confirm => LabelSource::UserConfirm,
        TrustVerdict::Remove | TrustVerdict::Edit if verifier_disputed(entry.state()) => {
            LabelSource::VerifierDisagree
        }
        TrustVerdict::Remove => LabelSource::UserRemove,
        TrustVerdict::Edit => LabelSource::UserEdit,
        TrustVerdict::EvidenceAdded => LabelSource::EvidenceAdded,
        TrustVerdict::VerifierAgreed => LabelSource::VerifierAgree,
    };
    counted_clusters(trust)
        .into_iter()
        .map(|(source_version_id, level)| CalibrationOutcome {
            source_version_id,
            outcome: Outcome {
                level,
                kind: trust.profile.kind,
                decision_was_sufficient: sufficient,
                right: sufficient == verdict.held(),
                at,
            },
            label_source,
        })
        .collect()
}

/// The Verifier disputed the claim and the user has not resolved it since.
fn verifier_disputed(state: &ClaimState) -> bool {
    matches!(
        state,
        ClaimState::Open {
            kind: VerdictKind::Disputed { .. },
            ..
        } | ClaimState::Queued {
            kind: VerdictKind::Disputed { .. },
            ..
        } | ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            ..
        }
    )
}

/// Each supporting cluster's best support and the level the gate counted
/// it at: the supports that met the bar when it was met, else every
/// support the gate counted (an excluded one taught it nothing). Supports
/// the user added are left out.
fn counted_clusters(trust: &TrustAssessment) -> Vec<(String, TrustLevel)> {
    let support = |id: &str| {
        trust
            .supports
            .iter()
            .find(|support| support.source_version_id == id && support.added_by != AddedBy::User)
    };
    let counted: Vec<(&Support, TrustLevel)> = match &trust.decision {
        Decision::Sufficient { met_by, .. } => met_by
            .iter()
            .filter_map(|id| support(id))
            .map(|support| (support, support.trust.level))
            .collect(),
        Decision::Insufficient { have, .. } => have
            .iter()
            .filter_map(|counted| Some((support(&counted.source_version_id)?, counted.counted?)))
            .collect(),
    };
    let mut best: Vec<(&ClusterId, String, TrustLevel)> = Vec::new();
    for (support, level) in counted {
        match best
            .iter_mut()
            .find(|(cluster, ..)| **cluster == support.origin_cluster)
        {
            Some(entry) if entry.2 < level => {
                entry.1 = support.source_version_id.clone();
                entry.2 = level;
            }
            Some(_) => {}
            None => best.push((
                &support.origin_cluster,
                support.source_version_id.clone(),
                level,
            )),
        }
    }
    best.into_iter()
        .map(|(_, version, level)| (version, level))
        .collect()
}
