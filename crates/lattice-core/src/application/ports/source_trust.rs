//! Where source provenance and trust are kept (§16).
//!
//! The learning feature implements this over the `source_provenance`,
//! `source_standing_assessments`, `source_subject_authority`,
//! `claim_profiles`, `source_relations`, `source_trust_overrides` and
//! `source_trust_outcomes` tables. Capture writes a version's provenance row
//! and its relations inside the version's own transaction (the learning
//! feature's capture hook); everything else goes through this port. Methods
//! are program-scoped where the table is.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §16, §17.

use async_trait::async_trait;

use crate::domain::provenance::{
    AuthorityAssessment, ClaimProfile, JudgementKey, OriginKey, OutcomeRecord, OverrideScope,
    Overrides, ProvenanceRecord, SourceRelation, StandingAssessment, SubjectKey, TrackRecord,
    TrustOverride,
};
use crate::shared::error::Result;

/// A judgement as stored: its key, its value and what the call cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredJudgement<T> {
    pub key: JudgementKey,
    pub value: T,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub assessed_at: i64,
}

#[async_trait]
pub trait SourceTrustStore: Send + Sync {
    /// One version's provenance row, if capture recorded one.
    async fn provenance(&self, source_version_id: &str) -> Result<Option<ProvenanceRecord>>;

    /// Every provenance row of a course's active source versions.
    async fn program_provenance(&self, program_id: &str) -> Result<Vec<ProvenanceRecord>>;

    /// Creates or replaces a version's provenance row (a re-extraction).
    async fn save_provenance(&self, record: &ProvenanceRecord) -> Result<()>;

    /// The standing judgement for `key` (`input_sha256` is the signals'
    /// sha256), shared by every source with identical signals.
    async fn standing(
        &self,
        key: &JudgementKey,
    ) -> Result<Option<StoredJudgement<StandingAssessment>>>;

    async fn save_standing(&self, judgement: &StoredJudgement<StandingAssessment>) -> Result<()>;

    /// The standing judgements stored for `keys`, in key order; a key with
    /// no row is skipped (claim sources design §7.3).
    async fn standings(&self, keys: &[JudgementKey]) -> Result<Vec<StandingAssessment>>;

    /// The ownership judgement for (origin, subject) in a course under a
    /// policy and model.
    async fn authority(
        &self,
        program_id: &str,
        origin_key: &OriginKey,
        subject_key: &SubjectKey,
        policy: &str,
        model: &str,
    ) -> Result<Option<StoredJudgement<AuthorityAssessment>>>;

    async fn save_authority(
        &self,
        program_id: &str,
        judgement: &StoredJudgement<AuthorityAssessment>,
    ) -> Result<()>;

    /// Every ownership judgement stored for a subject in a course under a
    /// policy and model, whatever the origin: where the subject's owner is
    /// believed to publish (claim sources design §7.3).
    async fn authority_for_subject(
        &self,
        program_id: &str,
        subject_key: &SubjectKey,
        policy: &str,
        model: &str,
    ) -> Result<Vec<AuthorityAssessment>>;

    /// The profile for `key` (`input_sha256` is the claim's profile input
    /// hash, `claim_profile_key`).
    async fn claim_profile(
        &self,
        key: &JudgementKey,
    ) -> Result<Option<StoredJudgement<ClaimProfile>>>;

    /// Writes one row per claim of a batched profile call.
    async fn save_claim_profiles(&self, profiles: &[StoredJudgement<ClaimProfile>]) -> Result<()>;

    /// Every relation recorded in a course.
    async fn relations(&self, program_id: &str) -> Result<Vec<SourceRelation>>;

    /// Adds relations; an edge already recorded (`from`, `kind`, `to`) is
    /// replaced with the new evidence.
    async fn save_relations(&self, program_id: &str, relations: &[SourceRelation]) -> Result<()>;

    /// The overrides in force for a course: its source overrides and every
    /// site and site-for-subject override.
    async fn overrides(&self, program_id: &str) -> Result<Overrides>;

    /// Every site and site-for-subject override, for Settings.
    async fn site_overrides(&self) -> Result<Vec<TrustOverride>>;

    /// Creates or replaces the override at its scope.
    async fn set_override(&self, trust_override: &TrustOverride) -> Result<()>;

    /// Removes the override at `scope`; `false` when there was none.
    async fn clear_override(&self, scope: &OverrideScope) -> Result<bool>;

    async fn record_outcome(&self, outcome: &OutcomeRecord) -> Result<()>;

    /// Every outcome, oldest first: calibration is global.
    async fn outcomes(&self) -> Result<Vec<OutcomeRecord>>;

    /// The verdicts on claims an origin supported.
    async fn track_record(&self, origin_key: &OriginKey) -> Result<TrackRecord>;
}
