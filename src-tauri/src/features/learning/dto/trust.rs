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

/// Whether a verified claim's support met the bar for its kind (§6, §18.9):
/// the claim's kind and subject, the bar, the label it is shown with when
/// its support is below primary, and the rules that decided.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSufficiencyDto {
    /// `specification`, `measurement`, `version_bound`, `definition`,
    /// `practice`, `opinion`, `historical` or `general`.
    pub claim_kind: String,
    pub subject: Option<String>,
    pub sufficient: bool,
    /// The bar an insufficient support failed (`primary`, `origin`,
    /// `authoritative`, `established`, `attributed`, `any`).
    pub bar: Option<String>,
    /// How a sufficient claim is shown when its support is below primary
    /// (`practice`, `attributed`, `community_consensus`, `reported_by`,
    /// `secondary`, `mirror`).
    pub label: Option<String>,
    pub reasons: Vec<LearningTrustReasonDto>,
}

/// A publisher's judged standing (§12.1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceStanding {
    Institutional,
    Identified,
    Community,
    Unknown,
}

/// An origin's record on the user's verdicts (§13): claims it supported
/// that held, and that were overturned. Shown, never a level by itself.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrackRecordDto {
    pub right: u32,
    pub wrong: u32,
}

/// The claims an override would send back to be re-decided: those waiting
/// in review that cite a version it covers (§19).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustImpactDto {
    pub versions: u32,
    pub claims: u32,
    pub lessons: u32,
}

/// What each override scope would reach for one source: the source alone
/// (this course), or its whole site (every course; with or without a
/// subject class, a site override covers the same versions).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustImpactsDto {
    pub source: LearningTrustImpactDto,
    /// `None` for a source with no site (a file, pasted text).
    pub site: Option<LearningTrustImpactDto>,
}

/// One source's trust as the Sources tab, its override menu and a claim
/// card show it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceTrustEntryDto {
    pub source_id: String,
    pub source_version_id: String,
    /// The level whatever the subject in the Sources tab; for the claim's
    /// subject on a claim card.
    pub trust: LearningSourceTrustDto,
    /// `None` until the publisher's standing has been judged.
    pub standing: Option<LearningSourceStanding>,
    /// The standing judgement's own reasons, verbatim.
    pub standing_reasons: Vec<LearningTrustReasonDto>,
    /// `None` when capture recorded no provenance for the version.
    pub provenance: Option<LearningSourceProvenanceDto>,
    pub track_record: LearningTrackRecordDto,
    /// Every override that can touch this source: its own, its site's
    /// standing, and its site's per-class levels.
    pub overrides: Vec<LearningTrustOverrideDto>,
    /// Filled only when the request asks for it (the override menu).
    pub impact: Option<LearningTrustImpactsDto>,
}

/// The subject a claim is about, with what a site-for-subject override can
/// be scoped to: the subject, its aliases and its broader classes.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimSubjectDto {
    pub name: String,
    pub kind: String,
    pub aliases: Vec<String>,
    /// Broader classes, most specific first.
    pub classes: Vec<String>,
}

/// A claim's profile (§12.3): its kind and subject.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimProfileDto {
    /// `specification`, `measurement`, `version_bound`, `definition`,
    /// `practice`, `opinion`, `historical` or `general`.
    pub claim_kind: String,
    pub subject: Option<LearningClaimSubjectDto>,
    pub measurement_origin: Option<String>,
    /// Empty for a judged profile; why it is the `general` fallback
    /// otherwise.
    pub reasons: Vec<LearningTrustReasonDto>,
}

/// Why a level is or is not accepted alone for a kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningBarBasis {
    /// The design's table, unmoved.
    Static,
    /// Below the table's bar, accepted alone because its claims held.
    Relaxed,
    /// At or above the table's bar, no longer accepted alone.
    Tightened,
}

/// One level under one kind's bar, with its verdicts.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLevelBarDto {
    pub level: LearningTrustLevel,
    pub accepted_alone: bool,
    pub basis: LearningBarBasis,
    /// Claims at this level and kind that held, and that did not.
    pub held: u32,
    pub failed: u32,
    /// `P(reliability ≥ accept_reliability)` under the cell's posterior.
    pub probability_reliable: f64,
    /// When a moved bar's current run began (unix seconds).
    pub since: Option<i64>,
}

/// One claim kind's bar in force (§13).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningKindBarDto {
    pub claim_kind: String,
    /// The lowest level the design's table accepts alone.
    pub static_lowest: LearningTrustLevel,
    /// The lowest level accepted alone now; `None` when only corroboration
    /// meets the bar.
    pub lowest_accepted: Option<LearningTrustLevel>,
    pub levels: Vec<LearningLevelBarDto>,
    /// Each automatic change, with its numbers.
    pub reasons: Vec<LearningTrustReasonDto>,
}

/// A claim's trust as its review card shows it: the profile, each support
/// levelled for the claim's subject, and the bar for its kind.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimTrustDto {
    /// `None` until the claim is profiled under the current subject classes.
    pub profile: Option<LearningClaimProfileDto>,
    pub supports: Vec<LearningSourceTrustEntryDto>,
    pub bar: Option<LearningKindBarDto>,
}

/// The claim a trust read is for: its statement and the versions it cites.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustClaimRequestDto {
    pub statement: String,
    pub source_version_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLearningSourceTrustRequestDto {
    pub program_id: String,
    /// These sources only; empty for every active source of the course.
    pub source_ids: Vec<String>,
    /// Also level these supports for one claim's subject.
    pub claim: Option<LearningTrustClaimRequestDto>,
    /// Count what each override scope would reopen.
    pub include_impact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceTrustViewDto {
    pub sources: Vec<LearningSourceTrustEntryDto>,
    pub claim: Option<LearningClaimTrustDto>,
}

/// An override as the user sets it. A site override sets a standing; only
/// a source, or a site for a subject class, may set a level (Primary
/// included).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum LearningTrustOverrideInputDto {
    Source {
        #[serde(rename = "programId")]
        program_id: String,
        #[serde(rename = "sourceId")]
        source_id: String,
        level: LearningTrustLevel,
        note: Option<String>,
    },
    Site {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
        standing: LearningSiteStanding,
        note: Option<String>,
    },
    SiteSubject {
        #[serde(rename = "registrableDomain")]
        registrable_domain: String,
        /// The class as the user picked it: the claim's subject, an alias
        /// or a broader class.
        #[serde(rename = "classLabel")]
        class_label: String,
        level: LearningTrustLevel,
        note: Option<String>,
    },
}

/// An override change and what it sends back to be re-decided.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustChangeResultDto {
    pub change: LearningTrustChangeDto,
    pub impact: LearningTrustImpactDto,
}

/// A site override with what its sources' claims did since (§13: "you set
/// example.org to institutional; 2 of 9 claims it supported were
/// overturned").
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSiteTrustOverrideDto {
    pub trust_override: LearningTrustOverrideDto,
    pub track_record: LearningTrackRecordDto,
}

/// One stated policy parameter and its derivation (`policy.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTrustPolicyParameterDto {
    pub name: String,
    pub value: f64,
    pub derivation: String,
}

/// The bars in force and why (§13), for Settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceTrustCalibrationDto {
    pub policy_version: String,
    pub parameters: Vec<LearningTrustPolicyParameterDto>,
    pub kinds: Vec<LearningKindBarDto>,
    /// Verdicts recorded across every course.
    pub verdicts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReassessLearningSourceTrustRequestDto {
    pub program_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReprofileLearningClaimRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    pub claim_id: String,
}

/// Where the provenance eval lines were written.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportedProvenanceEvalExamplesDto {
    pub path: String,
    pub examples: u32,
}
