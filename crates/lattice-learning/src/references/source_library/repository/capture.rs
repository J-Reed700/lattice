use super::*;
use futures::FutureExt;

/// The library row of a new source.
pub(in super::super) struct SourceInsert<'a> {
    pub id: &'a str,
    pub program_id: &'a str,
    pub kind: &'a str,
    pub origin: &'a str,
    pub requested_url: Option<&'a str>,
    pub freshness_policy: &'a str,
    pub active_version_id: &'a str,
    pub created_at: i64,
    pub added_by: &'a str,
}

pub(in super::super) async fn insert_source(
    connection: &mut SqliteConnection,
    s: SourceInsert<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at,added_by) VALUES(?,?,?,?,?,?,?,NULL,0,?,?,?)")
        .bind(s.id).bind(s.program_id).bind(s.kind).bind(s.origin).bind(s.requested_url).bind(s.freshness_policy).bind(s.active_version_id).bind(s.created_at).bind(s.created_at).bind(s.added_by).execute(connection).await.map_err(db)?;
    Ok(())
}

pub(in super::super) async fn set_selection_note(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    note: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_source_library SET selection_note=? WHERE program_id=? AND id=?")
        .bind(note)
        .bind(program_id)
        .bind(source_id)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// An active, admitted source's current version, as the page check reads it.
pub(in super::super) struct ActivePage {
    pub version_id: String,
    pub resolved_url: Option<String>,
    pub requested_url: Option<String>,
    pub content_sha256: Option<String>,
}

pub(in super::super) async fn active_pages(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<ActivePage>> {
    let rows = sqlx::query("SELECT v.id,v.resolved_url,v.requested_url,v.content_sha256 FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id)
        .fetch_all(connection)
        .boxed()
        .await
        .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| ActivePage {
            version_id: row.get("id"),
            resolved_url: row.get("resolved_url"),
            requested_url: row.get("requested_url"),
            content_sha256: row.get("content_sha256"),
        })
        .collect())
}

/// A source as the refresh offer reads it, with its program's status.
pub(in super::super) struct RefreshOffer {
    pub revision: i64,
    pub kind: String,
    pub requested_url: Option<String>,
    pub freshness_policy: String,
    pub deleted_at: Option<i64>,
    pub program_status: String,
}

pub(in super::super) async fn refresh_offer(
    pool: &SqlitePool,
    program_id: &str,
    source_id: &str,
) -> Result<Option<RefreshOffer>> {
    let row = sqlx::query("SELECT s.revision,s.kind,s.requested_url,s.freshness_policy,s.deleted_at,p.status FROM learning_source_library s JOIN learning_programs p ON p.id=s.program_id WHERE s.program_id=? AND s.id=?")
        .bind(program_id).bind(source_id).fetch_optional(pool).boxed().await.map_err(db)?;
    Ok(row.map(|r| RefreshOffer {
        revision: r.get("revision"),
        kind: r.get("kind"),
        requested_url: r.get("requested_url"),
        freshness_policy: r.get("freshness_policy"),
        deleted_at: r.get("deleted_at"),
        program_status: r.get("status"),
    }))
}

/// A source as a refresh finds it inside its transaction.
pub(in super::super) struct RefreshState {
    pub revision: i64,
    pub pending_version_id: Option<String>,
    pub active_version_id: Option<String>,
    pub kind: String,
    pub requested_url: Option<String>,
    pub freshness_policy: String,
}

pub(in super::super) async fn refresh_state(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<RefreshState>> {
    let row = sqlx::query("SELECT revision,active_version_id,pending_version_id,kind,requested_url,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
        .bind(program_id).bind(source_id).fetch_optional(connection).boxed().await.map_err(db)?;
    Ok(row.map(|r| RefreshState {
        revision: r.get("revision"),
        pending_version_id: r.get("pending_version_id"),
        active_version_id: r.get("active_version_id"),
        kind: r.get("kind"),
        requested_url: r.get("requested_url"),
        freshness_policy: r.get("freshness_policy"),
    }))
}

pub(in super::super) async fn version_digest(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT content_sha256 FROM learning_source_versions WHERE program_id=? AND id=?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_optional(connection)
    .boxed()
    .await
    .map_err(db)
}

/// Whether the source still had `expected_revision` and took the new pending version.
pub(in super::super) async fn set_pending_version(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    pending_version_id: &Option<String>,
    (revision, expected_revision): (i64, i64),
    updated_at: i64,
) -> Result<bool> {
    let result=sqlx::query("UPDATE learning_source_library SET pending_version_id=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
        .bind(pending_version_id).bind(revision).bind(updated_at).bind(program_id).bind(source_id).bind(expected_revision).execute(connection).await.map_err(db)?;
    Ok(result.rows_affected() == 1)
}

/// One refresh check, as the workspace lists it.
pub(in super::super) struct RefreshCheckInsert<'a> {
    pub operation_id: &'a str,
    pub program_id: &'a str,
    pub source_id: &'a str,
    pub status: &'a str,
    pub checked_at: i64,
    pub active_digest: &'a Option<String>,
    pub pending_version_id: &'a Option<String>,
    pub message: &'a Option<String>,
}

pub(in super::super) async fn insert_refresh_check(
    connection: &mut SqliteConnection,
    c: RefreshCheckInsert<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_refresh_checks(operation_id,program_id,source_id,status,checked_at,active_digest,pending_version_id,message) VALUES(?,?,?,?,?,?,?,?)")
        .bind(c.operation_id).bind(c.program_id).bind(c.source_id).bind(c.status).bind(c.checked_at).bind(c.active_digest).bind(c.pending_version_id).bind(c.message).execute(connection).await.map_err(db)?;
    Ok(())
}
