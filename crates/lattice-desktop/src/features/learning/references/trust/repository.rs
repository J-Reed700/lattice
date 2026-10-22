//! The trust store over the pool (§16): lattice-core's `SourceTrustStore`,
//! built with the pool the way the other learning repositories are, so no
//! dependency-injection module changes.

use super::rows::{
    delete_override, from_json, override_from_row, provenance_from_row, relation_from_row, to_json,
    tokens_from_column, tokens_to_column, unknown, write_override, write_provenance,
    write_relations, OVERRIDE_COLUMNS, PROVENANCE_COLUMNS, RELATION_COLUMNS,
};
use crate::features::learning::persistence::db;
use async_trait::async_trait;
use lattice_core::application::ports::source_trust::{SourceTrustStore, StoredJudgement};
use lattice_core::domain::provenance::{
    AuthorityAssessment, ClaimKind, ClaimProfile, JudgementKey, LabelSource, OriginKey, Outcome,
    OutcomeRecord, OverrideScope, Overrides, ProvenanceRecord, SourceRelation, StandingAssessment,
    SubjectKey, TrackRecord, TrustLevel, TrustOverride,
};
use lattice_core::shared::error::Result;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

/// Unix seconds now: the clock the trust tables keep, like the domain's
/// dates and outcomes.
pub(in crate::features::learning) fn unix_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Clone)]
pub struct SqliteSourceTrustStore {
    pool: SqlitePool,
}

impl SqliteSourceTrustStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(in crate::features::learning) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn judgement<T: serde::de::DeserializeOwned>(
    row: &SqliteRow,
    input_column: &str,
    value_column: &str,
) -> Result<StoredJudgement<T>> {
    Ok(StoredJudgement {
        key: JudgementKey {
            policy: row.get("policy"),
            model: row.get("model"),
            input_sha256: row.get(input_column),
        },
        value: from_json(row.get::<&str, _>(value_column), value_column)?,
        input_tokens: tokens_from_column(row.get("input_tokens"))?,
        output_tokens: tokens_from_column(row.get("output_tokens"))?,
        assessed_at: row.get("assessed_at"),
    })
}

fn outcome_from_row(row: &SqliteRow) -> Result<OutcomeRecord> {
    let level: String = row.get("level");
    let kind: String = row.get("claim_kind");
    let label: String = row.get("label_source");
    Ok(OutcomeRecord {
        id: row.get("id"),
        program_id: row.get("program_id"),
        lesson_id: row.get("lesson_id"),
        claim_id: row.get("claim_id"),
        source_version_id: row.get("source_version_id"),
        origin_key: OriginKey(row.get("origin_key")),
        outcome: Outcome {
            level: TrustLevel::from_column(&level).ok_or_else(|| unknown("level", &level))?,
            kind: ClaimKind::from_column(&kind).ok_or_else(|| unknown("claim kind", &kind))?,
            decision_was_sufficient: row.get::<i64, _>("decision_sufficient") != 0,
            right: row.get::<i64, _>("decision_right") != 0,
            at: row.get("at"),
        },
        label_source: LabelSource::from_column(&label)
            .ok_or_else(|| unknown("label source", &label))?,
        policy: row.get("policy"),
    })
}

#[async_trait]
impl SourceTrustStore for SqliteSourceTrustStore {
    async fn provenance(&self, source_version_id: &str) -> Result<Option<ProvenanceRecord>> {
        let sql =
            format!("SELECT {PROVENANCE_COLUMNS} FROM source_provenance WHERE source_version_id=?");
        sqlx::query(&sql)
            .bind(source_version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(provenance_from_row)
            .transpose()
    }

    async fn program_provenance(&self, program_id: &str) -> Result<Vec<ProvenanceRecord>> {
        let sql = format!(
            "SELECT {} FROM source_provenance p JOIN learning_source_library l ON l.program_id=p.program_id AND l.active_version_id=p.source_version_id WHERE p.program_id=? AND l.deleted_at IS NULL ORDER BY p.source_version_id",
            PROVENANCE_COLUMNS
                .split(',')
                .map(|column| format!("p.{column}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        sqlx::query(&sql)
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(provenance_from_row)
            .collect()
    }

    async fn save_provenance(&self, record: &ProvenanceRecord) -> Result<()> {
        let mut conn = self.pool.acquire().await.map_err(db)?;
        write_provenance(&mut conn, record).await
    }

    async fn standing(
        &self,
        key: &JudgementKey,
    ) -> Result<Option<StoredJudgement<StandingAssessment>>> {
        sqlx::query("SELECT signals_sha256,policy,model,judgement_json,input_tokens,output_tokens,assessed_at FROM source_standing_assessments WHERE signals_sha256=? AND policy=? AND model=?")
            .bind(&key.input_sha256)
            .bind(&key.policy)
            .bind(&key.model)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(|row| judgement(row, "signals_sha256", "judgement_json"))
            .transpose()
    }

    async fn save_standing(&self, judgement: &StoredJudgement<StandingAssessment>) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO source_standing_assessments(signals_sha256,policy,model,standing,judgement_json,input_tokens,output_tokens,assessed_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&judgement.key.input_sha256)
            .bind(&judgement.key.policy)
            .bind(&judgement.key.model)
            .bind(judgement.value.standing.as_str())
            .bind(to_json(&judgement.value)?)
            .bind(tokens_to_column(judgement.input_tokens)?)
            .bind(tokens_to_column(judgement.output_tokens)?)
            .bind(judgement.assessed_at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn standings(&self, keys: &[JudgementKey]) -> Result<Vec<StandingAssessment>> {
        let mut found = Vec::new();
        for key in keys {
            if let Some(stored) = self.standing(key).await? {
                found.push(stored.value);
            }
        }
        Ok(found)
    }

    async fn authority_for_subject(
        &self,
        program_id: &str,
        subject_key: &SubjectKey,
        policy: &str,
        model: &str,
    ) -> Result<Vec<AuthorityAssessment>> {
        sqlx::query("SELECT judgement_json FROM source_subject_authority WHERE program_id=? AND subject_key=? AND policy=? AND model=? ORDER BY origin_key")
            .bind(program_id)
            .bind(&subject_key.0)
            .bind(policy)
            .bind(model)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(|row| from_json(row.get::<&str, _>("judgement_json"), "judgement_json"))
            .collect()
    }

    async fn authority(
        &self,
        program_id: &str,
        origin_key: &OriginKey,
        subject_key: &SubjectKey,
        policy: &str,
        model: &str,
    ) -> Result<Option<StoredJudgement<AuthorityAssessment>>> {
        sqlx::query("SELECT input_sha256,policy,model,judgement_json,input_tokens,output_tokens,assessed_at FROM source_subject_authority WHERE program_id=? AND origin_key=? AND subject_key=? AND policy=? AND model=?")
            .bind(program_id)
            .bind(&origin_key.0)
            .bind(&subject_key.0)
            .bind(policy)
            .bind(model)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(|row| judgement(row, "input_sha256", "judgement_json"))
            .transpose()
    }

    async fn save_authority(
        &self,
        program_id: &str,
        judgement: &StoredJudgement<AuthorityAssessment>,
    ) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO source_subject_authority(program_id,origin_key,subject_key,policy,model,input_sha256,relation,judgement_json,input_tokens,output_tokens,assessed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(program_id)
            .bind(&judgement.value.origin_key.0)
            .bind(&judgement.value.subject_key.0)
            .bind(&judgement.key.policy)
            .bind(&judgement.key.model)
            .bind(&judgement.key.input_sha256)
            .bind(judgement.value.relation.as_str())
            .bind(to_json(&judgement.value)?)
            .bind(tokens_to_column(judgement.input_tokens)?)
            .bind(tokens_to_column(judgement.output_tokens)?)
            .bind(judgement.assessed_at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn claim_profile(
        &self,
        key: &JudgementKey,
    ) -> Result<Option<StoredJudgement<ClaimProfile>>> {
        sqlx::query("SELECT input_sha256,policy,model,profile_json,input_tokens,output_tokens,assessed_at FROM claim_profiles WHERE input_sha256=? AND policy=? AND model=?")
            .bind(&key.input_sha256)
            .bind(&key.policy)
            .bind(&key.model)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(|row| judgement(row, "input_sha256", "profile_json"))
            .transpose()
    }

    async fn save_claim_profiles(&self, profiles: &[StoredJudgement<ClaimProfile>]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for profile in profiles {
            sqlx::query("INSERT OR REPLACE INTO claim_profiles(input_sha256,policy,model,profile_json,input_tokens,output_tokens,assessed_at) VALUES(?,?,?,?,?,?,?)")
                .bind(&profile.key.input_sha256)
                .bind(&profile.key.policy)
                .bind(&profile.key.model)
                .bind(to_json(&profile.value)?)
                .bind(tokens_to_column(profile.input_tokens)?)
                .bind(tokens_to_column(profile.output_tokens)?)
                .bind(profile.assessed_at)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn relations(&self, program_id: &str) -> Result<Vec<SourceRelation>> {
        let sql = format!(
            "SELECT {RELATION_COLUMNS} FROM source_relations WHERE program_id=? ORDER BY from_version_id,kind,COALESCE(to_version_id,to_url)"
        );
        sqlx::query(&sql)
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(relation_from_row)
            .collect()
    }

    async fn save_relations(&self, program_id: &str, relations: &[SourceRelation]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        write_relations(&mut tx, program_id, relations, unix_seconds()).await?;
        tx.commit().await.map_err(db)
    }

    async fn overrides(&self, program_id: &str) -> Result<Overrides> {
        let sql = format!(
            "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides WHERE (scope='source' AND program_id=?) OR scope IN ('site','site_subject') ORDER BY scope,registrable_domain,class_key,source_id"
        );
        let entries = sqlx::query(&sql)
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(override_from_row)
            .collect::<Result<Vec<_>>>()?;
        Ok(Overrides { entries })
    }

    async fn site_overrides(&self) -> Result<Vec<TrustOverride>> {
        let sql = format!(
            "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides WHERE scope IN ('site','site_subject') ORDER BY registrable_domain,scope,class_key"
        );
        sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(override_from_row)
            .collect()
    }

    async fn set_override(&self, trust_override: &TrustOverride) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        write_override(&mut tx, trust_override).await?;
        tx.commit().await.map_err(db)
    }

    async fn clear_override(&self, scope: &OverrideScope) -> Result<bool> {
        let mut conn = self.pool.acquire().await.map_err(db)?;
        delete_override(&mut conn, scope).await
    }

    async fn record_outcome(&self, outcome: &OutcomeRecord) -> Result<()> {
        sqlx::query("INSERT INTO source_trust_outcomes(id,program_id,lesson_id,claim_id,source_version_id,origin_key,level,claim_kind,decision_sufficient,decision_right,label_source,policy,at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&outcome.id)
            .bind(&outcome.program_id)
            .bind(&outcome.lesson_id)
            .bind(&outcome.claim_id)
            .bind(&outcome.source_version_id)
            .bind(&outcome.origin_key.0)
            .bind(outcome.outcome.level.as_str())
            .bind(outcome.outcome.kind.as_str())
            .bind(outcome.outcome.decision_was_sufficient as i64)
            .bind(outcome.outcome.right as i64)
            .bind(outcome.label_source.as_str())
            .bind(&outcome.policy)
            .bind(outcome.outcome.at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn outcomes(&self) -> Result<Vec<OutcomeRecord>> {
        sqlx::query("SELECT id,program_id,lesson_id,claim_id,source_version_id,origin_key,level,claim_kind,decision_sufficient,decision_right,label_source,policy,at FROM source_trust_outcomes ORDER BY at,rowid")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(outcome_from_row)
            .collect()
    }

    /// `right` counts the claims that held (a sufficient decision that was
    /// right, or an insufficient one that was wrong: `Outcome::held`),
    /// `wrong` the rest.
    async fn track_record(&self, origin_key: &OriginKey) -> Result<TrackRecord> {
        let (held, failed): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(decision_sufficient=decision_right),0), COALESCE(SUM(decision_sufficient<>decision_right),0) FROM source_trust_outcomes WHERE origin_key=?",
        )
        .bind(&origin_key.0)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let count = |value: i64| {
            u32::try_from(value).map_err(|_| {
                lattice_core::shared::error::AppError::Database(
                    "Track record count is out of range".into(),
                )
            })
        };
        Ok(TrackRecord {
            right: count(held)?,
            wrong: count(failed)?,
        })
    }
}
