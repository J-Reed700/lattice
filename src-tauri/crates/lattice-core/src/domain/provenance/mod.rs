//! Source provenance and trust: who publishes a source, what a claim is
//! about, and whether the evidence a claim rests on is enough for its kind.
//!
//! Trust is a relation between a source and the subject of a claim, not a
//! label on a URL. *Standing* (who publishes, how accountably) is absolute;
//! *authority* (whether that publisher owns the claim's subject) is
//! relational; the *level* combines them by a fixed procedure
//! ([`trust_level`]); the *sufficiency* gate ([`decide`]) compares the levels
//! of a claim's independent supports with the bar for its kind; contrary
//! evidence is weighed by [`resolve`]; and [`overturn`] is the one rule that
//! may take a supported verdict away. Every function returns its reasons, so
//! the user sees which rule fired and can override it.
//!
//! Pure: no I/O, no clock, no global state. The model judgements that feed
//! this module live in `application::services::source_trust`; storage is
//! `application::ports::source_trust::SourceTrustStore`.
//!
//! Storage forms: enums that are table columns spell their serde name
//! ([`TrustLevel::as_str`] and friends); structured values are stored as
//! their serde JSON.
//!
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §3, §5-§13.

/// `as_str`, `from_column` and `ALL` for a fieldless enum whose serde name is
/// also its column value. A unit test per enum checks the two spellings agree.
macro_rules! wire_names {
    ($ty:ident { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $ty {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            /// The column value, as the serde form spells it.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($ty::$variant => $name),+
                }
            }

            /// The variant a column value names.
            pub fn from_column(value: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|variant| variant.as_str() == value)
            }
        }
    };
}

mod authority;
mod calibration;
mod chain;
mod claim_profile;
mod contradiction;
mod independence;
mod judgement;
mod origin;
mod overrides;
mod overturn;
mod policy;
mod reason;
mod record;
mod signals;
mod standing;
mod sufficiency;
mod temporal;

pub use authority::{AuthorityAssessment, InheritedTrust, SubjectRelation, Trust, TrustLevel};
pub use calibration::{
    BarBasis, Calibration, CalibrationCell, EffectiveBar, EffectiveBars, LabelSource, LevelBar,
    Outcome, OutcomeRecord, Posterior,
};
pub use chain::{CitationCandidate, LinkStanding};
pub use claim_profile::{
    ClaimKind, ClaimProfile, ClaimSubject, SubjectClass, SubjectKey, SubjectKind, TemporalScope,
};
pub use contradiction::{Contrary, Resolution};
pub use independence::{
    Cluster, ClusterId, RelationEvidence, RelationKind, RelationTarget, ShingleSet, Sketch,
    SourceRelation,
};
pub use judgement::{Judged, JudgementKey};
pub use origin::{ArchivedFrom, Origin, OriginKey, RegistryKind, TldRegistry};
pub use overrides::{OverrideRef, OverrideScope, Overrides, SiteStanding, TrustOverride};
pub use overturn::Overturn;
pub use policy::{Policy, TRUST_POLICY_VERSION};
pub use reason::{AddedBy, Reason, ReasonCode};
pub use record::{ExtractedFrom, ProvenanceRecord};
pub use signals::{
    DateBasis, DeclaredOrigin, DocumentSignals, HttpFacts, NamedEntity, OutboundLink, PageSignals,
    PastedSignals, ProvenanceSignals, SourceDates, PAGE_SIGNALS_VERSION,
};
pub use standing::{
    ContentNature, EditorialProcess, PublisherKind, RiskFlag, Standing, StandingAssessment,
    TrackRecord,
};
pub use sufficiency::{
    Bar, CountedSupport, Decision, Label, Missing, NextStep, PassageKind, StaticBars, Support,
};
pub use temporal::{EffectiveDate, TemporalValidity};
