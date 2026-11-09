//! A course's sources as the Sources tab, the override menu and a claim card
//! read their trust (§19): each version's level and reasons, its publisher's
//! standing, relations, track record and overrides. It reads judgements
//! already made and never calls a model, levelling with the same
//! `trust_level` inputs the gate uses.

use super::calibration;
use super::dto_map;
use super::repository;
use super::repository::capture::version_text;
use super::repository::source_view::{latest_claim_profile, Version};
use super::repository::SqliteSourceTrustStore;
use crate::dto::{
    GetLearningSourceTrustRequestDto, LearningClaimTrustDto, LearningSourceTrustDto,
    LearningSourceTrustEntryDto, LearningSourceTrustViewDto, LearningTrackRecordDto,
    LearningTrustClaimRequestDto, LearningTrustLevel, LearningTrustReasonDto,
};
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::application::services::source_trust::{
    claim_profile_key, standing_key, CLAIM_PROFILE_POLICY, OWNERSHIP_POLICY, STANDING_POLICY,
};
use lattice_core::domain::provenance::{
    containment, known_classes, shingles, trust_level, AuthorityAssessment, ClaimProfile,
    ClaimSubject, InheritedTrust, LevelInputs, OriginKey, Overrides, ProvenanceRecord,
    RelationEvidence, RelationKind, RelationTarget, SourceRelation, StandingAssessment, SubjectKey,
    TrackRecord, TrustOverride, POLICY,
};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;
use std::collections::BTreeSet;

/// The relations through which a version inherits a level (§5 step 4), as
/// the gate reads them.
const COPY_RELATIONS: [RelationKind; 4] = [
    RelationKind::CopyOf,
    RelationKind::ContainsCopyOf,
    RelationKind::ArchiveOf,
    RelationKind::CanonicalOf,
];

/// The reason code of a version whose publisher nobody has judged yet. Not
/// a rule of the level procedure, so it is not a core `ReasonCode`.
pub const NOT_ASSESSED: &str = "not_assessed";

/// The course's overrides and relations, read once per request.
struct Course<'a> {
    store: &'a SqliteSourceTrustStore,
    program_id: &'a str,
    overrides: Overrides,
    relations: Vec<SourceRelation>,
}

/// The trust view a request asks for.
pub async fn view(
    store: &SqliteSourceTrustStore,
    request: &GetLearningSourceTrustRequestDto,
) -> Result<LearningSourceTrustViewDto> {
    let course = Course {
        store,
        program_id: &request.program_id,
        overrides: store.overrides(&request.program_id).await?,
        relations: store.relations(&request.program_id).await?,
    };
    let mut sources = Vec::new();
    for version in course.active_versions(&request.source_ids).await? {
        sources.push(course.entry(&version, None).await?);
    }
    let claim = match &request.claim {
        Some(claim) => Some(course.claim(claim).await?),
        None => None,
    };
    Ok(LearningSourceTrustViewDto { sources, claim })
}

/// The latest profile of `statement` under the current class vocabulary.
pub async fn latest_profile(
    pool: &SqlitePool,
    statement: &str,
    known_classes: &[String],
) -> Result<Option<ClaimProfile>> {
    // The input hash does not depend on the model; any model's key has it.
    let key = claim_profile_key(statement, known_classes, "");
    latest_claim_profile(pool, &key.input_sha256, CLAIM_PROFILE_POLICY).await
}

/// The latest `source-standing-v2` judgement of `record`'s version, by any
/// model: its key is computed from the version's text and how the course got
/// it, as the gate computes it (claim sources design §4.3). A version judged
/// only under v1 has none: the view shows it as not judged yet.
pub(crate) async fn latest_standing(
    pool: &SqlitePool,
    program_id: &str,
    record: &ProvenanceRecord,
) -> Result<Option<StandingAssessment>> {
    let Some(content) = super::judgements::content_evidence(pool, program_id, record).await? else {
        return Ok(None);
    };
    // The input hash does not depend on the model; any model's key has it.
    let key = standing_key(&record.signals, &content, "");
    repository::source_view::latest_standing(pool, &key.input_sha256, STANDING_POLICY).await
}

async fn latest_authority(
    pool: &SqlitePool,
    program_id: &str,
    origin: &OriginKey,
    subject: &SubjectKey,
) -> Result<Option<AuthorityAssessment>> {
    repository::source_view::latest_authority(
        pool,
        program_id,
        &origin.0,
        &subject.0,
        OWNERSHIP_POLICY,
    )
    .await
}

fn reason(code: &str, text: &str) -> LearningTrustReasonDto {
    LearningTrustReasonDto {
        code: code.to_owned(),
        text: text.to_owned(),
    }
}

impl Course<'_> {
    fn pool(&self) -> &SqlitePool {
        self.store.pool()
    }

    async fn active_versions(&self, source_ids: &[String]) -> Result<Vec<Version>> {
        Ok(
            repository::source_view::active_versions(self.pool(), self.program_id)
                .await?
                .into_iter()
                .filter(|row| source_ids.is_empty() || source_ids.contains(&row.source_id))
                .collect(),
        )
    }

    async fn version(&self, version_id: &str) -> Result<Option<Version>> {
        repository::source_view::version_of(self.pool(), self.program_id, version_id).await
    }

    async fn text(&self, version_id: &str) -> Result<String> {
        version_text(self.pool(), self.program_id, version_id).await
    }

    async fn claim(&self, claim: &LearningTrustClaimRequestDto) -> Result<LearningClaimTrustDto> {
        let profile = latest_profile(
            self.pool(),
            &claim.statement,
            &known_classes(&self.overrides.entries),
        )
        .await?;
        let subject = profile.as_ref().and_then(ClaimProfile::authority_subject);
        let mut supports = Vec::new();
        let mut seen = BTreeSet::new();
        for version_id in &claim.source_version_ids {
            if !seen.insert(version_id.as_str()) {
                continue;
            }
            if let Some(version) = self.version(version_id).await? {
                supports.push(self.entry(&version, subject.as_ref()).await?);
            }
        }
        let bar = match &profile {
            Some(profile) => {
                let calibrated = calibration::calibrated(self.pool()).await?;
                calibrated
                    .bars
                    .bar(profile.kind)
                    .map(|bar| dto_map::kind_bar(bar, &calibrated.calibration, &POLICY))
            }
            None => None,
        };
        Ok(LearningClaimTrustDto {
            profile: profile.as_ref().map(dto_map::profile),
            supports,
            bar,
        })
    }

    async fn entry(
        &self,
        version: &Version,
        subject: Option<&ClaimSubject>,
    ) -> Result<LearningSourceTrustEntryDto> {
        let record = self.store.provenance(&version.version_id).await?;
        let site = record
            .as_ref()
            .and_then(|record| record.origin.site().map(str::to_owned));
        let track = match &record {
            Some(record) => self.store.track_record(&record.origin.key).await?,
            None => TrackRecord::default(),
        };
        let standing = match &record {
            Some(record) => latest_standing(self.pool(), self.program_id, record).await?,
            None => None,
        };
        let trust = match (&record, &standing) {
            (Some(record), Some(standing)) => {
                let trust = self.level(version, record, standing, subject, true).await?;
                LearningSourceTrustDto {
                    source_version_id: version.version_id.clone(),
                    level: dto_map::level(trust.level),
                    subject: subject.map(|subject| subject.name.clone()),
                    reasons: dto_map::reasons(&trust.reasons),
                    override_scope: trust.override_.map(|by| dto_map::scope(&by.scope)),
                }
            }
            (None, _) => unlevelled(
                version,
                reason(
                    "no_signals",
                    "No provenance was recorded when this source was saved; refresh it to record its publisher",
                ),
            ),
            (Some(_), None) => unlevelled(
                version,
                reason(
                    NOT_ASSESSED,
                    "Its publisher has not been judged yet; Assess judges it with your Learning model",
                ),
            ),
        };
        Ok(LearningSourceTrustEntryDto {
            source_id: version.source_id.clone(),
            source_version_id: version.version_id.clone(),
            trust,
            standing: standing
                .as_ref()
                .map(|standing| dto_map::standing(standing.standing)),
            standing_reasons: standing
                .as_ref()
                .map(|standing| dto_map::reasons(&standing.reasons))
                .unwrap_or_default(),
            provenance: record
                .as_ref()
                .map(|record| dto_map::provenance(record, &self.relations)),
            track_record: LearningTrackRecordDto {
                right: track.right,
                wrong: track.wrong,
            },
            overrides: self
                .overrides
                .entries
                .iter()
                .filter(|entry| touches(entry, &version.source_id, site.as_deref()))
                .map(dto_map::trust_override)
                .collect(),
            // What an override would re-decide is the claim ledger's
            // (`claim_reviews::trust_overrides::view` fills it).
            impact: None,
        })
    }

    /// The version's level for `subject`, inheriting through a copy relation
    /// when `inherit` (one hop, as the gate does).
    async fn level(
        &self,
        version: &Version,
        record: &ProvenanceRecord,
        standing: &StandingAssessment,
        subject: Option<&ClaimSubject>,
        inherit: bool,
    ) -> Result<lattice_core::domain::provenance::Trust> {
        let relation = match subject {
            Some(subject) if record.origin.host.is_some() => {
                latest_authority(
                    self.pool(),
                    self.program_id,
                    &record.origin.key,
                    &subject.key,
                )
                .await?
            }
            _ => None,
        };
        let inherited = if inherit {
            self.inherited(version, subject).await?
        } else {
            Vec::new()
        };
        Ok(trust_level(&LevelInputs {
            program_id: self.program_id,
            source_id: &version.source_id,
            origin: &record.origin,
            signals: &record.signals,
            standing,
            subject,
            relation: relation.as_ref(),
            inherited: &inherited,
            overrides: &self.overrides,
            added_by: version.added_by,
            policy: &POLICY,
        }))
    }

    async fn inherited(
        &self,
        version: &Version,
        subject: Option<&ClaimSubject>,
    ) -> Result<Vec<InheritedTrust>> {
        let mut inherited = Vec::new();
        for edge in self
            .relations
            .iter()
            .filter(|edge| edge.from == version.version_id && COPY_RELATIONS.contains(&edge.kind))
        {
            let RelationTarget::Version(original) = &edge.to else {
                continue;
            };
            let Some(source) = self.version(original).await? else {
                continue;
            };
            let Some(record) = self.store.provenance(original).await? else {
                continue;
            };
            let Some(standing) = latest_standing(self.pool(), self.program_id, &record).await?
            else {
                continue;
            };
            let own = Box::pin(self.level(&source, &record, &standing, subject, false)).await?;
            let contained = match edge.evidence {
                RelationEvidence::Text { containment, .. } => containment,
                _ => containment(
                    &shingles(&self.text(original).await?, &POLICY),
                    &shingles(&self.text(&version.version_id).await?, &POLICY),
                ),
            };
            inherited.push(InheritedTrust {
                of: original.clone(),
                of_origin: record.origin.key.clone(),
                relation: edge.kind,
                level: own.level,
                containment: contained,
            });
        }
        Ok(inherited)
    }
}

/// A version the level procedure cannot run on yet: Unknown, with why.
fn unlevelled(version: &Version, why: LearningTrustReasonDto) -> LearningSourceTrustDto {
    LearningSourceTrustDto {
        source_version_id: version.version_id.clone(),
        level: LearningTrustLevel::Unknown,
        subject: None,
        reasons: vec![why],
        override_scope: None,
    }
}

/// Whether `entry` can apply to a source: its own source override, or any
/// override of its site.
fn touches(entry: &TrustOverride, source_id: &str, site: Option<&str>) -> bool {
    match entry {
        TrustOverride::Source {
            source_id: overridden,
            ..
        } => overridden == source_id,
        TrustOverride::Site {
            registrable_domain, ..
        }
        | TrustOverride::SiteSubject {
            registrable_domain, ..
        } => site == Some(registrable_domain.as_str()),
    }
}
