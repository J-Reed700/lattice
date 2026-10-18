//! Sufficiency (§6): whether a claim's independent supports meet the bar
//! for its kind. A pure function over cached judgements, so it costs
//! nothing to recompute when an override or a relation changes.

use serde::{Deserialize, Serialize};

use super::authority::{Trust, TrustLevel};
use super::claim_profile::ClaimKind;
use super::independence::ClusterId;
use super::reason::{AddedBy, Reason};
use super::signals::SourceDates;
use super::temporal::TemporalValidity;

/// How a passage bears on the claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "passage", rename_all = "snake_case")]
pub enum PassageKind {
    /// The source states the fact itself.
    Direct,
    /// The source quotes another source.
    Quotation {
        #[serde(default)]
        of: Option<String>,
    },
    /// The source reports what someone else says ("X reports that…").
    /// `borne_out` is set once the chain to X was followed (§8).
    Testimonial {
        attributed_to: String,
        #[serde(default)]
        borne_out: Option<bool>,
    },
}

/// One passage the checker used as support, levelled for the claim's
/// subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Support {
    pub source_version_id: String,
    pub origin_cluster: ClusterId,
    pub trust: Trust,
    pub dates: SourceDates,
    pub passage: PassageKind,
    pub temporal: TemporalValidity,
    pub added_by: AddedBy,
    /// Citation candidates in the passage, best first (`citation_candidates`):
    /// what [`NextStep::FollowCitation`] offers.
    #[serde(default)]
    pub cited_urls: Vec<String>,
    /// Owner hints from this support's ownership judgement: what
    /// [`NextStep::SearchOwner`] offers.
    #[serde(default)]
    pub owner_hints: Vec<String>,
}

/// The rule a claim's supports must meet, named for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bar {
    /// The subject's owner (or an authoritative, uncontradicted source).
    Primary,
    /// The measurement's origin.
    Origin,
    Authoritative,
    Established,
    /// An identified source the claim can be attributed to.
    Attributed,
    /// Any identified support.
    Any,
}

/// How an accepted claim must be worded or shown when its support is below
/// Primary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "label", rename_all = "snake_case")]
pub enum Label {
    Practice,
    Attributed,
    CommunityConsensus,
    ReportedBy { attributing_to: String },
    Secondary,
    Mirror,
}

/// A support as the gate counted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountedSupport {
    pub source_version_id: String,
    pub origin_cluster: ClusterId,
    /// The level it counted at after the passage, date and exclusion rules.
    /// `None` when it was excluded.
    #[serde(default)]
    pub counted: Option<TrustLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Sufficient {
        /// The supports that met the bar, best first.
        met_by: Vec<String>,
        #[serde(default)]
        label: Option<Label>,
        reasons: Vec<Reason>,
    },
    Insufficient {
        bar: Bar,
        have: Vec<CountedSupport>,
        missing: Missing,
        next: Vec<NextStep>,
        reasons: Vec<Reason>,
    },
}

/// What the supports lack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "missing", rename_all = "snake_case")]
pub enum Missing {
    NoPrimaryForSubject,
    NoMeasurementOrigin,
    NoIndependentCorroboration {
        have: usize,
    },
    NoIdentifiedSource,
    /// Every support that would meet the bar only reports someone else.
    OnlyTestimonial,
    OnlyStale {
        newer_primary: String,
    },
}

/// What research may do next, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum NextStep {
    FollowCitation { url: String, from: String },
    SearchOwner { origin_hints: Vec<String> },
    SearchIndependent,
    FetchNewer { newer_primary: String },
    AskUser,
}

/// The §6.1 table's "accepted alone" column: per claim kind, the lowest
/// level accepted as the only support. The prior calibration moves (§13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticBars {
    pub entries: Vec<(ClaimKind, TrustLevel)>,
}
