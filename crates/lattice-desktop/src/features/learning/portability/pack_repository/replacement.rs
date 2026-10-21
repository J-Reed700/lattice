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

/// Unfinished lesson reviews on this computer, which a replace discards: a
/// review that is not finished keeps its draft in its job's checkpoints, and
/// neither the pack nor the backup carries those. Said, not refused, so a
/// course with a failed lesson can still come back.
pub(super) async fn replace_warnings(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    let unfinished: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM learning_lesson_reviews WHERE program_id=? AND state<>'ready'",
    )
    .bind(program_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    Ok(match unfinished {
        0 => Vec::new(),
        1 => vec!["Replacing discards 1 unfinished lesson review on this computer; that lesson will need preparing again.".into()],
        count => vec![format!("Replacing discards {count} unfinished lesson reviews on this computer; those lessons will need preparing again.")],
    })
}

/// A tangent chat's place in a lesson: conversation, lesson, section index
/// and title, the selected passage, its context size and when it began.
type TangentLink = (
    String,
    String,
    Option<i64>,
    Option<String>,
    String,
    i64,
    i64,
);

/// The course's Verifier audit switch, web search and the consent given on
/// this computer, with its provider.
type VerifierConsent = (String, i64, Option<i64>, Option<String>);

/// What lives on this computer outside any pack: lesson notes in the
/// notebook, tangent chats, and the Verifier audit consent given here.
/// Taken before the replaced course is deleted, which cascades it away.
pub(super) struct LocalLinks {
    journal_id: Option<String>,
    notes: Vec<(String, String, i64)>,
    tangents: Vec<TangentLink>,
    verifier: Option<VerifierConsent>,
}

pub(super) async fn snapshot_local_links(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LocalLinks> {
    let journal_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT journal_id FROM learning_memory WHERE program_id=?",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    .flatten();
    let notes = sqlx::query(
        "SELECT lesson_id,note_id,created_at FROM learning_lesson_note_links WHERE program_id=? ORDER BY rowid",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?
    .into_iter()
    .map(|row| (row.get("lesson_id"), row.get("note_id"), row.get("created_at")))
    .collect();
    let tangents = sqlx::query_as(
        "SELECT conversation_id,lesson_id,section_index,section_title,selected_text,context_message_count,created_at FROM learning_tangents WHERE program_id=? ORDER BY created_at,conversation_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let verifier = sqlx::query_as(
        "SELECT verifier_audit,verifier_web_search,verifier_consent_at,verifier_consent_provider FROM learning_programs WHERE id=?",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(LocalLinks {
        journal_id,
        notes,
        tangents,
        verifier,
    })
}

/// Put back onto the imported course what this computer held for the one it
/// replaced: notes and tangent chats on every lesson it still has, its
/// notebook when the pack's memory record has none, and the Verifier consent.
pub(super) async fn restore_local_links(
    connection: &mut SqliteConnection,
    program_id: &str,
    saved: LocalLinks,
) -> Result<()> {
    if saved.journal_id.is_some() || !saved.notes.is_empty() {
        sqlx::query("INSERT OR IGNORE INTO learning_memory(program_id,journal_id,deck_id,created_at) VALUES(?,NULL,NULL,?)")
            .bind(program_id)
            .bind(now())
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    if let Some(journal_id) = &saved.journal_id {
        sqlx::query("UPDATE learning_memory SET journal_id=? WHERE program_id=? AND journal_id IS NULL AND EXISTS(SELECT 1 FROM journals WHERE id=?)")
            .bind(journal_id)
            .bind(program_id)
            .bind(journal_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    for (lesson_id, note_id, created_at) in saved.notes {
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
    // The lesson's module comes from the imported course, where it may have moved.
    for (
        conversation_id,
        lesson_id,
        section_index,
        section_title,
        selected_text,
        context,
        created_at,
    ) in saved.tangents
    {
        sqlx::query("INSERT OR IGNORE INTO learning_tangents(conversation_id,program_id,module_id,lesson_id,section_index,section_title,selected_text,context_message_count,created_at) SELECT ?,l.program_id,l.module_id,l.id,?,?,?,?,? FROM learning_lessons l WHERE l.program_id=? AND l.id=? AND EXISTS(SELECT 1 FROM conversations WHERE id=?)")
            .bind(&conversation_id)
            .bind(section_index)
            .bind(&section_title)
            .bind(&selected_text)
            .bind(context)
            .bind(created_at)
            .bind(program_id)
            .bind(&lesson_id)
            .bind(&conversation_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    if let Some((audit, web_search, consent_at, provider)) = saved.verifier {
        sqlx::query("UPDATE learning_programs SET verifier_audit=?,verifier_web_search=?,verifier_consent_at=?,verifier_consent_provider=? WHERE id=?")
            .bind(audit)
            .bind(web_search)
            .bind(consent_at)
            .bind(provider)
            .bind(program_id)
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
