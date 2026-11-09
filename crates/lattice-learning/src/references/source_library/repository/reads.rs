use super::*;
use futures::{stream::BoxStream, FutureExt, StreamExt};

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
        snapshot_hash: r.get("snapshot_hash"),
    }
}

/// Read source metadata and history through an existing database snapshot.
pub(in super::super) async fn workspace_on(
    connection: &mut SqliteConnection,
    program_id: &str,
    include_deleted: bool,
) -> Result<LearningSourceWorkspaceDto> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .boxed()
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
        .fetch_all(&mut *connection)
        .boxed()
        .await
        .map_err(db)?;
    let mut sources = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get("id");
        let versions = version_summaries(connection, program_id, &id).await?;
        let active_id: Option<String> = row.try_get("active_version_id").map_err(db)?;
        let pending_id: Option<String> = row.try_get("pending_version_id").map_err(db)?;
        let checks = refresh_checks(connection, program_id, &id).await?;
        sources.push(LearningSourceLibraryItemDto {
            id: id.clone(),
            kind: parse_kind(row.get("kind"))?,
            added_by: parse_added_by(row.get("added_by"))?.into(),
            found_by_search: Some(row.get::<i64, _>("found_by_search") != 0),
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
        not_sources: Vec::new(),
        unused_research: Vec::new(),
        recheck: None,
    })
}

async fn version_summaries(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<LearningSourceVersionSummaryDto>> {
    let rows = sqlx::query("SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? ORDER BY version_number DESC")
        .bind(program_id).bind(source_id).fetch_all(&mut *connection).boxed().await.map_err(db)?;
    rows.into_iter().map(|r| Ok(summary(&r))).collect()
}

async fn refresh_checks(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<LearningSourceCheckDto>> {
    let rows = sqlx::query("SELECT * FROM learning_source_refresh_checks WHERE program_id=? AND source_id=? ORDER BY checked_at DESC,rowid DESC LIMIT ?")
        .bind(program_id).bind(source_id).bind(MAX_CHECKS_IN_WORKSPACE).fetch_all(&mut *connection).boxed().await.map_err(db)?;
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

/// One stored version: its summary and the whole captured text.
pub(in super::super) struct StoredVersion {
    pub summary: LearningSourceVersionSummaryDto,
    pub full_text: String,
}

pub(in super::super) async fn stored_version(
    pool: &SqlitePool,
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
    .fetch_optional(pool)
    .boxed()
    .await
    .map_err(db)?;
    Ok(row.map(|row| StoredVersion {
        summary: summary(&row),
        full_text: row.get("full_text"),
    }))
}

pub(in super::super) async fn version_usage(
    pool: &SqlitePool,
    program_id: &str,
    version_id: &str,
) -> Result<Vec<LearningSourceUsageDto>> {
    let mut read = pool.begin_read().await.map_err(db)?;
    let usage = version_usage_on(&mut read, program_id, version_id).await?;
    read.commit().await.map_err(db)?;
    Ok(usage)
}

async fn version_usage_on(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<Vec<LearningSourceUsageDto>> {
    let mut out = Vec::new();
    let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,b.title ref_title FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? AND EXISTS(SELECT 1 FROM json_each(b.source_ids_json) WHERE value=?) ORDER BY l.ordinal,b.ordinal")
        .bind(program_id).bind(version_id).fetch_all(&mut *connection).boxed().await.map_err(db)?;
    for r in rows {
        out.push(LearningSourceUsageDto {
            lesson_id: r.get("lesson_id"),
            lesson_title: r.get("lesson_title"),
            reference_kind: "lesson_block".into(),
            reference_title: r.get("ref_title"),
        });
    }
    let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,c.prompt ref_title FROM learning_assessment_candidates c JOIN learning_lessons l ON l.id=c.lesson_id WHERE c.program_id=? AND c.review_status='agreed' AND EXISTS(SELECT 1 FROM json_each(c.source_version_ids_json) WHERE value=?) ORDER BY l.ordinal,c.rowid")
        .bind(program_id).bind(version_id).fetch_all(&mut *connection).boxed().await.map_err(db)?;
    for r in rows {
        out.push(LearningSourceUsageDto {
            lesson_id: r.get("lesson_id"),
            lesson_title: r.get("lesson_title"),
            reference_kind: "assessment_question".into(),
            reference_title: r.get("ref_title"),
        });
    }
    let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,d.question ref_title FROM learning_card_drafts d JOIN learning_lessons l ON l.id=d.lesson_id WHERE d.program_id=? AND d.status='pending' AND EXISTS(SELECT 1 FROM json_each(d.source_ids_json) WHERE value=?) ORDER BY l.ordinal,d.created_at")
        .bind(program_id).bind(version_id).fetch_all(&mut *connection).boxed().await.map_err(db)?;
    for r in rows {
        out.push(LearningSourceUsageDto {
            lesson_id: r.get("lesson_id"),
            lesson_title: r.get("lesson_title"),
            reference_kind: "recall_draft".into(),
            reference_title: r.get("ref_title"),
        });
    }
    let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,c.question ref_title FROM learning_card_origins o JOIN learning_lessons l ON l.id=o.lesson_id JOIN study_cards c ON c.id=o.card_id WHERE o.program_id=? AND EXISTS(SELECT 1 FROM json_each(o.source_ids_json) WHERE value=?) ORDER BY l.ordinal,o.accepted_at")
        .bind(program_id).bind(version_id).fetch_all(&mut *connection).boxed().await.map_err(db)?;
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

/// A stored version's text, as a search reads it.
pub(in super::super) struct SearchedVersion {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub text: String,
}

/// Every version of the program, newest first, read as the caller pulls them.
pub(in super::super) fn searchable_versions<'a>(
    pool: &'a SqlitePool,
    program_id: &'a str,
) -> BoxStream<'a, Result<SearchedVersion>> {
    sqlx::query("SELECT source_id,id,title,full_text FROM learning_source_versions WHERE program_id=? ORDER BY acquired_at DESC,id")
        .bind(program_id)
        .fetch(pool)
        .map(|row| -> Result<SearchedVersion> {
            let r = row.map_err(db)?;
            Ok(SearchedVersion {
                source_id: r.get("source_id"),
                version_id: r.get("id"),
                title: r.get("title"),
                text: r.get("full_text"),
            })
        })
        .boxed()
}

fn source_dto(r: &sqlx::sqlite::SqliteRow) -> LearningSourceDto {
    LearningSourceDto {
        id: r.get("id"),
        title: r.get("title"),
        url: r
            .get::<Option<String>, _>("resolved_url")
            .or_else(|| r.get("requested_url")),
        excerpt: r.get("excerpt"),
        acquired_at: r.get("acquired_at"),
    }
}

pub(in super::super) async fn excerpted_sources(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<LearningSourceDto>> {
    let rows=sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.excerpt,v.acquired_at FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id).fetch_all(pool).boxed().await.map_err(db)?;
    Ok(rows.iter().map(source_dto).collect())
}

pub(in super::super) async fn full_text_sources(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<LearningSourceDto>> {
    let rows=sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.full_text AS excerpt,v.acquired_at FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id).fetch_all(pool).boxed().await.map_err(db)?;
    Ok(rows.iter().map(source_dto).collect())
}

pub(in super::super) async fn active_versions(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<ActiveVersion>> {
    let rows = sqlx::query("SELECT v.id,v.title,v.content_sha256 FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id)
        .fetch_all(pool).boxed()
        .await
        .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| ActiveVersion {
            id: row.get("id"),
            title: row.get("title"),
            content_sha256: row.get("content_sha256"),
        })
        .collect())
}

pub(in super::super) async fn version_text(
    pool: &SqlitePool,
    program_id: &str,
    version_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT full_text FROM learning_source_versions WHERE program_id=? AND id=?")
        .bind(program_id)
        .bind(version_id)
        .fetch_optional(pool)
        .boxed()
        .await
        .map_err(db)
}

pub(in super::super) async fn version_texts(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<HashMap<String, String>> {
    let rows = sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=?")
        .bind(program_id)
        .fetch_all(pool)
        .boxed()
        .await
        .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("id"), r.get("full_text")))
        .collect())
}
