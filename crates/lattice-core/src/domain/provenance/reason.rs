//! Reasons: every level, bar, decision and overturn carries the rules that
//! produced it, each as a closed code (for logic and tests) and a sentence
//! (for the user, shown verbatim).

use serde::{Deserialize, Serialize};

/// One reason: which rule or signal, and what it said here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: ReasonCode,
    pub text: String,
}

impl Reason {
    pub fn new(code: ReasonCode, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }
}

/// Every rule or signal a reason can name. Closed: a judgement that names a
/// code outside the subset its schema offers is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    // Standing judgement (§12.1): each names the signal a conclusion rests on.
    PublisherMetadata,
    AuthorMetadata,
    OriginIdentity,
    RestrictedRegistry,
    DeclaredType,
    EditorialSignal,
    DateSignal,
    LicenseSignal,
    GeneratorSignal,
    NoPublisherMetadata,
    NoAuthorMetadata,
    MetadataAddressesReader,
    SelfDescribedOfficial,
    TrackRecord,
    ContentFarmSignal,
    GeneratedContentSignal,
    DeclaredByUser,

    // Ownership judgement (§12.2).
    PublisherIsOwner,
    DomainOfOwner,
    OwnerCommunityArea,
    OwnerPartner,
    ThirdParty,
    OwnershipUnsettled,

    // Claim profile judgement (§12.3).
    ProfileClassified,

    // Judgement plumbing: what stands in when a judgement could not be made.
    JudgementFailed,
    MetadataTooLarge,
    NoSignals,

    // The level procedure (§5).
    SourceOverride,
    SiteStandingOverride,
    SiteSubjectOverride,
    InjectionDistrusted,
    DistrustedByYou,
    Mirror,
    OwnerOfSubject,
    OwnerHostedCommunity,
    OwnerMarketing,
    InstitutionalEditorial,
    InstitutionalSelfPublished,
    IdentifiedPublisher,
    CommunityPublisher,
    UnknownPublisher,
    ContentNatureCap,
    SelfDeclaredOfficialCap,
    ContentFarmCap,
    GeneratedContentCap,
    AddedByYou,
    NoSubject,

    // Sufficiency (§6).
    ExcludedDistrusted,
    ExcludedInvalidDate,
    ExcludedCannotPlace,
    TestimonialCounted,
    UndatedCountedLower,
    StaleSupport,
    MetAlone,
    MetByCorroboration,
    BelowBar,
    CalibratedBar,
    PrimaryContradicts,
    ContradictionBlocksCorroboration,

    // Temporal validity (§9).
    FutureDateIgnored,
    WeakDateBasis,

    // Contradiction (§10).
    IgnoredDistrusted,
    TemporallyPlaced,
    OutranksByLevel,
    NewerEvidence,
    MoreIndependentOrigins,
    DisputedByLowerStanding,
    Unresolved,

    // Citation chains (§8).
    CitationNotBorneOut,
    CitedPageUnavailable,
}

/// Who added a source: mirrors the learning ledger's `SourceAddedBy`
/// (`domain::learning::claim_review`) without depending on it, so chat can
/// reuse this module. The learning feature converts between the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddedBy {
    User,
    Research,
    Outline,
    Import,
    Verifier,
}

wire_names!(AddedBy {
    User => "user",
    Research => "research",
    Outline => "outline",
    Import => "import",
    Verifier => "verifier",
});
