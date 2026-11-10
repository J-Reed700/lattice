use super::*;

pub(in super::super) struct AdoptionState {
    pub revision: i64,
    pub pending_version_id: Option<String>,
    pub active_version_id: Option<String>,
}

pub(in super::super) async fn adoption_state(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<AdoptionState>> {
    let row = sqlx::query("SELECT revision,pending_version_id,active_version_id FROM learning_source_library WHERE program_id=? AND id=?")
        .bind(program_id).bind(source_id).fetch_optional(connection).await.map_err(db)?;
    Ok(row.map(|r| AdoptionState {
        revision: r.get("revision"),
        pending_version_id: r.get("pending_version_id"),
        active_version_id: r.get("active_version_id"),
    }))
}

/// Whether the source still had `expected_revision` and took the version.
pub(in super::super) async fn adopt_version(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    version_id: &str,
    (revision, expected_revision): (i64, i64),
    updated_at: i64,
) -> Result<bool> {
    let result=sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
        .bind(version_id).bind(revision).bind(updated_at).bind(program_id).bind(source_id).bind(expected_revision).execute(connection).await.map_err(db)?;
    Ok(result.rows_affected() == 1)
}

pub(in super::super) struct PolicyState {
    pub revision: i64,
    pub kind: String,
    pub freshness_policy: String,
}

pub(in super::super) async fn policy_state(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<PolicyState>> {
    let row = sqlx::query("SELECT revision,kind,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
        .bind(program_id).bind(source_id).fetch_optional(connection).await.map_err(db)?;
    Ok(row.map(|r| PolicyState {
        revision: r.get("revision"),
        kind: r.get("kind"),
        freshness_policy: r.get("freshness_policy"),
    }))
}

/// Whether the source still had `expected_revision` and took the policy.
pub(in super::super) async fn set_policy(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    freshness_policy: &str,
    (revision, expected_revision): (i64, i64),
    updated_at: i64,
) -> Result<bool> {
    let result=sqlx::query("UPDATE learning_source_library SET freshness_policy=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
        .bind(freshness_policy).bind(revision).bind(updated_at).bind(program_id).bind(source_id).bind(expected_revision).execute(connection).await.map_err(db)?;
    Ok(result.rows_affected() == 1)
}

pub(in super::super) struct DeletionState {
    pub revision: i64,
    pub deleted_at: Option<i64>,
}

pub(in super::super) async fn deletion_state(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<DeletionState>> {
    let row = sqlx::query(
        "SELECT revision,deleted_at FROM learning_source_library WHERE program_id=? AND id=?",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_optional(connection)
    .await
    .map_err(db)?;
    Ok(row.map(|r| DeletionState {
        revision: r.get("revision"),
        deleted_at: r.get("deleted_at"),
    }))
}

/// Whether the source still had `expected_revision` and was live to delete.
pub(in super::super) async fn mark_deleted(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    reason: &str,
    expected_revision: i64,
    deleted_at: i64,
) -> Result<bool> {
    let updated = sqlx::query("UPDATE learning_source_library SET deleted_at=?,deletion_reason=?,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NULL")
        .bind(deleted_at)
        .bind(reason)
        .bind(deleted_at)
        .bind(program_id)
        .bind(source_id)
        .bind(expected_revision)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(updated.rows_affected() == 1)
}

pub(in super::super) async fn active_text_in(
    conn: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT v.full_text FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.id=?",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(db)
}

/// A source as a re-import finds it.
pub(in super::super) struct SourceRow {
    pub revision: i64,
    pub deleted_at: Option<i64>,
    pub requested_url: Option<String>,
    pub active_version_id: Option<String>,
    pub kind: String,
}

pub(in super::super) async fn source_row(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Option<SourceRow>> {
    let row = sqlx::query("SELECT * FROM learning_source_library WHERE program_id=? AND id=?")
        .bind(program_id)
        .bind(source_id)
        .fetch_optional(connection)
        .await
        .map_err(db)?;
    Ok(row.map(|r| SourceRow {
        revision: r.get("revision"),
        deleted_at: r.get("deleted_at"),
        requested_url: r.get("requested_url"),
        active_version_id: r.get("active_version_id"),
        kind: r.get("kind"),
    }))
}

/// A stored version as a re-import restores it.
pub(in super::super) struct StoredVersion {
    pub id: String,
    pub title: String,
    pub publisher: Option<String>,
    pub resolved_url: Option<String>,
    pub extraction_version: String,
    pub truncated: bool,
    pub full_text: String,
}

pub(in super::super) async fn stored_version(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    version_id: &str,
) -> Result<Option<StoredVersion>> {
    let row = sqlx::query(
        "SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?",
    )
    .bind(program_id)
    .bind(source_id)
    .bind(version_id)
    .fetch_optional(connection)
    .await
    .map_err(db)?;
    Ok(row.map(|r| StoredVersion {
        id: r.get("id"),
        title: r.get("title"),
        publisher: r.get("publisher"),
        resolved_url: r.get("resolved_url"),
        extraction_version: r.get("extraction_version"),
        truncated: r.get::<i64, _>("truncated") != 0,
        full_text: r.get("full_text"),
    }))
}

pub(in super::super) async fn version_id_in_use(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<bool> {
    let collision: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?")
            .bind(program_id)
            .bind(version_id)
            .fetch_optional(connection)
            .await
            .map_err(db)?;
    Ok(collision.is_some())
}

/// Whether the source still had `expected_revision` and was removed.
pub(in super::super) async fn restore_source(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    active_version_id: &str,
    expected_revision: i64,
    restored_at: i64,
) -> Result<bool> {
    let updated = sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,deleted_at=NULL,deletion_reason=NULL,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NOT NULL")
        .bind(active_version_id)
        .bind(restored_at)
        .bind(program_id)
        .bind(source_id)
        .bind(expected_revision)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(updated.rows_affected() == 1)
}
