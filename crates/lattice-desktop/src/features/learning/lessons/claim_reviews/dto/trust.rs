//! Source trust in the claim review (source provenance §13, §18.7): the
//! failure a claim's support below its bar gives, why a trust change
//! reopened a claim, and what the gate weighed, as the card and its History
//! show them.

use lattice_core::domain::learning::claim_review::{TrustAssessment, TrustCause};
use lattice_core::domain::provenance::{
    ContentNature, CountedSupport, Decision, PublisherKind, TrustLevel, WorkEvidence,
};
use serde::{Deserialize, Serialize};

use crate::features::learning::dto::{LearningSourceKind, LearningTrustLevel};

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

/// The kind of work a source is: the user's own file or pasted text by its
/// library kind, else by its judged nature and publisher kind and the shape
/// of the work the judgement read (`None` when no judgement of it is
/// stored, which reads as `page`). A reference text is an online book when
/// it is chaptered: its address is a path of numbered chapters, or a
/// publisher issues it sectioned (checker design §4.2).
pub fn source_kind(
    judged: Option<(ContentNature, PublisherKind, &WorkEvidence)>,
    library: &LearningSourceKind,
) -> SourceKindDto {
    use ContentNature as N;
    use PublisherKind as P;
    match library {
        LearningSourceKind::Document => return SourceKindDto::YourFile,
        LearningSourceKind::Pasted => return SourceKindDto::PastedText,
        LearningSourceKind::Web => {}
    }
    let Some((nature, publisher, work)) = judged else {
        return SourceKindDto::Page;
    };
    let chaptered = work.numbered_path || (publisher == P::Publisher && work.structured());
    match (nature, publisher) {
        (N::PrimaryDocument | N::Reporting, P::Academic) => SourceKindDto::PeerReviewedPaper,
        (N::PrimaryDocument | N::Reporting, P::Government | P::StandardsBody) => {
            SourceKindDto::Report
        }
        (N::PrimaryDocument | N::Documentation, _) => SourceKindDto::Documentation,
        (N::Reference | N::Aggregation, P::CommunityPlatform) => SourceKindDto::Wiki,
        (N::Reference, _) if chaptered => SourceKindDto::OnlineBook,
        (N::Reference, _) => SourceKindDto::ReferenceWork,
        (N::Reporting | N::Opinion, _) => SourceKindDto::Article,
        (N::Tutorial, _) => SourceKindDto::Guide,
        (N::Discussion, _) => SourceKindDto::Forum,
        (N::Marketing, _) => SourceKindDto::MarketingPage,
        (N::Aggregation | N::Unknown, _) => SourceKindDto::Page,
    }
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
