//! Program-scoped immutable source snapshots and refresh history.
use super::{
    dto::*,
    portability_dto::{DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto},
};
use crate::shared::error::{AppError, Result};
use futures::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

const MAX_TEXT_CHARS: usize = 64_000;
const MAX_EXCERPT_CHARS: usize = 2_400;
const LEGACY_EXTRACTION: &str = "legacy_bounded_extraction_v1";
const MAX_SOURCE_COUNT: i64 = 100;
const MAX_VERSION_COUNT: i64 = 100;
const MAX_CHECKS_IN_WORKSPACE: i64 = 50;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}
fn bounded_text(text: &str) -> (String, bool) {
    let normalized = normalized(text);
    let truncated = normalized.chars().count() > MAX_TEXT_CHARS;
    (normalized.chars().take(MAX_TEXT_CHARS).collect(), truncated)
}
fn excerpt(text: &str) -> String {
    text.chars().take(MAX_EXCERPT_CHARS).collect()
}
fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
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
fn json_hash<T: serde::Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| AppError::Serialization(e.to_string()))?;
    Ok(digest(&String::from_utf8_lossy(&bytes)))
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
        let row=sqlx::query("SELECT program_id,source_id,kind,payload_hash FROM learning_source_operations WHERE operation_id=?")
            .bind(operation_id).fetch_optional(&self.pool).await.map_err(db)?;
        let Some(row) = row else {
            return Ok(false);
        };
        if row.get::<String, _>("program_id") == program_id
            && row.get::<String, _>("source_id") == source_id
            && row.get::<String, _>("kind") == kind
            && row.get::<String, _>("payload_hash") == payload_hash
        {
            return Ok(true);
        }
        Err(invalid(
            "Operation ID was already used with different source data.",
        ))
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
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_source_library WHERE program_id=?")
                .bind(program_id)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        if count >= MAX_SOURCE_COUNT {
            return Err(invalid(
                "A learning program can contain at most 100 sources.",
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
                .bind(digest(&text)).bind(words(&text) as i64).bind(program_id).bind(id).bind(LEGACY_EXTRACTION).execute(&self.pool).await.map_err(db)?;
        }
        Ok(())
    }

    pub async fn workspace(&self, program_id: &str) -> Result<LearningSourceWorkspaceDto> {
        self.workspace_with_deleted(program_id, false).await
    }

    /// Build a source snapshot for portable exports, including tombstones whose
    /// immutable versions and refresh history must remain recoverable.
    pub(crate) async fn workspace_including_deleted(
        &self,
        program_id: &str,
    ) -> Result<LearningSourceWorkspaceDto> {
        self.workspace_with_deleted(program_id, true).await
    }

    async fn workspace_with_deleted(
        &self,
        program_id: &str,
        include_deleted: bool,
    ) -> Result<LearningSourceWorkspaceDto> {
        Self::validate_program_id(program_id)?;
        self.backfill_legacy(program_id).await?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        let query = if include_deleted {
            "SELECT * FROM learning_source_library WHERE program_id=? ORDER BY created_at,id"
        } else {
            "SELECT * FROM learning_source_library WHERE program_id=? AND deleted_at IS NULL ORDER BY created_at,id"
        };
        let rows = sqlx::query(query)
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut sources = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let versions = self.version_summaries(program_id, &id).await?;
            let active_id: Option<String> = row.try_get("active_version_id").map_err(db)?;
            let pending_id: Option<String> = row.try_get("pending_version_id").map_err(db)?;
            let checks = self.checks(program_id, &id).await?;
            sources.push(LearningSourceLibraryItemDto {
                id: id.clone(),
                kind: parse_kind(row.get("kind"))?,
                origin: row.get("origin"),
                requested_url: row.get("requested_url"),
                freshness_policy: parse_policy(row.get("freshness_policy"))?,
                active_version_id: active_id.clone(),
                pending_version_id: pending_id.clone(),
                revision: row.get("revision"),
                deleted_at: row.get("deleted_at"),
                deletion_reason: row.get("deletion_reason"),
                active_version: active_id
                    .as_deref()
                    .and_then(|vid| versions.iter().find(|v| v.id == vid).cloned()),
                pending_version: pending_id
                    .as_deref()
                    .and_then(|vid| versions.iter().find(|v| v.id == vid).cloned()),
                versions,
                latest_check: checks.first().cloned(),
                checks,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }
        Ok(LearningSourceWorkspaceDto {
            program_id: program_id.into(),
            sources,
        })
    }

    async fn version_summaries(
        &self,
        program_id: &str,
        source_id: &str,
    ) -> Result<Vec<LearningSourceVersionSummaryDto>> {
        let rows = sqlx::query("SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? ORDER BY version_number DESC")
            .bind(program_id).bind(source_id).fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter().map(|r| Ok(summary(&r))).collect()
    }
    async fn checks(
        &self,
        program_id: &str,
        source_id: &str,
    ) -> Result<Vec<LearningSourceCheckDto>> {
        let rows = sqlx::query("SELECT * FROM learning_source_refresh_checks WHERE program_id=? AND source_id=? ORDER BY checked_at DESC,rowid DESC LIMIT ?")
            .bind(program_id).bind(source_id).bind(MAX_CHECKS_IN_WORKSPACE).fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningSourceCheckDto {
                    operation_id: r.get("operation_id"),
                    status: parse_check(r.get("status"))?,
                    checked_at: r.get("checked_at"),
                    active_digest: r.get("active_digest"),
                    pending_version_id: r.get("pending_version_id"),
                    message: r.get("message"),
                })
            })
            .collect()
    }

    pub async fn get_version(
        &self,
        req: &GetLearningSourceVersionRequestDto,
    ) -> Result<LearningSourceVersionDto> {
        Self::validate_program_id(&req.program_id)?;
        for (v, label) in [
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if v.trim().is_empty() || v.len() > 256 {
                return Err(invalid(format!("Invalid {label} ID")));
            }
        }
        self.backfill_legacy(&req.program_id).await?;
        let row = sqlx::query(
            "SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .bind(&req.version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Source version not found in this program".into()))?;
        let usage = self.version_usage(&req.program_id, &req.version_id).await?;
        Ok(LearningSourceVersionDto {
            source_id: req.source_id.clone(),
            version: summary(&row),
            full_text: row.get("full_text"),
            usage,
        })
    }

    async fn version_usage(
        &self,
        program_id: &str,
        version_id: &str,
    ) -> Result<Vec<LearningSourceUsageDto>> {
        let mut out = Vec::new();
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,b.title ref_title FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? AND EXISTS(SELECT 1 FROM json_each(b.source_ids_json) WHERE value=?) ORDER BY l.ordinal,b.ordinal")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "lesson_block".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,q.prompt ref_title FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=? AND EXISTS(SELECT 1 FROM json_each(q.source_ids_json) WHERE value=?) ORDER BY l.ordinal,q.ordinal")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "assessment_question".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,d.question ref_title FROM learning_card_drafts d JOIN learning_lessons l ON l.id=d.lesson_id WHERE d.program_id=? AND d.status='pending' AND EXISTS(SELECT 1 FROM json_each(d.source_ids_json) WHERE value=?) ORDER BY l.ordinal,d.created_at")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "recall_draft".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,c.question ref_title FROM learning_card_origins o JOIN learning_lessons l ON l.id=o.lesson_id JOIN study_cards c ON c.id=o.card_id WHERE o.program_id=? AND EXISTS(SELECT 1 FROM json_each(o.source_ids_json) WHERE value=?) ORDER BY l.ordinal,o.accepted_at")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "accepted_recall_card".into(),
                reference_title: r.get("ref_title"),
            });
        }
        Ok(out)
    }

    pub async fn search(
        &self,
        req: &SearchLearningSourcesRequestDto,
    ) -> Result<Vec<LearningSourceSearchResultDto>> {
        Self::validate_program_id(&req.program_id)?;
        let query = req.query.trim();
        if query.is_empty() || query.chars().count() > 200 {
            return Err(invalid("Search query must contain 1–200 characters"));
        }
        let limit = req.limit.clamp(1, 50) as i64;
        let mut rows=sqlx::query("SELECT source_id,id,title,full_text FROM learning_source_versions WHERE program_id=? ORDER BY acquired_at DESC,id")
            .bind(&req.program_id).fetch(&self.pool);
        let mut results = Vec::new();
        while let Some(r) = rows.try_next().await.map_err(db)? {
            let text: String = r.get("full_text");
            if let Some(index) = find_case_insensitive(&text, query) {
                results.push(LearningSourceSearchResultDto {
                    source_id: r.get("source_id"),
                    version_id: r.get("id"),
                    title: r.get("title"),
                    excerpt: context_excerpt(&text, index),
                });
                if results.len() as i64 >= limit {
                    break;
                }
            }
        }
        Ok(results)
    }

    // Source capture owns a fixed provenance tuple plus caller-supplied stable
    // IDs; keeping those fields explicit makes replay validation auditable.
    #[allow(clippy::too_many_arguments)]
    pub async fn add(
        &self,
        operation_id: &str,
        source_id: &str,
        version_id: &str,
        program_id: &str,
        kind: LearningSourceKind,
        operation_kind: &str,
        origin: &str,
        requested_url: Option<&str>,
        freshness: LearningSourcePolicy,
        captured: CapturedLearningSource,
        payload_hash: String,
    ) -> Result<()> {
        for (v, label) in [
            (operation_id, "operation"),
            (source_id, "source"),
            (version_id, "source version"),
            (program_id, "program"),
        ] {
            uuid(v, label)?;
        }
        let (full_text, was_truncated) = bounded_text(&captured.text);
        if full_text.trim().is_empty() {
            return Err(invalid("Source text must not be empty"));
        }
        let title = captured.title.trim();
        if title.is_empty() || title.chars().count() > 180 {
            return Err(invalid("Source title must contain 1–180 characters"));
        }
        if origin.trim().is_empty() || origin.chars().count() > 2048 {
            return Err(invalid("Source origin must contain 1–2048 characters"));
        }
        if requested_url.is_some_and(|value| value.chars().count() > 2048)
            || captured
                .resolved_url
                .as_deref()
                .is_some_and(|value| value.chars().count() > 2048)
        {
            return Err(invalid("Source URLs must not exceed 2048 characters"));
        }
        if captured
            .publisher
            .as_deref()
            .is_some_and(|value| value.chars().count() > 180)
        {
            return Err(invalid("Publisher must not exceed 180 characters"));
        }
        if matches!(
            kind,
            LearningSourceKind::Document | LearningSourceKind::Pasted
        ) && freshness != LearningSourcePolicy::Fixed
        {
            return Err(invalid("Document and pasted sources use the fixed policy"));
        }
        let acquired = now();
        let source_kind = match kind {
            LearningSourceKind::Web => "web",
            LearningSourceKind::Document => "document",
            LearningSourceKind::Pasted => "pasted",
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, program_id).await?;
        if replay(
            &mut tx,
            operation_id,
            program_id,
            source_id,
            operation_kind,
            &payload_hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_source_library WHERE program_id=?")
                .bind(program_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if count >= MAX_SOURCE_COUNT {
            return Err(invalid(
                "A learning program can contain at most 100 sources.",
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
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if collision.is_some() {
            return Err(invalid(
                "The requested source or version ID is already in use.",
            ));
        }
        let (text, truncated) = (full_text, was_truncated || captured.truncated);
        let text_digest = digest(&text);
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,NULL,0,?,?)")
            .bind(source_id).bind(program_id).bind(source_kind).bind(origin).bind(requested_url).bind(policy(&freshness)).bind(version_id).bind(acquired).bind(acquired).execute(&mut *tx).await.map_err(db)?;
        insert_version(
            &mut tx,
            program_id,
            source_id,
            version_id,
            1,
            title,
            captured.publisher.as_deref(),
            requested_url,
            captured.resolved_url.as_deref(),
            &text,
            truncated,
            &captured.extraction_version,
            acquired,
            &text_digest,
        )
        .await?;
        mirror_legacy(
            &mut tx,
            program_id,
            version_id,
            title,
            captured.resolved_url.as_deref().or(requested_url),
            &text,
            acquired,
        )
        .await?;
        record_operation(
            &mut tx,
            operation_id,
            program_id,
            source_id,
            operation_kind,
            &payload_hash,
            Some(version_id),
            0,
            acquired,
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn source_for_refresh(
        &self,
        program_id: &str,
        source_id: &str,
    ) -> Result<(i64, String, LearningSourcePolicy)> {
        Self::validate_program_id(program_id)?;
        owned_id(source_id, "source")?;
        self.backfill_legacy(program_id).await?;
        let r=sqlx::query("SELECT s.revision,s.kind,s.requested_url,s.freshness_policy,s.deleted_at,p.status FROM learning_source_library s JOIN learning_programs p ON p.id=s.program_id WHERE s.program_id=? AND s.id=?")
            .bind(program_id).bind(source_id).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        if r.get::<String, _>("status") != "active"
            || r.get::<Option<i64>, _>("deleted_at").is_some()
        {
            return Err(AppError::NotFound(
                "Active learning source not found".into(),
            ));
        }
        if r.get::<String, _>("kind") != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        let freshness = parse_policy(r.get("freshness_policy"))?;
        if freshness == LearningSourcePolicy::Fixed {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        let url: Option<String> = r.get("requested_url");
        Ok((
            r.get("revision"),
            url.ok_or_else(|| AppError::Database("Web source has no requested URL".into()))?,
            freshness,
        ))
    }

    pub async fn refresh(
        &self,
        req: &RefreshLearningSourceRequestDto,
        captured: Option<CapturedLearningSource>,
        failure: Option<String>,
    ) -> Result<()> {
        for (v, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
        ] {
            if label == "source" {
                owned_id(v, label)?;
            } else {
                uuid(v, label)?;
            }
        }
        let payload_hash = json_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "refresh",
            &payload_hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        let src=sqlx::query("SELECT revision,active_version_id,pending_version_id,kind,requested_url,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = src.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if src.get::<String, _>("kind") != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        if src.get::<String, _>("freshness_policy") == "fixed" {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        let active_id: Option<String> = src.get("active_version_id");
        let active_digest: Option<String> = if let Some(ref id) = active_id {
            sqlx::query_scalar(
                "SELECT content_sha256 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
        } else {
            None
        };
        let checked = now();
        let (status, pending_id, message) = if let Some(message) = failure {
            (
                LearningSourceCheckStatus::Failed,
                src.get("pending_version_id"),
                Some(message.chars().take(500).collect::<String>()),
            )
        } else {
            let captured = captured.ok_or_else(|| invalid("Refresh result is missing"))?;
            let (text, was_truncated) = bounded_text(&captured.text);
            if text.trim().is_empty()
                || captured.title.trim().is_empty()
                || captured.title.chars().count() > 180
                || captured
                    .publisher
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 180)
                || captured
                    .resolved_url
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 2048)
            {
                (
                    LearningSourceCheckStatus::Failed,
                    src.get("pending_version_id"),
                    Some("The page did not contain valid source metadata and text.".into()),
                )
            } else {
                let new_digest = digest(&text);
                if active_digest.as_deref() == Some(new_digest.as_str()) {
                    (LearningSourceCheckStatus::Unchanged, None, None)
                } else {
                    let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=? AND content_sha256=?")
                        .bind(&req.program_id).bind(&req.source_id).bind(&new_digest).fetch_optional(&mut *tx).await.map_err(db)?;
                    let version_id = if let Some(id) = existing {
                        id
                    } else {
                        let version_count:i64=sqlx::query_scalar("SELECT count(*) FROM learning_source_versions WHERE program_id=? AND source_id=?").bind(&req.program_id).bind(&req.source_id).fetch_one(&mut *tx).await.map_err(db)?;
                        if version_count >= MAX_VERSION_COUNT {
                            return Err(invalid(
                                "A source can contain at most 100 captured versions.",
                            ));
                        }
                        let next:i64=sqlx::query_scalar("SELECT COALESCE(MAX(version_number),0)+1 FROM learning_source_versions WHERE program_id=? AND source_id=?")
                            .bind(&req.program_id).bind(&req.source_id).fetch_one(&mut *tx).await.map_err(db)?;
                        let version_id = uuid::Uuid::new_v4().to_string();
                        let requested_url: Option<String> = src.get("requested_url");
                        insert_version(
                            &mut tx,
                            &req.program_id,
                            &req.source_id,
                            &version_id,
                            next,
                            captured.title.trim(),
                            captured.publisher.as_deref(),
                            requested_url.as_deref(),
                            captured.resolved_url.as_deref(),
                            &text,
                            was_truncated || captured.truncated,
                            &captured.extraction_version,
                            checked,
                            &new_digest,
                        )
                        .await?;
                        mirror_legacy(
                            &mut tx,
                            &req.program_id,
                            &version_id,
                            captured.title.trim(),
                            captured.resolved_url.as_deref(),
                            &text,
                            checked,
                        )
                        .await?;
                        version_id
                    };
                    (
                        LearningSourceCheckStatus::UpdateAvailable,
                        Some(version_id),
                        None,
                    )
                }
            }
        };
        let old_pending: Option<String> = src.get("pending_version_id");
        let next_revision =
            if status != LearningSourceCheckStatus::Failed && old_pending != pending_id {
                revision + 1
            } else {
                revision
            };
        if next_revision != revision {
            let result=sqlx::query("UPDATE learning_source_library SET pending_version_id=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
                .bind(&pending_id).bind(next_revision).bind(checked).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        let result_version = pending_id.as_deref();
        record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "refresh",
            &payload_hash,
            result_version,
            next_revision,
            checked,
        )
        .await?;
        sqlx::query("INSERT INTO learning_source_refresh_checks(operation_id,program_id,source_id,status,checked_at,active_digest,pending_version_id,message) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&req.operation_id).bind(&req.program_id).bind(&req.source_id).bind(check_name(&status)).bind(checked).bind(active_digest).bind(&pending_id).bind(&message).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn adopt(&self, req: &AdoptLearningSourceVersionRequestDto) -> Result<()> {
        for (v, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if label == "source" {
                owned_id(v, label)?;
            } else {
                uuid(v, label)?;
            }
        }
        let hash = json_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        let row=sqlx::query("SELECT revision,pending_version_id,active_version_id FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        let pending: Option<String> = row.get("pending_version_id");
        if pending.as_deref() != Some(&req.version_id) {
            return Err(invalid("Only the pending source version can be adopted."));
        }
        let current: Option<String> = row.get("active_version_id");
        let next_revision = if current.as_deref() == Some(&req.version_id) {
            revision
        } else {
            revision + 1
        };
        if next_revision != revision {
            let result=sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(&req.version_id).bind(next_revision).bind(now()).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
            Some(&req.version_id),
            next_revision,
            now(),
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn update_policy(&self, req: &UpdateLearningSourcePolicyRequestDto) -> Result<()> {
        for (v, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
        ] {
            if label == "source" {
                owned_id(v, label)?;
            } else {
                uuid(v, label)?;
            }
        }
        let hash = json_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        let row=sqlx::query("SELECT revision,kind,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.get::<String, _>("kind") != "web"
            && req.freshness_policy != LearningSourcePolicy::Fixed
        {
            return Err(invalid("Document and pasted sources use the fixed policy"));
        }
        let old: String = row.get("freshness_policy");
        let next = if old == policy(&req.freshness_policy) {
            revision
        } else {
            revision + 1
        };
        if next != revision {
            let result=sqlx::query("UPDATE learning_source_library SET freshness_policy=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(policy(&req.freshness_policy)).bind(next).bind(now()).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
            None,
            next,
            now(),
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn delete_source(&self, req: &DeleteLearningSourceRequestDto) -> Result<()> {
        for (value, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
        ] {
            if label == "source" {
                owned_id(value, label)?;
            } else {
                uuid(value, label)?;
            }
        }
        let reason = req.reason.trim();
        if reason.is_empty() || reason.chars().count() > 500 {
            return Err(invalid("Deletion reason must contain 1–500 characters."));
        }
        let hash = json_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "delete_source",
            &hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let row = sqlx::query(
            "SELECT revision,deleted_at FROM learning_source_library WHERE program_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.get::<Option<i64>, _>("deleted_at").is_some() {
            return Err(invalid("This source is already removed."));
        }
        let timestamp = now();
        let updated = sqlx::query("UPDATE learning_source_library SET deleted_at=?,deletion_reason=?,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NULL")
            .bind(timestamp)
            .bind(reason)
            .bind(timestamp)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(req.expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Source changed; reload and retry."));
        }
        record_portability_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "delete_source",
            &hash,
            Some(&req.source_id),
            timestamp,
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn reimport_source(&self, req: &ReimportLearningSourceRequestDto) -> Result<()> {
        for (value, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if label == "source" {
                owned_id(value, label)?;
            } else {
                uuid(value, label)?;
            }
        }
        if req
            .replacement_text
            .as_ref()
            .is_some_and(|text| text.trim().is_empty() || text.chars().count() > MAX_TEXT_CHARS)
        {
            return Err(invalid(
                "Replacement source text must contain 1–64,000 characters.",
            ));
        }
        let hash = json_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_replay(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "reimport_source",
            &hash,
        )
        .await?
        .is_some()
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let source =
            sqlx::query("SELECT * FROM learning_source_library WHERE program_id=? AND id=?")
                .bind(&req.program_id)
                .bind(&req.source_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = source.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if source.get::<Option<i64>, _>("deleted_at").is_none() {
            return Err(invalid("Only a removed source can be re-imported."));
        }
        let requested_url: Option<String> = source.get("requested_url");
        let current_id: Option<String> = source.get("active_version_id");
        let prior = if let Some(version_id) = current_id.as_deref() {
            sqlx::query("SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?")
                .bind(&req.program_id)
                .bind(&req.source_id)
                .bind(version_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
        } else {
            None
        };
        let prior = prior.ok_or_else(|| {
            AppError::InvalidState("Removed source has no version to re-import.".into())
        })?;
        let prior_text: String = prior.get("full_text");
        let text_input = req.replacement_text.as_deref().unwrap_or(&prior_text);
        let (text, was_truncated) = bounded_text(text_input);
        if text.trim().is_empty() {
            return Err(invalid("Re-imported source text must not be empty."));
        }
        let digest_value = digest(&text);
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=? AND content_sha256=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .bind(&digest_value)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let active_version = if let Some(existing_id) = existing {
            existing_id
        } else {
            let collision: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(&req.version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if collision.is_some() {
                return Err(invalid(
                    "The requested source version ID is already in use.",
                ));
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM learning_source_versions WHERE program_id=? AND source_id=?",
            )
            .bind(&req.program_id)
            .bind(&req.source_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if count >= MAX_VERSION_COUNT {
                return Err(invalid(
                    "A source can contain at most 100 captured versions.",
                ));
            }
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version_number),0)+1 FROM learning_source_versions WHERE program_id=? AND source_id=?",
            )
            .bind(&req.program_id)
            .bind(&req.source_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            let title: String = prior.get("title");
            let publisher: Option<String> = prior.get("publisher");
            let resolved_url: Option<String> = prior.get("resolved_url");
            let extraction: String = prior.get("extraction_version");
            let truncated = was_truncated || prior.get::<i64, _>("truncated") != 0;
            let timestamp = now();
            insert_version(
                &mut tx,
                &req.program_id,
                &req.source_id,
                &req.version_id,
                next,
                &title,
                publisher.as_deref(),
                requested_url.as_deref(),
                resolved_url.as_deref(),
                &text,
                truncated,
                &extraction,
                timestamp,
                &digest_value,
            )
            .await?;
            mirror_legacy(
                &mut tx,
                &req.program_id,
                &req.version_id,
                &title,
                resolved_url.as_deref().or(requested_url.as_deref()),
                &text,
                timestamp,
            )
            .await?;
            req.version_id.clone()
        };
        let timestamp = now();
        let updated = sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,deleted_at=NULL,deletion_reason=NULL,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NOT NULL")
            .bind(&active_version)
            .bind(timestamp)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(req.expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Source changed; reload and retry."));
        }
        record_portability_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            "reimport_source",
            &hash,
            Some(&active_version),
            timestamp,
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn prepare_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        self.backfill_legacy(program_id).await?;
        let rows=sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.excerpt,v.acquired_at FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| LearningSourceDto {
                id: r.get("id"),
                title: r.get("title"),
                url: r
                    .get::<Option<String>, _>("resolved_url")
                    .or_else(|| r.get("requested_url")),
                excerpt: r.get("excerpt"),
                acquired_at: r.get("acquired_at"),
            })
            .collect())
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
async fn replay(
    tx: &mut Transaction<'_, Sqlite>,
    op: &str,
    program: &str,
    source: &str,
    kind: &str,
    hash: &str,
) -> Result<Option<()>> {
    let row=sqlx::query("SELECT program_id,source_id,kind,payload_hash FROM learning_source_operations WHERE operation_id=?")
        .bind(op).fetch_optional(&mut **tx).await.map_err(db)?;
    if let Some(r) = row {
        if r.get::<String, _>("program_id") == program
            && r.get::<String, _>("source_id") == source
            && r.get::<String, _>("kind") == kind
            && r.get::<String, _>("payload_hash") == hash
        {
            return Ok(Some(()));
        }
        return Err(invalid(
            "Operation ID was already used with different source data.",
        ));
    }
    Ok(None)
}
// Operation identity, result, revision, and timestamp are recorded atomically
// with the source mutation, so these transaction details stay explicit here.
#[allow(clippy::too_many_arguments)]
async fn record_operation(
    tx: &mut Transaction<'_, Sqlite>,
    op: &str,
    program: &str,
    source: &str,
    kind: &str,
    hash: &str,
    version: Option<&str>,
    revision: i64,
    at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_operations(operation_id,program_id,source_id,kind,payload_hash,result_version_id,result_revision,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(op).bind(program).bind(source).bind(kind).bind(hash).bind(version).bind(revision).bind(at).execute(&mut **tx).await.map_err(db)?;
    Ok(())
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

async fn portability_replay(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    kind: &str,
    payload_hash: &str,
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT program_id,kind,payload_hash,result_id FROM learning_portability_operations WHERE operation_id=?",
    )
    .bind(operation_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<Option<String>, _>("program_id").as_deref() == Some(program_id)
        && row.get::<String, _>("kind") == kind
        && row.get::<String, _>("payload_hash") == payload_hash
    {
        return Ok(row.get("result_id"));
    }
    Err(invalid(
        "Operation ID was reused with different source data.",
    ))
}

async fn record_portability_operation(
    tx: &mut Transaction<'_, Sqlite>,
    operation_id: &str,
    program_id: &str,
    kind: &str,
    payload_hash: &str,
    result_id: Option<&str>,
    timestamp: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,?,?,?)")
        .bind(operation_id)
        .bind(program_id)
        .bind(kind)
        .bind(payload_hash)
        .bind(result_id)
        .bind(timestamp)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}
// Version snapshots persist each source field alongside its immutable hash.
#[allow(clippy::too_many_arguments)]
async fn insert_version(
    tx: &mut Transaction<'_, Sqlite>,
    program: &str,
    source: &str,
    id: &str,
    number: i64,
    title: &str,
    publisher: Option<&str>,
    requested: Option<&str>,
    resolved: Option<&str>,
    text: &str,
    truncated: bool,
    extract: &str,
    at: i64,
    sha: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(id).bind(program).bind(source).bind(number).bind(title).bind(publisher).bind(requested).bind(resolved).bind(text).bind(excerpt(text)).bind(sha).bind(words(text) as i64).bind(truncated as i64).bind(extract).bind(at).execute(&mut **tx).await.map_err(db)?;
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
