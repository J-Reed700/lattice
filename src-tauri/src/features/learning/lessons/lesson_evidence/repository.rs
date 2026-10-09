//! Persistence adapter for the learner-safe evidence projection.
use crate::features::learning::{
    content_verification::LessonVerificationReport, dto::LearningLessonDto,
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

#[derive(sqlx::FromRow)]
pub(in crate::features::learning) struct SavedEvidenceVersion {
    pub id: String,
    pub title: String,
    pub resolved_url: Option<String>,
    pub requested_url: Option<String>,
    pub content_sha256: String,
    pub active_version_id: Option<String>,
    pub deleted_at: Option<i64>,
}

pub(super) struct EvidenceSnapshot {
    pub lesson: LearningLessonDto,
    pub report: Option<LessonVerificationReport>,
    pub versions: Vec<SavedEvidenceVersion>,
}

pub(super) async fn read(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
) -> Result<EvidenceSnapshot> {
    let mut tx = pool.begin().await?;
    let program = LearningRepository::get_on(&mut tx, program_id).await?;
    let lesson = program
        .modules
        .into_iter()
        .flat_map(|module| module.lessons)
        .find(|lesson| lesson.id == lesson_id)
        .ok_or_else(|| AppError::NotFound("Lesson not found in this course.".into()))?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut *tx)
    .await?;
    let report = raw.map(|raw| serde_json::from_str::<LessonVerificationReport>(&raw)
        .map_err(|_| AppError::InvalidState("Saved lesson evidence is unavailable: the verification report is invalid or uses an unsupported format.".into())))
        .transpose()?;
    let versions = if report.is_some() {
        sqlx::query_as::<_, SavedEvidenceVersion>(
            "SELECT v.id,v.title,v.resolved_url,v.requested_url,v.content_sha256,s.active_version_id,s.deleted_at FROM learning_source_versions v JOIN learning_source_library s ON s.program_id=v.program_id AND s.id=v.source_id WHERE v.program_id=?"
        ).bind(program_id).fetch_all(&mut *tx).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(EvidenceSnapshot {
        lesson,
        report,
        versions,
    })
}
