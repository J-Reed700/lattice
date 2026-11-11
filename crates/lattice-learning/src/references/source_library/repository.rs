//! The source library's statements, their bindings and their row mapping.
use super::*;
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Transactions;
use sqlx::{Row, SqliteConnection};

pub(super) mod admission;
pub(super) mod capture;
pub(super) mod evidence;
pub(super) mod lifecycle;
pub(super) mod reads;

pub(super) async fn active_program_exists(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<bool> {
    let row: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=? AND status='active'")
            .bind(program_id)
            .fetch_optional(connection)
            .await
            .map_err(db)?;
    Ok(row.is_some())
}

pub(super) async fn ids_in_use(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    version_id: &str,
) -> Result<bool> {
    let collision: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM learning_source_library WHERE program_id=? AND id IN (?,?) UNION SELECT 1 FROM learning_source_versions WHERE program_id=? AND id IN (?,?) LIMIT 1",
    )
    .bind(program_id)
    .bind(source_id)
    .bind(version_id)
    .bind(program_id)
    .bind(source_id)
    .bind(version_id)
    .fetch_optional(connection)
    .await
    .map_err(db)?;
    Ok(collision.is_some())
}

/// Finalizes SQL-migrated legacy snapshots in one transaction.
pub(super) async fn backfill_legacy(pool: &impl DriverPool, program_id: &str) -> Result<()> {
    let pool = pool.sqlx_pool();
    let waiting: Option<i64> = {
        let mut read = pool.begin_read().await.map_err(db)?;
        let waiting = sqlx::query_scalar("SELECT 1 FROM learning_source_versions WHERE program_id=? AND extraction_version=? AND content_sha256='' LIMIT 1")
            .bind(program_id).bind(LEGACY_EXTRACTION).fetch_optional(&mut *read).await.map_err(db)?;
        read.commit().await.map_err(db)?;
        waiting
    };
    if waiting.is_none() {
        return Ok(());
    }
    let mut tx = pool.begin_write().await.map_err(db)?;
    let rows = sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=? AND extraction_version=? AND content_sha256='' ")
        .bind(program_id).bind(LEGACY_EXTRACTION).fetch_all(&mut *tx).await.map_err(db)?;
    for row in rows {
        let id: String = row.get("id");
        let text: String = row.get("full_text");
        sqlx::query("UPDATE learning_source_versions SET content_sha256=?,word_count=? WHERE program_id=? AND id=? AND extraction_version=? AND content_sha256='' ")
            .bind(hash_text(&text)).bind(words(&text) as i64).bind(program_id).bind(id).bind(LEGACY_EXTRACTION).execute(&mut *tx).await.map_err(db)?;
    }
    tx.commit().await.map_err(db)
}

/// Whether the active program exists and is now locked by this transaction.
pub(super) async fn lock_active_program(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<bool> {
    let result =
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=? AND status='active'")
            .bind(program_id)
            .execute(connection)
            .await
            .map_err(db)?;
    Ok(result.rows_affected() == 1)
}

pub(super) async fn version_with_digest(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    digest: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=? AND content_sha256=?")
        .bind(program_id).bind(source_id).bind(digest).fetch_optional(connection).await.map_err(db)
}

pub(super) async fn next_version_number(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT COALESCE(MAX(version_number),0)+1 FROM learning_source_versions WHERE program_id=? AND source_id=?")
        .bind(program_id).bind(source_id).fetch_one(connection).await.map_err(db)
}

/// One immutable source version snapshot: each source field alongside its
/// content hash. The excerpt and word count are derived from `full_text`.
pub(super) struct SourceVersionInsert<'a> {
    pub program_id: &'a str,
    pub source_id: &'a str,
    pub version_id: &'a str,
    pub version_number: i64,
    pub title: &'a str,
    pub publisher: Option<&'a str>,
    pub requested_url: Option<&'a str>,
    pub resolved_url: Option<&'a str>,
    pub full_text: &'a str,
    pub truncated: bool,
    pub extraction_version: &'a str,
    pub acquired_at: i64,
    pub content_sha256: &'a str,
}

pub(super) async fn insert_version(
    connection: &mut SqliteConnection,
    v: SourceVersionInsert<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(v.version_id).bind(v.program_id).bind(v.source_id).bind(v.version_number).bind(v.title).bind(v.publisher).bind(v.requested_url).bind(v.resolved_url).bind(v.full_text).bind(excerpt(v.full_text)).bind(v.content_sha256).bind(words(v.full_text) as i64).bind(v.truncated as i64).bind(v.extraction_version).bind(v.acquired_at).execute(connection).await.map_err(db)?;
    Ok(())
}

pub(super) async fn mirror_legacy(
    connection: &mut SqliteConnection,
    program: &str,
    id: &str,
    title: &str,
    url: Option<&str>,
    text: &str,
    at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,?)")
        .bind(id).bind(program).bind(title.trim()).bind(url).bind(excerpt(text)).bind(at).execute(connection).await.map_err(db)?;
    Ok(())
}
