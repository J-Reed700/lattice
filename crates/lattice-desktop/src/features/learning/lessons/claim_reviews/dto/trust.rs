//! Source trust in the claim review (source provenance §13, §18.7): the
//! failure a claim's support below its bar gives, why a trust change
//! reopened a claim, and what the gate weighed, as the card and its History
//! show them.

use lattice_core::domain::learning::claim_review::{TrustAssessment, TrustCause};
use lattice_core::domain::provenance::{CountedSupport, Decision, TrustLevel};
use serde::{Deserialize, Serialize};

use crate::features::learning::dto::LearningTrustLevel;

/// One support as the gate counted it: `level` is `null` when it was
/// excluded (distrusted, ruled out by its date).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CountedSupportDto {
    pub source_version_id: String,
    pub level: Option<LearningTrustLevel>,
}

impl From<&CountedSupport> for CountedSupportDto {
    fn from(counted: &CountedSupport) -> Self {
        Self {
            source_version_id: counted.source_version_id.clone(),
            level: counted.counted.map(level),
        }
    }
}

/// What kind of work a source is, as the review names it (UI design §4.3):
/// from its judged nature and publisher kind and its library kind; `page`
/// when nothing more is known.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindDto {
    OnlineBook,
    Documentation,
    ReferenceWork,
    PeerReviewedPaper,
    Report,
    Article,
    Guide,
    Forum,
    Wiki,
    MarketingPage,
    YourFile,
    PastedText,
    Page,
}

/// What moved the trust a decision rested on.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum TrustCauseDto {
    /// An override was set or cleared on these source versions.
    Override {
        #[serde(rename = "sourceVersionIds")]
        source_version_ids: Vec<String>,
    },
    /// Calibration moved the bar for this kind of claim (`specification`,
    /// `measurement`, …).
    Bar {
        #[serde(rename = "claimKind")]
        claim_kind: String,
    },
}

impl From<&TrustCause> for TrustCauseDto {
    fn from(cause: &TrustCause) -> Self {
        match cause {
            TrustCause::Override { source_version_ids } => Self::Override {
                source_version_ids: source_version_ids.clone(),
            },
            TrustCause::Bar { kind } => Self::Bar {
                claim_kind: kind.as_str().to_owned(),
            },
        }
    }
}

/// A `StandingAssessed` event's payload: the claim's kind, whether its
/// support met the bar (and which bar it failed), each support's level for
/// the claim's subject, the gate's reason, and the trust change that made
/// the gate decide again (`null` for the check it followed).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StandingAssessedDto {
    pub claim_kind: String,
    pub sufficient: bool,
    /// `primary`, `origin`, `authoritative`, `established`, `attributed` or
    /// `any`, when the support failed it.
    pub bar: Option<String>,
    pub supports: Vec<CountedSupportDto>,
    pub reason: String,
    pub cause: Option<TrustCauseDto>,
}

impl StandingAssessedDto {
    pub fn of(trust: &TrustAssessment, reason: &str, cause: Option<&TrustCause>) -> Self {
        let bar = match &trust.decision {
            Decision::Sufficient { .. } => None,
            Decision::Insufficient { bar, .. } => Some(bar_name(*bar)),
        };
        Self {
            claim_kind: trust.profile.kind.as_str().to_owned(),
            sufficient: bar.is_none(),
            bar,
            supports: trust
                .supports
                .iter()
                .map(|support| CountedSupportDto {
                    source_version_id: support.source_version_id.clone(),
                    level: Some(level(support.trust.level)),
                })
                .collect(),
            reason: reason.to_owned(),
            cause: cause.map(Into::into),
        }
    }
}

/// The bar's wire name, as `LearningSufficiencyDto.bar` spells it.
pub(super) fn bar_name(bar: lattice_core::domain::provenance::Bar) -> String {
    serde_json::to_value(bar)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(super) fn level(level: TrustLevel) -> LearningTrustLevel {
    match level {
        TrustLevel::Primary => LearningTrustLevel::Primary,
        TrustLevel::Authoritative => LearningTrustLevel::Authoritative,
        TrustLevel::Established => LearningTrustLevel::Established,
        TrustLevel::Community => LearningTrustLevel::Community,
        TrustLevel::Unknown => LearningTrustLevel::Unknown,
        TrustLevel::Distrusted => LearningTrustLevel::Distrusted,
    }
}
