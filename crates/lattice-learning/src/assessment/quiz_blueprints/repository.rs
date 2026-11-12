//! The quiz blueprints' statements: the course's quiz settings, the facts a blueprint is derived from, and the
//! stored blueprint revisions with their slots.

use crate::dto::LearningQuizSettingsDto;
use crate::persistence::db;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use sqlx::{Row, SqliteConnection};

/// A lesson of the course as its rows keep it.
pub(super) struct LessonRow {
    pub(super) id: String,
    pub(super) module_id: String,
    pub(super) title: String,
    pub(super) objective: String,
    pub(super) preparation: String,
}

/// A slot as (outcome, format, difficulty, difficulty_max, points in thousandths).
pub(super) type SlotRow = (String, String, i64, i64, i64);

/// A stored blueprint revision's own columns.
pub(super) struct StoredBlueprint {
    pub(super) title: String,
    pub(super) passing_score: f64,
    pub(super) expected_minutes: i64,
}

/// A blueprint revision to store.
pub(super) struct NewBlueprint<'a> {
    pub(super) id: &'a str,
    pub(super) program_id: &'a str,
    pub(super) revision: i64,
    pub(super) predecessor: Option<i64>,
    pub(super) purpose: &'a str,
    pub(super) title: &'a str,
    pub(super) instructions: &'a str,
    pub(super) expected_minutes: i64,
    pub(super) passing_score: f64,
    pub(super) source_version_ids_json: String,
    pub(super) change_reason: &'a str,
    pub(super) created_at: i64,
    pub(super) scope_kind: &'a str,
    pub(super) scope_id: &'a Option<String>,
}

/// A slot of a blueprint revision to store.
pub(super) struct NewSlot<'a> {
    pub(super) blueprint_id: &'a str,
    pub(super) revision: i64,
    pub(super) ordinal: i64,
    pub(super) outcome_id: &'a str,
    pub(super) format: &'a str,
    pub(super) difficulty: i64,
    pub(super) difficulty_max: i64,
    pub(super) points: f64,
}

/// The course's saved quiz settings, if it has any.
pub(super) async fn quiz_settings_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<LearningQuizSettingsDto>> {
    let row = sqlx::query("SELECT * FROM learning_quiz_settings WHERE program_id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    Ok(row.map(|row| LearningQuizSettingsDto {
        quiz_pass: row.get("quiz_pass"),
        test_pass: row.get("test_pass"),
        exam_pass: row.get("exam_pass"),
        written_answers: row.get::<i64, _>("written_answers") != 0,
    }))
}

/// The lesson's outcome, the earliest made.
pub(crate) async fn lesson_outcome_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=? ORDER BY created_at,id LIMIT 1",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)
}

/// The lesson's title, objective and ordinal, when it is one of the course's.
pub(super) async fn lesson_heading_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<(String, String, i64)>> {
    let row = sqlx::query(
        "SELECT title,objective,ordinal FROM learning_lessons WHERE id=? AND program_id=?",
    )
    .bind(lesson_id)
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(row.map(|lesson| {
        (
            lesson.get::<String, _>("title"),
            lesson.get::<String, _>("objective"),
            lesson.get::<i64, _>("ordinal"),
        )
    }))
}

/// A lesson-level outcome to store.
pub(super) struct NewLessonOutcome<'a> {
    pub(super) id: &'a str,
    pub(super) program_id: &'a str,
    pub(super) lesson_id: &'a str,
    pub(super) title: &'a str,
    pub(super) description: &'a str,
    pub(super) ordinal: i64,
    pub(super) created_at: i64,
}

/// Stores a lesson-level outcome.
pub(super) async fn insert_lesson_outcome_in(
    tx: &mut WriteTx,
    outcome: &NewLessonOutcome<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,NULL,?,?,?,?,?)")
        .bind(outcome.id)
        .bind(outcome.program_id)
        .bind(outcome.lesson_id)
        .bind(outcome.title)
        .bind(outcome.description)
        .bind(outcome.ordinal)
        .bind(outcome.created_at)
        .execute(&mut **tx)
        .await
        .map(drop)
        .map_err(db)
}

/// The course's title, when the course is there.
pub(super) async fn program_title_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)
}

/// The ids of the course's source versions.
pub(super) async fn source_version_ids_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT id FROM learning_source_versions WHERE program_id=?")
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)
}

/// The course's modules as (id, title), in order.
pub(super) async fn modules_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, String)>> {
    let rows =
        sqlx::query("SELECT id,title FROM learning_modules WHERE program_id=? ORDER BY ordinal")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("title")))
        .collect())
}

/// The ids of a module's own outcomes, in order.
pub(super) async fn module_outcome_ids_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    module_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND lesson_id IS NULL ORDER BY ordinal,id")
        .bind(program_id)
        .bind(module_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)
}

/// The course's lessons other than replaced or skipped ones, in course order.
pub(super) async fn lessons_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LessonRow>> {
    let rows = sqlx::query("SELECT l.id,l.module_id,l.title,l.objective,l.preparation FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY m.ordinal,l.ordinal")
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| LessonRow {
            id: row.get("id"),
            module_id: row.get("module_id"),
            title: row.get("title"),
            objective: row.get("objective"),
            preparation: row.get("preparation"),
        })
        .collect())
}

/// The source ids each block of the lesson cites, as stored.
pub(super) async fn block_source_ids_json_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT source_ids_json FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)
}

/// The id and revision of the scope's latest accepted blueprint.
pub(super) async fn latest_accepted_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope_kind: &str,
    scope_id: &Option<String>,
) -> Result<Option<(String, i64)>> {
    let row = sqlx::query("SELECT id,revision FROM learning_assessment_blueprints WHERE program_id=? AND scope_kind=? AND scope_id IS ? AND status='accepted' ORDER BY revision DESC LIMIT 1")
        .bind(program_id)
        .bind(scope_kind)
        .bind(scope_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    Ok(row.map(|row| (row.get("id"), row.get("revision"))))
}

/// A stored revision's own columns.
pub(super) async fn stored_blueprint_in(
    connection: &mut SqliteConnection,
    id: &str,
    revision: i64,
) -> Result<StoredBlueprint> {
    let row = sqlx::query("SELECT title,passing_score,expected_minutes FROM learning_assessment_blueprints WHERE id=? AND revision=?")
        .bind(id)
        .bind(revision)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)?;
    Ok(StoredBlueprint {
        title: row.get("title"),
        passing_score: row.get("passing_score"),
        expected_minutes: row.get("expected_minutes"),
    })
}

/// A stored revision's slots as (outcome, format, difficulty, difficulty_max, points in thousandths).
pub(super) async fn stored_slots_in(
    connection: &mut SqliteConnection,
    id: &str,
    revision: i64,
) -> Result<Vec<SlotRow>> {
    Ok(sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max,points FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal")
        .bind(id)
        .bind(revision)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .into_iter()
        .map(|r| {
            (
                r.get::<String, _>("outcome_id"),
                r.get::<String, _>("format"),
                r.get::<i64, _>("difficulty"),
                r.get::<i64, _>("difficulty_max"),
                (r.get::<f64, _>("points") * 1000.0).round() as i64,
            )
        })
        .collect::<Vec<_>>())
}

/// A stored revision's slots as (outcome, format, difficulty, difficulty_max), in order.
pub(super) async fn slot_requirements_in(
    connection: &mut SqliteConnection,
    id: &str,
    revision: i64,
) -> Result<Vec<(String, String, i64, i64)>> {
    let rows = sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal")
        .bind(id)
        .bind(revision)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("outcome_id"),
                row.get("format"),
                row.get("difficulty"),
                row.get("difficulty_max"),
            )
        })
        .collect())
}

/// Stores a blueprint revision.
pub(super) async fn insert_blueprint_in(
    tx: &mut WriteTx,
    blueprint: &NewBlueprint<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at,scope_kind,scope_id) VALUES(?,?,?,?,?,?,?,?,'[]',?,'[]','after_submission',?,'accepted',?,?,?,?)")
        .bind(blueprint.id)
        .bind(blueprint.program_id)
        .bind(blueprint.revision)
        .bind(blueprint.predecessor)
        .bind(blueprint.purpose)
        .bind(blueprint.title)
        .bind(blueprint.instructions)
        .bind(blueprint.expected_minutes)
        .bind(blueprint.passing_score)
        .bind(&blueprint.source_version_ids_json)
        .bind(blueprint.change_reason)
        .bind(blueprint.created_at)
        .bind(blueprint.scope_kind)
        .bind(blueprint.scope_id)
        .execute(&mut **tx)
        .await
        .map(drop)
        .map_err(db)
}

/// Stores a slot of a blueprint revision.
pub(super) async fn insert_slot_in(tx: &mut WriteTx, slot: &NewSlot<'_>) -> Result<()> {
    sqlx::query("INSERT INTO learning_assessment_blueprint_slots(blueprint_id,blueprint_revision,ordinal,outcome_id,format,difficulty,difficulty_max,points) VALUES(?,?,?,?,?,?,?,?)")
        .bind(slot.blueprint_id)
        .bind(slot.revision)
        .bind(slot.ordinal)
        .bind(slot.outcome_id)
        .bind(slot.format)
        .bind(slot.difficulty)
        .bind(slot.difficulty_max)
        .bind(slot.points)
        .execute(&mut **tx)
        .await
        .map(drop)
        .map_err(db)
}
