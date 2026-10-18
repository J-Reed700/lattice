//! Standing: who publishes a source and how accountable that publishing is
//! (§5, §12.1). Absolute: it does not depend on any claim.

use serde::{Deserialize, Serialize};

use super::reason::Reason;
use super::signals::NamedEntity;

/// How identifiable and accountable the publisher is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// An identified organisation that publishes on its own behalf.
    Institutional,
    /// An identified person or small publisher.
    Identified,
    /// A community platform: forum, wiki, Q&A, comments.
    Community,
    Unknown,
}

wire_names!(Standing {
    Institutional => "institutional",
    Identified => "identified",
    Community => "community",
    Unknown => "unknown",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherKind {
    Government,
    StandardsBody,
    Academic,
    Vendor,
    Publisher,
    News,
    Individual,
    CommunityPlatform,
    Aggregator,
    Unknown,
}

wire_names!(PublisherKind {
    Government => "government",
    StandardsBody => "standards_body",
    Academic => "academic",
    Vendor => "vendor",
    Publisher => "publisher",
    News => "news",
    Individual => "individual",
    CommunityPlatform => "community_platform",
    Aggregator => "aggregator",
    Unknown => "unknown",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorialProcess {
    PeerReviewed,
    Editorial,
    SelfPublished,
    CommunityEdited,
    None,
    Unknown,
}

wire_names!(EditorialProcess {
    PeerReviewed => "peer_reviewed",
    Editorial => "editorial",
    SelfPublished => "self_published",
    CommunityEdited => "community_edited",
    None => "none",
    Unknown => "unknown",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentNature {
    PrimaryDocument,
    Documentation,
    Reference,
    Reporting,
    Tutorial,
    Opinion,
    Discussion,
    Marketing,
    Aggregation,
}

wire_names!(ContentNature {
    PrimaryDocument => "primary_document",
    Documentation => "documentation",
    Reference => "reference",
    Reporting => "reporting",
    Tutorial => "tutorial",
    Opinion => "opinion",
    Discussion => "discussion",
    Marketing => "marketing",
    Aggregation => "aggregation",
});

/// A concrete risk signal. Each lowers or blocks the level (§5 steps 3, 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskFlag {
    /// The page calls itself official, verified or authoritative. Computed
    /// deterministically from `self_description`; harmless only for the
    /// subject's owner.
    SelfDeclaredOfficial,
    /// A metadata field addresses a reader or a model.
    InjectionAttempt,
    ContentFarmPattern,
    GeneratedContentPattern,
    NoIdentifiableAuthor,
    Undated,
    /// The metadata was too large for the judgement's window.
    OversizedMetadata,
    /// A page cited a source for a claim that the source does not bear out
    /// (§8); counted per origin.
    CitationNotBorneOut,
}

wire_names!(RiskFlag {
    SelfDeclaredOfficial => "self_declared_official",
    InjectionAttempt => "injection_attempt",
    ContentFarmPattern => "content_farm_pattern",
    GeneratedContentPattern => "generated_content_pattern",
    NoIdentifiableAuthor => "no_identifiable_author",
    Undated => "undated",
    OversizedMetadata => "oversized_metadata",
    CitationNotBorneOut => "citation_not_borne_out",
});

/// The user's verdicts on claims an origin supported (§13): shown on the
/// badge and passed to the standing judgement as a signal, never a level by
/// itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackRecord {
    pub right: u32,
    pub wrong: u32,
}

/// Judgement #1's validated output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingAssessment {
    pub standing: Standing,
    #[serde(default)]
    pub publisher: Option<NamedEntity>,
    pub publisher_kind: PublisherKind,
    pub editorial: EditorialProcess,
    pub nature: ContentNature,
    #[serde(default)]
    pub flags: Vec<RiskFlag>,
    /// Non-empty: every conclusion names the signal it rests on.
    pub reasons: Vec<Reason>,
    pub policy: String,
    pub model: String,
    /// The cache key: sha256 of the canonical signals JSON.
    pub signals_sha256: String,
    /// The origin's track record the judgement saw. Not part of the key: a
    /// new verdict does not re-judge every page of the origin; "Reassess"
    /// does.
    #[serde(default)]
    pub track_record: TrackRecord,
}
