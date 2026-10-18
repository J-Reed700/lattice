//! A course's sources as the Sources tab, the override menu and a claim card
//! read their trust (§19): each version's level with its reasons, its
//! publisher's standing, what capture recorded, its relations, its origin's
//! track record and the overrides that can touch it. For a claim, each
//! support is levelled for the claim's subject, beside the bar for its kind.
//!
//! Reads judgements already made and never calls a model: a version whose
//! standing nobody has judged says so, and "Assess" judges it
//! (`judgements::reassess_source`). Each judgement read is the latest made
//! for its input, whichever model made it. The level follows lattice-core's
//! `trust_level` with the same inputs the gate gives it, so the Sources tab
//! and a claim card show what the gate decided on.

use super::calibration;
use super::dto_map;
use super::repository::SqliteSourceTrustStore;
use super::rows::from_json;
use crate::features::learning::dto::{
    GetLearningSourceTrustRequestDto, LearningClaimTrustDto, LearningSourceTrustDto,
    LearningSourceTrustEntryDto, LearningSourceTrustViewDto, LearningTrackRecordDto,
    LearningTrustClaimRequestDto, LearningTrustLevel, LearningTrustReasonDto,
};
use crate::features::learning::persistence::db;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::application::services::source_trust::{
    claim_profile_key, CLAIM_PROFILE_POLICY, OWNERSHIP_POLICY, STANDING_POLICY,
};
use lattice_core::domain::provenance::{
    containment, known_classes, shingles, trust_level, AddedBy, AuthorityAssessment, ClaimProfile,
    ClaimSubject, InheritedTrust, LevelInputs, OriginKey, Overrides, ProvenanceRecord,
    RelationEvidence, RelationKind, RelationTarget, SourceRelation, StandingAssessment, SubjectKey,
    TrackRecord, TrustOverride, POLICY,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
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

/// One version as the view reads it.
#[derive(Debug, Clone)]
struct Version {
    source_id: String,
    version_id: String,
    added_by: AddedBy,
}

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
    latest(
        pool,
        "SELECT profile_json AS judgement FROM claim_profiles WHERE input_sha256=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[&key.input_sha256, CLAIM_PROFILE_POLICY],
        "profile_json",
    )
    .await
}

async fn latest_standing(
    pool: &SqlitePool,
    signals_sha256: &str,
) -> Result<Option<StandingAssessment>> {
    latest(
        pool,
        "SELECT judgement_json AS judgement FROM source_standing_assessments WHERE signals_sha256=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[signals_sha256, STANDING_POLICY],
        "judgement_json",
    )
    .await
}

async fn latest_authority(
    pool: &SqlitePool,
    program_id: &str,
    origin: &OriginKey,
    subject: &SubjectKey,
) -> Result<Option<AuthorityAssessment>> {
    latest(
        pool,
        "SELECT judgement_json AS judgement FROM source_subject_authority WHERE program_id=? AND origin_key=? AND subject_key=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[program_id, &origin.0, &subject.0, OWNERSHIP_POLICY],
        "judgement_json",
    )
    .await
}

async fn latest<T: serde::de::DeserializeOwned>(
    pool: &SqlitePool,
    sql: &str,
    binds: &[&str],
    column: &str,
) -> Result<Option<T>> {
    let mut query = sqlx::query(sql);
    for value in binds {
        query = query.bind(*value);
    }
    query
        .fetch_optional(pool)
        .await
        .map_err(db)?
        .map(|row| from_json(&row.get::<String, _>("judgement"), column))
        .transpose()
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
        let rows = sqlx::query(
            "SELECT id AS source_id, active_version_id AS version_id, added_by FROM learning_source_library \
             WHERE program_id=? AND deleted_at IS NULL AND active_version_id IS NOT NULL ORDER BY created_at, id",
        )
        .bind(self.program_id)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter()
            .map(version)
            .filter(|row| {
                row.as_ref().map_or(true, |row| {
                    source_ids.is_empty() || source_ids.contains(&row.source_id)
                })
            })
            .collect()
    }

    async fn version(&self, version_id: &str) -> Result<Option<Version>> {
        sqlx::query(
            "SELECT v.source_id, v.id AS version_id, l.added_by FROM learning_source_versions v \
             JOIN learning_source_library l ON l.id = v.source_id AND l.program_id = v.program_id \
             WHERE v.program_id=? AND v.id=?",
        )
        .bind(self.program_id)
        .bind(version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db)?
        .as_ref()
        .map(version)
        .transpose()
    }

    async fn text(&self, version_id: &str) -> Result<String> {
        sqlx::query_scalar(
            "SELECT full_text FROM learning_source_versions WHERE program_id=? AND id=?",
        )
        .bind(self.program_id)
        .bind(version_id)
        .fetch_one(self.pool())
        .await
        .map_err(db)
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
            Some(record) => latest_standing(self.pool(), &record.signals_sha256).await?,
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
            let Some(standing) = latest_standing(self.pool(), &record.signals_sha256).await? else {
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

fn version(row: &sqlx::sqlite::SqliteRow) -> Result<Version> {
    let added_by: String = row.try_get("added_by").map_err(db)?;
    Ok(Version {
        source_id: row.try_get("source_id").map_err(db)?,
        version_id: row.try_get("version_id").map_err(db)?,
        added_by: AddedBy::from_column(&added_by).ok_or_else(|| {
            AppError::InternalError(format!("Unknown source origin \"{added_by}\"."))
        })?,
    })
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
