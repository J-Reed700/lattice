//! Program-scoped immutable source snapshots and refresh history.
use crate::features::learning::{
    dto::*,
    portability_dto::{DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto},
};
use crate::features::learning::{
    operations::Operation,
    persistence::{db, hash, hash_text, now},
};
use crate::shared::error::{AppError, Result};
use futures::TryStreamExt;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;

mod capture;
mod lifecycle;
mod reads;

const MAX_EXCERPT_CHARS: usize = 2_400;
const LEGACY_EXTRACTION: &str = "legacy_bounded_extraction_v1";
const MAX_CHECKS_IN_WORKSPACE: i64 = 50;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}
fn excerpt(text: &str) -> String {
    text.chars().take(MAX_EXCERPT_CHARS).collect()
}
fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID")))
}
fn owned_id(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(invalid(format!("Invalid {label} ID")));
    }
    Ok(())
}
fn policy(value: &LearningSourcePolicy) -> &'static str {
    match value {
        LearningSourcePolicy::Fixed => "fixed",
        LearningSourcePolicy::Manual => "manual",
        LearningSourcePolicy::BeforeUse => "before_use",
    }
}
fn parse_policy(value: &str) -> Result<LearningSourcePolicy> {
    match value {
        "fixed" => Ok(LearningSourcePolicy::Fixed),
        "manual" => Ok(LearningSourcePolicy::Manual),
        "before_use" => Ok(LearningSourcePolicy::BeforeUse),
        _ => Err(AppError::Database("Invalid source freshness policy".into())),
    }
}
fn parse_kind(value: &str) -> Result<LearningSourceKind> {
    match value {
        "web" => Ok(LearningSourceKind::Web),
        "document" => Ok(LearningSourceKind::Document),
        "pasted" => Ok(LearningSourceKind::Pasted),
        _ => Err(AppError::Database("Invalid source kind".into())),
    }
}
fn parse_check(value: &str) -> Result<LearningSourceCheckStatus> {
    match value {
        "unchanged" => Ok(LearningSourceCheckStatus::Unchanged),
        "update_available" => Ok(LearningSourceCheckStatus::UpdateAvailable),
        "failed" => Ok(LearningSourceCheckStatus::Failed),
        _ => Err(AppError::Database("Invalid source check status".into())),
    }
}
fn check_name(value: &LearningSourceCheckStatus) -> &'static str {
    match value {
        LearningSourceCheckStatus::Unchanged => "unchanged",
        LearningSourceCheckStatus::UpdateAvailable => "update_available",
        LearningSourceCheckStatus::Failed => "failed",
    }
}

#[derive(Clone)]
pub struct LearningSourceLibraryRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct CapturedLearningSource {
    pub title: String,
    pub publisher: Option<String>,
    pub requested_url: Option<String>,
    pub resolved_url: Option<String>,
    pub text: String,
    pub truncated: bool,
    pub extraction_version: String,
}

impl LearningSourceLibraryRepository {
    pub fn before_use_allows(status: &LearningSourceCheckStatus) -> Result<()> {
        match status {
            LearningSourceCheckStatus::Unchanged => Ok(()),
            LearningSourceCheckStatus::UpdateAvailable => Err(invalid(
                "A newer version is available. Adopt it or change the freshness policy before preparing a lesson.",
            )),
            LearningSourceCheckStatus::Failed => Err(AppError::ServiceNotAvailable(
                "The before-use source check failed; retry after the source is reachable.".into(),
            )),
        }
    }
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    fn validate_program_id(id: &str) -> Result<()> {
        uuid(id, "program")
    }

    pub async fn preflight_replay(
        &self,
        operation_id: &str,
        program_id: &str,
        source_id: &str,
        kind: &str,
        payload_hash: &str,
    ) -> Result<bool> {
        uuid(operation_id, "operation")?;
        source_operation(operation_id, program_id, source_id, kind, payload_hash)
            .seen(&self.pool)
            .await
    }

    pub async fn preflight_new_source(
        &self,
        program_id: &str,
        source_id: &str,
        version_id: &str,
    ) -> Result<()> {
        Self::validate_program_id(program_id)?;
        uuid(source_id, "source")?;
        uuid(version_id, "source version")?;
        let active: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=? AND status='active'")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if active.is_none() {
            return Err(AppError::NotFound(
                "Active learning program not found".into(),
            ));
        }
        let collision: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_source_library WHERE program_id=? AND id IN (?,?) UNION SELECT 1 FROM learning_source_versions WHERE program_id=? AND id IN (?,?) LIMIT 1",
        )
        .bind(program_id)
        .bind(source_id)
        .bind(version_id)
        .bind(program_id)
        .bind(source_id)
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        if collision.is_some() {
            return Err(invalid(
                "The requested source or version ID is already in use.",
            ));
        }
        Ok(())
    }

    /// Finalize SQL-migrated legacy snapshots before exposing metadata. This is
    /// idempotent and only touches rows explicitly marked as legacy bounded.
    async fn backfill_legacy(&self, program_id: &str) -> Result<()> {
        let rows = sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=? AND extraction_version=? AND content_sha256='' ")
            .bind(program_id).bind(LEGACY_EXTRACTION).fetch_all(&self.pool).await.map_err(db)?;
        for row in rows {
            let id: String = row.get("id");
            let text: String = row.get("full_text");
            sqlx::query("UPDATE learning_source_versions SET content_sha256=?,word_count=? WHERE program_id=? AND id=? AND extraction_version=? AND content_sha256='' ")
                .bind(hash_text(&text)).bind(words(&text) as i64).bind(program_id).bind(id).bind(LEGACY_EXTRACTION).execute(&self.pool).await.map_err(db)?;
        }
        Ok(())
    }
}

fn summary(r: &sqlx::sqlite::SqliteRow) -> LearningSourceVersionSummaryDto {
    LearningSourceVersionSummaryDto {
        id: r.get("id"),
        version_number: r.get("version_number"),
        title: r.get("title"),
        publisher: r.get("publisher"),
        resolved_url: r.get("resolved_url"),
        excerpt: r.get("excerpt"),
        content_sha256: r.get("content_sha256"),
        word_count: r.get::<i64, _>("word_count").max(0) as usize,
        truncated: r.get::<i64, _>("truncated") != 0,
        extraction_version: r.get("extraction_version"),
        acquired_at: r.get("acquired_at"),
    }
}
fn find_case_insensitive(text: &str, query: &str) -> Option<usize> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    let folded: Vec<(char, usize)> = text
        .chars()
        .enumerate()
        .flat_map(|(index, ch)| ch.to_lowercase().map(move |lower| (lower, index)))
        .collect();
    folded
        .windows(needle.len())
        .position(|window| window.iter().map(|(ch, _)| *ch).eq(needle.iter().copied()))
        .and_then(|position| folded.get(position).map(|(_, index)| *index))
}
fn context_excerpt(text: &str, match_char: usize) -> String {
    let begin = match_char.saturating_sub(180);
    text.chars().skip(begin).take(420).collect()
}

async fn lock_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
    let result =
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=? AND status='active'")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound(
            "Active learning program not found".into(),
        ));
    }
    Ok(())
}
fn source_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    source_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "source",
        kind,
        program_id: Some(program_id),
        subject_id: Some(source_id),
        payload_hash,
        conflict: "Operation ID was already used with different source data.",
    }
}

/// What a source operation produced: the version it captured, if any, and
/// the source revision it left.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceOperationResult<'a> {
    version_id: Option<&'a str>,
    revision: i64,
}

fn portability_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "portability",
        kind,
        program_id: Some(program_id),
        subject_id: None,
        payload_hash,
        conflict: "Operation ID was reused with different source data.",
    }
}

async fn require_active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
    let row: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=? AND status='active'")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
    if row.is_none() {
        return Err(AppError::NotFound(
            "Active learning program not found".into(),
        ));
    }
    Ok(())
}

/// One immutable source version snapshot: each source field alongside its
/// content hash. The excerpt and word count are derived from `full_text`.
struct SourceVersionInsert<'a> {
    program_id: &'a str,
    source_id: &'a str,
    version_id: &'a str,
    version_number: i64,
    title: &'a str,
    publisher: Option<&'a str>,
    requested_url: Option<&'a str>,
    resolved_url: Option<&'a str>,
    full_text: &'a str,
    truncated: bool,
    extraction_version: &'a str,
    acquired_at: i64,
    content_sha256: &'a str,
}

async fn insert_version(
    tx: &mut Transaction<'_, Sqlite>,
    v: SourceVersionInsert<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(v.version_id).bind(v.program_id).bind(v.source_id).bind(v.version_number).bind(v.title).bind(v.publisher).bind(v.requested_url).bind(v.resolved_url).bind(v.full_text).bind(excerpt(v.full_text)).bind(v.content_sha256).bind(words(v.full_text) as i64).bind(v.truncated as i64).bind(v.extraction_version).bind(v.acquired_at).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
async fn mirror_legacy(
    tx: &mut Transaction<'_, Sqlite>,
    program: &str,
    id: &str,
    title: &str,
    url: Option<&str>,
    text: &str,
    at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,?)")
        .bind(id).bind(program).bind(title.trim()).bind(url).bind(excerpt(text)).bind(at).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
