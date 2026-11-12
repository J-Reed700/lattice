use super::rows::from_json;
use crate::persistence::db;
use lattice_core::domain::provenance::{
    AddedBy, AuthorityAssessment, ClaimProfile, StandingAssessment,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::Row;

/// One version as the trust view reads it.
#[derive(Debug, Clone)]
pub(in crate::references::trust) struct Version {
    pub source_id: String,
    pub version_id: String,
    pub added_by: AddedBy,
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

pub(in crate::references::trust) async fn latest_claim_profile(
    database: &Database,
    input_sha256: &str,
    policy: &str,
) -> Result<Option<ClaimProfile>> {
    latest(
        database,
        "SELECT profile_json AS judgement FROM claim_profiles WHERE input_sha256=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[input_sha256, policy],
        "profile_json",
    )
    .await
}

pub(in crate::references::trust) async fn latest_standing(
    database: &Database,
    signals_sha256: &str,
    policy: &str,
) -> Result<Option<StandingAssessment>> {
    latest(
        database,
        "SELECT judgement_json AS judgement FROM source_standing_assessments WHERE signals_sha256=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[signals_sha256, policy],
        "judgement_json",
    )
    .await
}

pub(in crate::references::trust) async fn latest_authority(
    database: &Database,
    program_id: &str,
    origin_key: &str,
    subject_key: &str,
    policy: &str,
) -> Result<Option<AuthorityAssessment>> {
    latest(
        database,
        "SELECT judgement_json AS judgement FROM source_subject_authority WHERE program_id=? AND origin_key=? AND subject_key=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
        &[program_id, origin_key, subject_key, policy],
        "judgement_json",
    )
    .await
}

async fn latest<T: serde::de::DeserializeOwned>(
    database: &Database,
    sql: &str,
    binds: &[&str],
    column: &str,
) -> Result<Option<T>> {
    let mut query = sqlx::query(sql);
    for value in binds {
        query = query.bind(*value);
    }
    let mut read = database.begin_read().await.map_err(db)?;
    let row = query.fetch_optional(&mut *read).await.map_err(db)?;
    read.commit().await.map_err(db)?;
    row.map(|row| from_json(&row.get::<String, _>("judgement"), column))
        .transpose()
}

pub(in crate::references::trust) async fn active_versions(
    database: &Database,
    program_id: &str,
) -> Result<Vec<Version>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let rows = sqlx::query(
        "SELECT id AS source_id, active_version_id AS version_id, added_by FROM learning_source_library \
         WHERE program_id=? AND deleted_at IS NULL AND active_version_id IS NOT NULL ORDER BY created_at, id",
    )
    .bind(program_id)
    .fetch_all(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    rows.iter().map(version).collect()
}

pub(in crate::references::trust) async fn version_of(
    database: &Database,
    program_id: &str,
    version_id: &str,
) -> Result<Option<Version>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let row = sqlx::query(
        "SELECT v.source_id, v.id AS version_id, l.added_by FROM learning_source_versions v \
         JOIN learning_source_library l ON l.id = v.source_id AND l.program_id = v.program_id \
         WHERE v.program_id=? AND v.id=?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_optional(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    row.as_ref().map(version).transpose()
}
