//! Source provenance and trust as the Sources tab, the evidence panel and the
//! override menu read them (§18.9-10, §19 of the provenance design). Types
//! only: `references::trust` maps lattice-core's provenance domain onto them.
use super::*;

/// How far a source can be relied on for a claim's subject, highest first.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningTrustLevel {
    Primary,
    Authoritative,
    Established,
    Community,
    Unknown,
    Distrusted,
}

/// The standings a site override may set (never a level: a site is primary
/// only for a subject class).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSiteStanding {
    Institutional,
    Identified,
    Community,
    Distrusted,
}

/// One rule that fired, shown verbatim.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustReasonDto {
    pub code: String,
    pub text: String,
}

/// A subject class as the user named it ("Web platform APIs").
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSubjectClassDto {
    pub label: String,
    pub key: String,
}

/// Where an override applies.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum LearningTrustOverrideScopeDto {
    Source {
        #[serde(rename = "programId")]
        program_id: String,
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Site {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
    },
    SiteSubject {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
        class: LearningSubjectClassDto,
    },
}

/// One override as the user set it; each scope carries only what it may
/// set.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum LearningTrustOverrideDto {
    Source {
        #[serde(rename = "programId")]
        program_id: String,
        #[serde(rename = "sourceId")]
        source_id: String,
        level: LearningTrustLevel,
        note: Option<String>,
        #[serde(rename = "setAt")]
        set_at: i64,
    },
    Site {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
        standing: LearningSiteStanding,
        note: Option<String>,
        #[serde(rename = "setAt")]
        set_at: i64,
    },
    SiteSubject {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
        class: LearningSubjectClassDto,
        level: LearningTrustLevel,
        note: Option<String>,
        #[serde(rename = "setAt")]
        set_at: i64,
    },
}

/// A source version's level for one subject, with the rules that set it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceTrustDto {
    pub source_version_id: String,
    pub level: LearningTrustLevel,
    /// The subject the level is relative to; `None` for a level that holds
    /// whatever the subject.
    pub subject: Option<String>,
    pub reasons: Vec<LearningTrustReasonDto>,
    pub override_scope: Option<LearningTrustOverrideScopeDto>,
}

/// An edge of the provenance graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceRelationKind {
    CopyOf,
    ContainsCopyOf,
    ArchiveOf,
    CanonicalOf,
    Cites,
    CitedForClaim,
}

/// One relation: "mirror of …", "archived copy of …", "cites …".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceRelationDto {
    pub kind: LearningSourceRelationKind,
    pub from_version_id: String,
    /// The captured version it points at, or else the URL.
    pub to_version_id: Option<String>,
    pub to_url: Option<String>,
}

/// What capture recorded about one source version.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceProvenanceDto {
    pub source_version_id: String,
    pub origin_key: String,
    /// The registrable domain (or host) site overrides name.
    pub site: Option<String>,
    pub publisher: Option<String>,
    pub author: Option<String>,
    pub site_name: Option<String>,
    /// Unix seconds.
    pub published_at: Option<i64>,
    pub modified_at: Option<i64>,
    /// The original address when the page was an archived copy.
    pub archived_from: Option<String>,
    /// Why nothing was extracted, when nothing was.
    pub not_extracted: Option<String>,
    pub relations: Vec<LearningSourceRelationDto>,
}

/// What an override change moves: the versions it covers, so the menu can
/// say how many claims will be re-decided.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustChangeDto {
    pub scope: LearningTrustOverrideScopeDto,
    pub source_version_ids: Vec<String>,
}
