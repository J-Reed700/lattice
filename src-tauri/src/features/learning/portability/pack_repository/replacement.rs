//! Replacement backups must describe exactly the database state being removed.
use super::{
    atomic_write, db, decode_learning_pack, encode_learning_pack, export, hash, invalid,
    json_string, managed_path, now, BackupExportRecord, ExportLearningPackRequestDto,
};
use crate::{interfaces::di::Container, shared::error::Result};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

#[cfg(test)]
#[path = "replacement_tests.rs"]
mod tests;

pub(super) async fn begin_apply(
    pool: &SqlitePool,
    container: &Container,
    preview_id: &str,
    program_id: &str,
    replaces_program: bool,
) -> Result<(Transaction<'static, Sqlite>, Option<BackupExportRecord>)> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    sqlx::query("PRAGMA defer_foreign_keys=ON")
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let pending = sqlx::query_scalar::<_, String>(
        "SELECT status FROM learning_pack_import_previews WHERE id=? AND status='pending'",
    )
    .bind(preview_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db)?;
    if pending.is_none() {
        return Err(invalid("Pack preview changed while it was being applied."));
    }
    let backup = if replaces_program {
        backup_if_present(&mut tx, container, program_id).await?
    } else {
        None
    };
    Ok((tx, backup))
}

pub(super) async fn require_recoverable_on(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<()> {
    // Derived retrieval chunks are rebuildable; these optional aggregates are
    // not. Check under the replacement lock as well as during preview.
    const OMITTED: &[(&str, &str)] = &[
        ("notebook links", "SELECT COUNT(*) FROM learning_lesson_note_links WHERE program_id=?"),
        ("runtime profiles", "SELECT COUNT(DISTINCT runtime_profile_id) FROM learning_practical_activities WHERE program_id=? AND runtime_profile_id IS NOT NULL"),
        ("hidden practical evaluators", "SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')"),
    ];
    for (label, sql) in OMITTED {
        if sqlx::query_scalar::<_, i64>(sql)
            .bind(program_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?
            > 0
        {
            return Err(invalid(format!("Replace was blocked because the target has {label} that this pack version cannot restore. The existing program was left unchanged.")));
        }
    }
    Ok(())
}

/// Requires the caller's BEGIN IMMEDIATE transaction. Do not acquire another
/// pooled connection here: other saves must wait until replacement commits or
/// rolls back. The durable file is written before any program rows are deleted.
async fn backup_if_present(
    connection: &mut SqliteConnection,
    container: &Container,
    program_id: &str,
) -> Result<Option<BackupExportRecord>> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    if exists.is_none() {
        return Ok(None);
    }
    require_recoverable_on(connection, program_id).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let request = ExportLearningPackRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: program_id.into(),
        file_name: format!("backup-{id}.lattice-learning"),
        include_evidence: true,
        include_practical_artifacts: true,
        include_source_bodies: true,
        source_body_redistribution_confirmed: true,
    };
    let snapshot = export::snapshot_on(connection, &request).await?;
    let bytes = encode_learning_pack(snapshot)?;
    let decoded = decode_learning_pack(&bytes)?;
    let path = managed_path(container, &request.file_name).await?;
    atomic_write(&path, &bytes).await?;
    Ok(Some(BackupExportRecord {
        id,
        program_id: program_id.into(),
        payload_hash: hash(&request)?,
        operation_id: request.operation_id,
        format_version: i64::from(decoded.manifest.version),
        root_sha256: decoded.manifest.root_sha256.clone(),
        destination_path: path.to_string_lossy().into_owned(),
        privacy_manifest_json: json_string(&decoded.manifest.privacy)?,
        entry_manifest_json: json_string(&decoded.manifest.entries)?,
        manifest_json: json_string(&decoded.manifest)?,
        created_at: now(),
    }))
}
