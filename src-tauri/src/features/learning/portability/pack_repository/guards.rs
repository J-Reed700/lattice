//! Preconditions for moving a course: no generation job may be running under
//! an export or a replacement, an export needs a course worth importing, and
//! a pack only imports into the same database schema that wrote it.
use super::{
    db, invalid, parse, replacement::unrecoverable_reason, validate_uuid,
    validation::validate_program, LearningPackRepository, PreviewRecord, PROGRAM_ENTRY,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::DecodedLearningPack,
    portability_dto::{LearningPackConflictDto, LearningPackExportNoticeDto},
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

/// A pending or running lesson job for this course. Replacing the course
/// would delete the job's rows under its worker; exporting would capture a
/// half-prepared course.
pub(super) async fn active_job_exists(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs j JOIN learning_jobs lj ON lj.job_id=j.id WHERE lj.program_id=? AND j.status IN ('pending','running'))")
        .bind(program_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)
}

pub(super) const PREPARING: &str =
    "This course is still being prepared. Export or replace it after its lessons finish generating.";

/// What stays behind when this course is exported.
pub(super) async fn export_notice(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LearningPackExportNoticeDto> {
    let active_job = active_job_exists(connection, program_id).await?;
    let notebook_note_count =
        sqlx::query_scalar("SELECT COUNT(*) FROM learning_lesson_note_links WHERE program_id=?")
            .bind(program_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?;
    let practical_draft_count =
        sqlx::query_scalar("SELECT COUNT(*) FROM learning_practical_drafts WHERE program_id=?")
            .bind(program_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?;
    Ok(LearningPackExportNoticeDto {
        active_job,
        notebook_note_count,
        practical_draft_count,
    })
}

/// Refuse an export the receiving computer could not import. Evidence
/// tables over the import's row cap are refused as they are encoded, which
/// covers replacement backups too.
pub(super) async fn require_exportable(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<()> {
    if active_job_exists(connection, program_id).await? {
        return Err(invalid(PREPARING));
    }
    let program = LearningRepository::get_on(connection, program_id).await?;
    if program.modules.is_empty() {
        return Err(invalid(
            "This course has no modules yet. Finish its outline before exporting it.",
        ));
    }
    validate_program(&program)
}

/// Why a replacement of `program` cannot go ahead right now, as blocked
/// preview conflicts: data no pack can restore, or a job still running.
pub(super) async fn replace_conflicts(
    connection: &mut SqliteConnection,
    program: &LearningProgramDto,
) -> Result<Vec<LearningPackConflictDto>> {
    let blocked = |entity_kind: &str, reason: String| LearningPackConflictDto {
        entity_kind: entity_kind.into(),
        incoming_id: program.summary.id.clone(),
        incoming_title: program.summary.title.clone(),
        existing_id: program.summary.id.clone(),
        existing_title: reason,
        resolution: "blocked".into(),
    };
    let mut conflicts = Vec::new();
    if let Some(reason) = unrecoverable_reason(connection, &program.summary.id).await? {
        conflicts.push(blocked("program_history", reason));
    }
    if active_job_exists(connection, &program.summary.id).await? {
        conflicts.push(blocked("program_jobs", PREPARING.into()));
    }
    Ok(conflicts)
}

/// A pack from another build of Lattice may describe its data differently,
/// so it is shown as blocked before anything else in it is trusted. The
/// course identity is read loosely: only enough to name it in the preview.
fn schema_conflict(
    pack: &DecodedLearningPack,
    local_schema: i64,
) -> Result<Option<LearningPackConflictDto>> {
    if pack.manifest.schema_version == local_schema {
        return Ok(None);
    }
    let raw: serde_json::Value = parse(
        pack.entries
            .get(PROGRAM_ENTRY)
            .ok_or_else(|| invalid("Pack is missing its program."))?,
    )?;
    let id = raw
        .pointer("/summary/id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("Pack program has no ID."))?;
    validate_uuid(id, "program")?;
    let title = raw
        .pointer("/summary/title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&pack.manifest.title);
    Ok(Some(LearningPackConflictDto {
        entity_kind: "pack_schema".into(),
        incoming_id: id.into(),
        incoming_title: title.into(),
        existing_id: local_schema.to_string(),
        existing_title: format!(
            "This pack was made by a different build of Lattice (database schema {}; this computer has {local_schema}). Install the same Lattice version on both computers, then export the course again.",
            pack.manifest.schema_version
        ),
        resolution: "blocked".into(),
    }))
}

/// The highest applied migration. Two computers on the same build agree.
pub(super) async fn schema_version(connection: &mut SqliteConnection) -> Result<i64> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut *connection)
        .await
        .map_err(db)?;
    version
        .ok_or_else(|| AppError::Database("This database has no recorded schema version.".into()))
}

impl LearningPackRepository {
    /// Refuse before asking the user where to save a course that cannot go.
    pub async fn check_exportable(&self, program_id: &str) -> Result<()> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        require_exportable(&mut connection, program_id).await
    }

    /// The blocked preview of a pack written by another build, if it is one.
    pub(super) async fn schema_mismatch(
        &self,
        pack: &DecodedLearningPack,
    ) -> Result<Option<PreviewRecord>> {
        let local = schema_version(&mut *self.pool.acquire().await.map_err(db)?).await?;
        Ok(schema_conflict(pack, local)?.map(|conflict| PreviewRecord {
            program_id: conflict.incoming_id.clone(),
            title: conflict.incoming_title.clone(),
            conflicts: vec![conflict],
            changes: vec![],
            warnings: vec![],
        }))
    }

    /// Apply checks the stamp again: a preview never authorizes changed data.
    pub(super) async fn require_same_schema(&self, pack: &DecodedLearningPack) -> Result<()> {
        let local = schema_version(&mut *self.pool.acquire().await.map_err(db)?).await?;
        match schema_conflict(pack, local)? {
            Some(conflict) => Err(invalid(conflict.existing_title)),
            None => Ok(()),
        }
    }
}
