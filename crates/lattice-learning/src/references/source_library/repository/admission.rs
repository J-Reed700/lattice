use super::super::admission::UnusedSource;
use super::*;
use std::collections::HashSet;

/// A page the gate has not read under its current policy, with its text.
pub(in super::super) struct UnadmittedVersion {
    pub source_id: String,
    pub revision: i64,
    pub version_id: String,
    pub title: String,
    pub text: String,
    pub url: Option<String>,
}

/// A source research added, with the version a lesson may cite.
pub(in super::super) struct ResearchSource {
    pub version_id: String,
    pub source: UnusedSource,
}

pub(in super::super) async fn unadmitted(
    connection: &mut SqliteConnection,
    program_id: &str,
    policy: &str,
) -> Result<Vec<UnadmittedVersion>> {
    let rows = sqlx::query(
        "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, v.full_text, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND s.added_by<>'user' AND (s.added_by<>'outline' OR s.found_by_search=1) AND (v.admission_policy IS NULL OR v.admission_policy<>?) ORDER BY s.created_at,s.id",
    )
    .bind(program_id)
    .bind(policy)
    .fetch_all(connection)
    .await
    .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| UnadmittedVersion {
            source_id: row.get("source_id"),
            revision: row.get("revision"),
            version_id: row.get("version_id"),
            title: row.get("title"),
            text: row.get("full_text"),
            url: row.get("url"),
        })
        .collect())
}

pub(in super::super) async fn mark_admission(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
    reason: &Option<String>,
    policy: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_source_versions SET not_a_source=?, admission_policy=? WHERE program_id=? AND id=?",
    )
    .bind(reason)
    .bind(policy)
    .bind(program_id)
    .bind(version_id)
    .execute(connection)
    .await
    .map_err(db)?;
    Ok(())
}

pub(in super::super) async fn admitted_under(
    pool: &impl DriverPool,
    program_id: &str,
    policy: &str,
    words: i64,
) -> Result<Vec<ShortSource>> {
    let pool = pool.sqlx_pool();
    let rows = sqlx::query(
        "SELECT s.id AS source_id, v.id AS version_id, v.title, v.word_count, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL AND v.admission_policy=? AND v.word_count<? ORDER BY v.word_count, s.created_at, s.id",
    )
    .bind(program_id)
    .bind(policy)
    .bind(words)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| ShortSource {
            source_id: row.get("source_id"),
            version_id: row.get("version_id"),
            title: row.get("title"),
            url: row.get("url"),
            words: usize::try_from(row.get::<i64, _>("word_count")).unwrap_or_default(),
        })
        .collect())
}

pub(in super::super) async fn refused_sources(
    pool: &impl DriverPool,
    program_id: &str,
) -> Result<Vec<RefusedSource>> {
    let pool = pool.sqlx_pool();
    let rows = sqlx::query(
        "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, v.not_a_source AS reason, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NOT NULL ORDER BY s.created_at,s.id",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| RefusedSource {
            source_id: row.get("source_id"),
            revision: row.get("revision"),
            version_id: row.get("version_id"),
            title: row.get("title"),
            url: row.get("url"),
            reason: row.get("reason"),
        })
        .collect())
}

pub(in super::super) async fn job_running(
    pool: &impl DriverPool,
    kind: &str,
    subject_id: &str,
) -> Result<bool> {
    let pool = pool.sqlx_pool();
    let running: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM jobs WHERE kind=? AND subject_id=? AND status IN ('pending','running') LIMIT 1",
    )
    .bind(kind)
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    Ok(running.is_some())
}

pub(in super::super) async fn cited_by_reports(
    pool: &impl DriverPool,
    program_id: &str,
) -> Result<HashSet<String>> {
    let pool = pool.sqlx_pool();
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT DISTINCT json_extract(e.value, '$.source_id') FROM learning_lesson_verifications r, json_each(r.report_json, '$.findings') f, json_each(f.value, '$.evidence') e WHERE r.program_id=?",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?
    .into_iter()
    .flatten()
    .collect())
}

pub(in super::super) async fn admitted_added_by(
    pool: &impl DriverPool,
    program_id: &str,
    added_by: &str,
) -> Result<Vec<ResearchSource>> {
    let pool = pool.sqlx_pool();
    let rows = sqlx::query(
        "SELECT s.id AS source_id, s.revision, v.id AS version_id, v.title, COALESCE(v.resolved_url, v.requested_url) AS url FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND s.added_by=? AND v.not_a_source IS NULL ORDER BY s.created_at,s.id",
    )
    .bind(program_id)
    .bind(added_by)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| ResearchSource {
            version_id: row.get("version_id"),
            source: UnusedSource {
                source_id: row.get("source_id"),
                revision: row.get("revision"),
                title: row.get("title"),
                url: row.get("url"),
            },
        })
        .collect())
}
