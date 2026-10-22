//! What the repository records about packs and reads back: the export and
//! import ledgers, stored previews, the workspace read model built from them,
//! and the notice of what an export leaves behind.
use super::{conflicts::blocks, db, guards::active_job_exists, invalid, json_string, parse};
use crate::features::learning::{
    operations::Operation,
    pack::{DecodedLearningPack, LearningPackManifest},
    persistence::now,
    portability_dto::*,
    source_library::LearningSourceLibraryRepository,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};
use std::path::Path;

/// What a preview found, before it is stored.
pub(super) struct PreviewRecord {
    pub(super) program_id: String,
    pub(super) title: String,
    pub(super) conflicts: Vec<LearningPackConflictDto>,
    pub(super) changes: Vec<LearningPackChangeDto>,
    pub(super) warnings: Vec<String>,
}

/// A replacement's backup of the course it removes, recorded with the
/// replacement in the same commit.
#[derive(Clone)]
pub(super) struct BackupExportRecord {
    pub(super) id: String,
    pub(super) program_id: String,
    pub(super) operation_id: String,
    pub(super) payload_hash: String,
    pub(super) format_version: i64,
    pub(super) root_sha256: String,
    pub(super) destination_path: String,
    pub(super) privacy_manifest_json: String,
    pub(super) entry_manifest_json: String,
    pub(super) manifest_json: String,
    pub(super) created_at: i64,
}

/// Pack requests are not program data: a preview or import may name a program
/// that does not exist yet, so their ledger rows belong to no program.
pub(super) fn pack_operation<'a>(
    operation_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "portability",
        kind,
        program_id: None,
        subject_id: None,
        payload_hash,
        conflict: "Operation ID was reused with a different pack request.",
    }
}

/// What an earlier identical pack request produced: the export's file
/// path, the preview ID, or the imported program ID.
pub(super) async fn replay(
    pool: &SqlitePool,
    id: &str,
    kind: &str,
    payload: &str,
) -> Result<Option<String>> {
    pack_operation(id, kind, payload)
        .replay::<String, _>(pool)
        .await
}

/// The stored form of a policy is its wire form, so one codec serves both.
fn policy_column(policy: LearningPackConflictPolicy) -> Result<String> {
    serde_json::to_value(policy)
        .map_err(|e| AppError::Serialization(e.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Serialization("A pack policy has no name.".into()))
}

fn parse_policy(value: &str) -> Result<LearningPackConflictPolicy> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|_| AppError::Database("Invalid pack conflict policy".into()))
}

fn export_dto(r: &SqliteRow) -> Result<LearningPackExportDto> {
    Ok(LearningPackExportDto {
        id: r.get("id"),
        program_id: r.get("program_id"),
        destination_path: r.get("destination_path"),
        manifest: parse(r.get::<String, _>("manifest_json").as_bytes())?,
        created_at: r.get("created_at"),
    })
}

/// What an identical earlier export request wrote, while that file is
/// still there. A pack deleted since is written again under the same ID.
pub(super) async fn replayed_export(
    pool: &SqlitePool,
    req: &ExportLearningPackRequestDto,
    payload: &str,
) -> Result<Option<LearningPackExportDto>> {
    let Some(path) = replay(pool, &req.operation_id, "export", payload).await? else {
        return Ok(None);
    };
    if !Path::new(&path).exists() {
        return Ok(None);
    }
    let row = sqlx::query("SELECT manifest_json,destination_path,created_at,id,program_id FROM learning_pack_exports WHERE operation_id=?")
        .bind(&req.operation_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?;
    row.as_ref().map(export_dto).transpose()
}

/// Record a pack just written to `destination`. Whatever an earlier attempt
/// recorded under the same operation ID points at a pack that is gone, since
/// the replay found no file, and gives way to this one.
pub(super) async fn record_export(
    pool: &SqlitePool,
    req: &ExportLearningPackRequestDto,
    payload: &str,
    destination: String,
    manifest: LearningPackManifest,
) -> Result<LearningPackExportDto> {
    let id = uuid::Uuid::new_v4().to_string();
    let created = now();
    let mut tx = pool.begin().await.map_err(db)?;
    sqlx::query("DELETE FROM learning_pack_exports WHERE operation_id=?")
        .bind(&req.operation_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    pack_operation(&req.operation_id, "export", payload)
        .forget(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO learning_pack_exports(id,program_id,operation_id,payload_hash,format_version,root_sha256,destination_path,privacy_manifest_json,entry_manifest_json,manifest_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(&req.program_id).bind(&req.operation_id).bind(payload).bind(manifest.version as i64).bind(&manifest.root_sha256).bind(&destination).bind(json_string(&manifest.privacy)?).bind(json_string(&manifest.entries)?).bind(json_string(&manifest)?).bind(created).execute(&mut *tx).await.map_err(db)?;
    pack_operation(&req.operation_id, "export", payload)
        .record(&mut *tx, &destination)
        .await?;
    tx.commit().await.map_err(db)?;
    Ok(LearningPackExportDto {
        id,
        program_id: req.program_id.clone(),
        destination_path: destination,
        manifest,
        created_at: created,
    })
}

/// What stays behind when this course is exported, and what would refuse
/// the export right now.
async fn export_notice(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LearningPackExportNoticeDto> {
    let mut stays_behind = Vec::new();
    for (kind, sql) in [
        (
            LearningPackStaysBehindKind::NotebookNotes,
            "SELECT COUNT(*) FROM learning_lesson_note_links WHERE program_id=?",
        ),
        (
            LearningPackStaysBehindKind::PracticalDrafts,
            "SELECT COUNT(*) FROM learning_practical_drafts WHERE program_id=?",
        ),
        (
            LearningPackStaysBehindKind::UnfinishedLessonReviews,
            "SELECT COUNT(*) FROM learning_lesson_reviews WHERE program_id=? AND state<>'ready'",
        ),
        (
            LearningPackStaysBehindKind::TangentChats,
            "SELECT COUNT(*) FROM learning_tangents WHERE program_id=?",
        ),
        (
            LearningPackStaysBehindKind::VerifierConsent,
            "SELECT COUNT(*) FROM learning_programs WHERE id=? AND verifier_audit='on'",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(sql)
            .bind(program_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?;
        if count > 0 {
            stays_behind.push(LearningPackStaysBehindDto {
                kind,
                count: u32::try_from(count).unwrap_or(u32::MAX),
            });
        }
    }
    let mut blockers = Vec::new();
    if active_job_exists(connection, program_id).await? {
        blockers.push(LearningPackConflictKind::ProgramJobs);
    }
    Ok(LearningPackExportNoticeDto {
        stays_behind,
        blockers,
    })
}

pub(super) async fn workspace(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<LearningPortabilityWorkspaceDto> {
    let source_workspace = match LearningSourceLibraryRepository::new(pool.clone())
        .workspace(program_id)
        .await
    {
        Ok(value) => value,
        Err(AppError::NotFound(_)) => crate::features::learning::dto::LearningSourceWorkspaceDto {
            program_id: program_id.into(),
            sources: vec![],
            recheck: None,
        },
        Err(error) => return Err(error),
    };
    let exports = sqlx::query("SELECT manifest_json,destination_path,created_at,id,program_id FROM learning_pack_exports WHERE program_id=? ORDER BY created_at DESC")
            .bind(program_id).fetch_all(pool).await.map_err(db)?.iter().map(export_dto).collect::<Result<Vec<_>>>()?;
    let import_previews = sqlx::query(
        "SELECT * FROM learning_pack_import_previews WHERE program_id=? ORDER BY created_at DESC",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?
    .into_iter()
    .map(|r| -> Result<_> {
        let status = match r.get::<String, _>("status").as_str() {
            "pending" => LearningPackPreviewStatus::Pending,
            "applied" => LearningPackPreviewStatus::Applied,
            "cancelled" => LearningPackPreviewStatus::Cancelled,
            "stale" => LearningPackPreviewStatus::Stale,
            _ => return Err(AppError::Database("Invalid pack preview status".into())),
        };
        let policy = parse_policy(&r.get::<String, _>("conflict_policy"))?;
        let changes: Vec<LearningPackChangeDto> =
            parse(r.get::<String, _>("changes_json").as_bytes())?;
        let conflicts: Vec<LearningPackConflictDto> =
            parse(r.get::<String, _>("conflicts_json").as_bytes())?;
        let warnings: Vec<String> = parse(r.get::<String, _>("warnings_json").as_bytes())?;
        let manifest: LearningPackManifest = parse(r.get::<String, _>("manifest_json").as_bytes())?;
        let can_apply = status == LearningPackPreviewStatus::Pending && !blocks(&conflicts, policy);
        Ok(LearningPackImportPreviewDto {
            id: r.get("id"),
            source_path: r.get("source_path"),
            incoming_program_id: r.get("program_id"),
            incoming_program_title: r.get("program_title"),
            conflict_policy: policy,
            conflicts,
            changes,
            warnings,
            status,
            can_apply,
            created_at: r.get("created_at"),
            decided_at: r.get("decided_at"),
            manifest,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let imports = sqlx::query(
        "SELECT * FROM learning_pack_imports WHERE imported_program_id=? ORDER BY imported_at DESC",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?
    .into_iter()
    .map(|r| -> Result<_> {
        Ok(LearningPackImportResultDto {
            id: r.get("id"),
            preview_id: r.get("preview_id"),
            imported_program_id: r.get("imported_program_id"),
            backup_id: r.get("backup_id"),
            applied_changes: parse(r.get::<String, _>("applied_changes_json").as_bytes())?,
            imported_at: r.get("imported_at"),
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let mut connection = pool.acquire().await.map_err(db)?;
    let export_notice = export_notice(&mut connection, program_id).await?;
    drop(connection);
    Ok(LearningPortabilityWorkspaceDto {
        program_id: program_id.into(),
        exports,
        import_previews,
        imports,
        source_workspace,
        export_notice,
    })
}

/// The program a stored preview would produce.
pub(super) async fn preview_program(pool: &SqlitePool, preview_id: &str) -> Result<String> {
    sqlx::query_scalar("SELECT program_id FROM learning_pack_import_previews WHERE id=?")
        .bind(preview_id)
        .fetch_one(pool)
        .await
        .map_err(db)
}

/// Store a preview; it can be applied only while none of its conflicts
/// stands under its policy. Answers with the program it would produce.
pub(super) async fn record_preview(
    pool: &SqlitePool,
    req: &PreviewLearningPackImportRequestDto,
    payload: &str,
    decoded: &DecodedLearningPack,
    preview: PreviewRecord,
) -> Result<String> {
    let result_program_id = if req.conflict_policy.remaps_ids() {
        uuid::Uuid::new_v4().to_string()
    } else {
        preview.program_id
    };
    let id = req.preview_id.clone();
    let timestamp = now();
    let manifest_json = json_string(&decoded.manifest)?;
    let mut tx = pool.begin().await.map_err(db)?;
    sqlx::query("INSERT INTO learning_pack_import_previews(id,operation_id,payload_hash,source_path,root_sha256,program_id,program_title,conflict_policy,conflicts_json,changes_json,warnings_json,manifest_json,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'pending',?)")
        .bind(&id).bind(&req.operation_id).bind(payload).bind(&req.source_path).bind(&decoded.manifest.root_sha256).bind(&result_program_id).bind(&preview.title).bind(policy_column(req.conflict_policy)?).bind(json_string(&preview.conflicts)?).bind(json_string(&preview.changes)?).bind(json_string(&preview.warnings)?).bind(&manifest_json).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
    pack_operation(&req.operation_id, "preview_import", payload)
        .record(&mut *tx, &id)
        .await?;
    tx.commit().await.map_err(db)?;
    Ok(result_program_id)
}

/// A stored preview that may be applied.
pub(super) struct PendingPreview {
    pub(super) source_path: String,
    pub(super) root_sha256: String,
    pub(super) target_id: String,
    pub(super) policy: LearningPackConflictPolicy,
    pub(super) changes: Vec<LearningPackChangeDto>,
}

/// The preview `preview_id`, while it is pending and nothing blocks it.
pub(super) async fn pending_preview(pool: &SqlitePool, preview_id: &str) -> Result<PendingPreview> {
    let row = sqlx::query("SELECT * FROM learning_pack_import_previews WHERE id=?")
        .bind(preview_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Pack preview not found".into()))?;
    if row.get::<String, _>("status") != "pending" {
        return Err(invalid("Only a pending pack preview can be applied."));
    }
    let policy = parse_policy(&row.get::<String, _>("conflict_policy"))?;
    let conflicts: Vec<LearningPackConflictDto> =
        parse(row.get::<String, _>("conflicts_json").as_bytes())?;
    if blocks(&conflicts, policy) {
        return Err(invalid(
            "This import preview contains unresolved conflicts and cannot be applied.",
        ));
    }
    Ok(PendingPreview {
        source_path: row.get("source_path"),
        root_sha256: row.get("root_sha256"),
        target_id: row.get("program_id"),
        policy,
        changes: parse(row.get::<String, _>("changes_json").as_bytes())?,
    })
}

/// Record an applied import in its own transaction: the replacement's
/// backup, the import, the preview's decision, and the ledger entry.
pub(super) async fn record_import(
    connection: &mut SqliteConnection,
    req: &ApplyLearningPackImportRequestDto,
    payload: &str,
    program_id: &str,
    backup: Option<&BackupExportRecord>,
    changes: &[LearningPackChangeDto],
) -> Result<()> {
    if let Some(backup) = backup {
        // If restoration fails, the old program rolls back and the durable
        // backup file remains as an additional recovery copy.
        sqlx::query("INSERT INTO learning_pack_exports(id,program_id,operation_id,payload_hash,format_version,root_sha256,destination_path,privacy_manifest_json,entry_manifest_json,manifest_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&backup.id).bind(&backup.program_id).bind(&backup.operation_id).bind(&backup.payload_hash).bind(backup.format_version).bind(&backup.root_sha256).bind(&backup.destination_path).bind(&backup.privacy_manifest_json).bind(&backup.entry_manifest_json).bind(&backup.manifest_json).bind(backup.created_at).execute(&mut *connection).await.map_err(db)?;
    }
    let result_id = uuid::Uuid::new_v4().to_string();
    let timestamp = now();
    sqlx::query("INSERT INTO learning_pack_imports(id,preview_id,operation_id,payload_hash,imported_program_id,backup_id,applied_changes_json,imported_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(&result_id).bind(&req.preview_id).bind(&req.operation_id).bind(payload).bind(program_id).bind(backup.map(|x|x.id.as_str())).bind(json_string(&changes)?).bind(timestamp).execute(&mut *connection).await.map_err(db)?;
    sqlx::query(
        "UPDATE learning_pack_import_previews SET status='applied',decided_at=? WHERE id=?",
    )
    .bind(timestamp)
    .bind(&req.preview_id)
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    pack_operation(&req.operation_id, "apply_import", payload)
        .record(&mut *connection, &result_id)
        .await
}
