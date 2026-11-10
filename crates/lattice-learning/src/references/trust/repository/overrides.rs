use crate::persistence::db;
use crate::references::trust::overrides::CoveredVersion;
use lattice_core::domain::provenance::OverrideScope;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;
use sqlx::{Row, SqliteConnection};

/// A version a scope covers: its course and its id.
pub(in crate::references::trust) type CoveredIds = (String, String);

fn covered_ids(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<CoveredIds> {
    rows.iter()
        .map(|row| (row.get("program_id"), row.get("source_version_id")))
        .collect()
}

pub(in crate::references::trust) async fn source_versions(
    conn: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<CoveredIds>> {
    let rows = sqlx::query(
        "SELECT program_id, id AS source_version_id FROM learning_source_versions WHERE program_id=? AND source_id=? ORDER BY version_number",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?;
    Ok(covered_ids(&rows))
}

pub(in crate::references::trust) async fn site_versions(
    conn: &mut SqliteConnection,
    registrable_domain: &str,
) -> Result<Vec<CoveredIds>> {
    let rows = sqlx::query(
        "SELECT program_id, source_version_id FROM source_provenance WHERE site=? ORDER BY program_id, source_version_id",
    )
    .bind(registrable_domain)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?;
    Ok(covered_ids(&rows))
}

/// Per site and origin, the outcomes that held and the ones that failed.
pub(in crate::references::trust) async fn site_outcome_counts(
    pool: &impl DatabaseHandle,
    sites: &[&str],
) -> Result<Vec<(String, i64, i64)>> {
    let pool = pool.sqlx_pool();
    sqlx::query_as(
        "SELECT p.site, SUM(o.decision_sufficient=o.decision_right),
                        SUM(o.decision_sufficient<>o.decision_right)
         FROM (SELECT DISTINCT site,origin_key FROM source_provenance
               WHERE site IN (SELECT value FROM json_each(?))) p
         JOIN source_trust_outcomes o ON o.origin_key=p.origin_key
         GROUP BY p.site,p.origin_key",
    )
    .bind(serde_json::to_string(sites)?)
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// A source scope covers every version of that source in its course; a
/// site scope (with or without a subject class) covers every captured
/// version whose origin is that site, in every course: the class narrows
/// which claims the level applies to, which the gate decides.
pub(in crate::references::trust) async fn covered_in(
    conn: &mut SqliteConnection,
    scope: &OverrideScope,
) -> Result<Vec<CoveredVersion>> {
    let rows = match scope {
        OverrideScope::Source {
            program_id,
            source_id,
        } => source_versions(conn, program_id, source_id).await?,
        OverrideScope::Site { registrable_domain }
        | OverrideScope::SiteSubject {
            registrable_domain, ..
        } => site_versions(conn, registrable_domain).await?,
    };
    Ok(rows
        .into_iter()
        .map(|(program_id, source_version_id)| CoveredVersion {
            program_id,
            source_version_id,
        })
        .collect())
}
