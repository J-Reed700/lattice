//! Replacement backups must describe exactly the database state being removed.
use super::{
    db, export,
    files::{atomic_write, managed_path},
    guards, hash, invalid, json_string, now,
    records::BackupExportRecord,
};
use crate::features::learning::{
    pack::{decode_learning_pack, encode_learning_pack},
    portability_dto::{ExportLearningPackRequestDto, LearningPackExportOptionsDto},
};
use lattice_core::shared::error::Result;
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};
use std::path::Path;

#[cfg(test)]
#[path = "replacement_tests.rs"]
mod tests;

pub(super) async fn begin_apply(
    pool: &SqlitePool,
    managed_dir: &Path,
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
    // A job may have started after the preview; deleting the course now would
    // remove its rows from under the worker.
    if replaces_program && guards::active_job_exists(&mut tx, program_id).await? {
        return Err(invalid(guards::PREPARING));
    }
    let backup = if replaces_program {
        backup_if_present(&mut tx, managed_dir, program_id).await?
    } else {
        None
    };
    Ok((tx, backup))
}

pub(super) async fn require_recoverable_on(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<()> {
    match unrecoverable_reason(connection, program_id).await? {
        Some(reason) => Err(invalid(reason)),
        None => Ok(()),
    }
}

/// Why replacing this course would lose data no pack can restore, in words
/// the preview shows as they are.
pub(super) async fn unrecoverable_reason(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<String>> {
    // Derived retrieval chunks are rebuildable; hidden evaluator files are
    // not, and no pack carries them. Lesson note links are re-attached in the
    // replacing transaction, and runtime profile references survive whenever
    // the profile exists on this computer. Check under the replacement lock
    // as well as during preview.
    const OMITTED: &[(&str, &str)] = &[
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
            return Ok(Some(format!("Replace was blocked because the target has {label} that this pack version cannot restore. The existing program was left unchanged.")));
        }
    }
    Ok(None)
}

/// Lesson notes live in this computer's notebook, outside any pack. Taken
/// before the replaced course is deleted, which cascades its links away.
pub(super) struct NoteLinks {
    journal_id: Option<String>,
    links: Vec<(String, String, i64)>,
}

pub(super) async fn snapshot_note_links(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<NoteLinks> {
    let journal_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT journal_id FROM learning_memory WHERE program_id=?",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    .flatten();
    let links = sqlx::query(
        "SELECT lesson_id,note_id,created_at FROM learning_lesson_note_links WHERE program_id=? ORDER BY rowid",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?
    .into_iter()
    .map(|row| (row.get("lesson_id"), row.get("note_id"), row.get("created_at")))
    .collect();
    Ok(NoteLinks { journal_id, links })
}

/// Re-attach notes to every lesson the imported course still has, and give the
/// course back its notebook when the pack's memory record has none.
pub(super) async fn restore_note_links(
    connection: &mut SqliteConnection,
    program_id: &str,
    saved: NoteLinks,
) -> Result<()> {
    if saved.journal_id.is_none() && saved.links.is_empty() {
        return Ok(());
    }
    sqlx::query("INSERT OR IGNORE INTO learning_memory(program_id,journal_id,deck_id,created_at) VALUES(?,NULL,NULL,?)")
        .bind(program_id)
        .bind(now())
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    if let Some(journal_id) = &saved.journal_id {
        sqlx::query("UPDATE learning_memory SET journal_id=? WHERE program_id=? AND journal_id IS NULL AND EXISTS(SELECT 1 FROM journals WHERE id=?)")
            .bind(journal_id)
            .bind(program_id)
            .bind(journal_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    for (lesson_id, note_id, created_at) in saved.links {
        sqlx::query("INSERT OR IGNORE INTO learning_lesson_note_links(program_id,lesson_id,note_id,created_at) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=? AND id=?) AND EXISTS(SELECT 1 FROM daily_notes_workspace WHERE id=?)")
            .bind(program_id)
            .bind(&lesson_id)
            .bind(&note_id)
            .bind(created_at)
            .bind(program_id)
            .bind(&lesson_id)
            .bind(&note_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    Ok(())
}

/// Requires the caller's BEGIN IMMEDIATE transaction. Do not acquire another
/// pooled connection here: other saves must wait until replacement commits or
/// rolls back. The durable file is written before any program rows are deleted.
async fn backup_if_present(
    connection: &mut SqliteConnection,
    managed_dir: &Path,
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
        options: LearningPackExportOptionsDto {
            include_evidence: true,
            include_practical_artifacts: true,
            include_source_bodies: true,
            source_body_redistribution_confirmed: true,
        },
    };
    let snapshot = export::snapshot_on(connection, program_id, &request.options).await?;
    let bytes = encode_learning_pack(snapshot)?;
    let decoded = decode_learning_pack(&bytes)?;
    let path = managed_path(managed_dir, &format!("backup-{id}")).await?;
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
