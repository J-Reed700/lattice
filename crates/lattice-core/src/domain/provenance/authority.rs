//! Authority and level: whether a publisher owns a claim's subject (§12.2),
//! and the level that standing, ownership, relations and overrides combine
//! into (§5).

use serde::{Deserialize, Serialize};

use super::claim_profile::SubjectKey;
use super::independence::RelationKind;
use super::origin::OriginKey;
use super::overrides::OverrideRef;
use super::reason::Reason;

/// How an origin stands to a subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectRelation {
    /// The origin publishes on its own behalf about its own subject.
    Owner,
    /// The owner's origin, but a community area, a reseller or a partner.
    Affiliated,
    Independent,
    Unknown,
}

wire_names!(SubjectRelation {
    Owner => "owner",
    Affiliated => "affiliated",
    Independent => "independent",
    Unknown => "unknown",
});

/// Judgement #2's validated output, per (origin, subject).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityAssessment {
    pub origin_key: OriginKey,
    pub subject_key: SubjectKey,
    pub relation: SubjectRelation,
    pub basis: Vec<Reason>,
    /// Hostnames where the owner most likely publishes, validated
    /// syntactically: beliefs used as `site:` scopes for searching, never
    /// facts.
    #[serde(default)]
    pub owner_origin_hints: Vec<String>,
    pub policy: String,
    pub model: String,
}

/// How far a source can be relied on for one subject. Declared lowest first
/// so `Ord` reads naturally: `Primary > Authoritative > … > Distrusted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Distrusted,
    Unknown,
    Community,
    Established,
    Authoritative,
    /// The subject's owner, or the measurement's origin: a relation to a
    /// subject, never a property of a site alone.
    Primary,
}

wire_names!(TrustLevel {
    Distrusted => "distrusted",
    Unknown => "unknown",
    Community => "community",
    Established => "established",
    Authoritative => "authoritative",
    Primary => "primary",
});

/// A level with the rules that produced it and the override, if one fixed
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trust {
    pub level: TrustLevel,
    pub reasons: Vec<Reason>,
    #[serde(rename = "override", default)]
    pub override_: Option<OverrideRef>,
}

/// A level a version may inherit through a content-identity relation (§5
/// step 4): an archived copy of a primary is primary; a verified mirror of
/// an official text is what the text is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InheritedTrust {
    /// The version whose level is inherited.
    pub of: String,
    /// Its origin, for the reason text.
    pub of_origin: OriginKey,
    pub relation: RelationKind,
    /// That version's level for the same subject.
    pub level: TrustLevel,
    /// How much of the original's text the inheriting version contains.
    pub containment: f64,
}
