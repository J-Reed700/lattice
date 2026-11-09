//! The schedules' statements: the pattern and lesson schedule rows, what is waiting for a grade, what the lessons
//! stand at, and the records of which lessons fit which material.

use crate::persistence::db;
use crate::recall::schedule::CardSchedule;
use futures::FutureExt;
use lattice_core::shared::error::Result;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

/// The columns of a schedule row, after its key.
const SCHEDULE_COLUMNS: &str =
    "stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses";

/// A table of schedules and the two columns that key a row.
pub(super) struct Table {
    name: &'static str,
    keys: &'static str,
}

/// One schedule for each pattern of a material.
pub(super) const PATTERNS: Table = Table {
    name: "learning_drill_pattern_schedules",
    keys: "material,pattern_id",
};

/// One schedule for each lesson of a course.
pub(super) const LESSONS: Table = Table {
    name: "learning_drill_lesson_schedules",
    keys: "program_id,lesson_id",
};

/// A lesson of a course as (lesson id, completed, current, due at).
pub(super) type LessonRow = (String, i64, i64, Option<i64>);

/// A block of a lesson as (ordinal, kind, title, body, source ids as stored).
pub(super) type BlockRow = (i64, String, String, String, String);

/// The memory of a reviewed schedule.
pub(super) struct Memory {
    pub(super) stability: f64,
    pub(super) difficulty: f64,
    pub(super) reviewed: i64,
}

fn schedule_of(row: &SqliteRow) -> CardSchedule {
    CardSchedule {
        stability: Some(row.get("stability")),
        difficulty: Some(row.get("difficulty")),
        last_reviewed_at: Some(row.get("last_reviewed_at")),
        due_at: row.get("due_at"),
        interval_days: row.get("interval_days"),
        review_count: row.get("review_count"),
        lapses: row.get("lapses"),
    }
}

/// The schedule at `key`, if there is one.
pub(super) async fn schedule_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
) -> Result<Option<CardSchedule>> {
    let row = sqlx::query(&format!(
        "SELECT {SCHEDULE_COLUMNS} FROM {} WHERE ({}) = (?,?)",
        table.name, table.keys
    ))
    .bind(key.0)
    .bind(key.1)
    .fetch_optional(&mut *connection)
    .boxed()
    .await
    .map_err(db)?;
    Ok(row.as_ref().map(schedule_of))
}

/// Writes a reviewed schedule at `key`, replacing the one there.
pub(super) async fn write_schedule_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
    memory: &Memory,
    schedule: &CardSchedule,
) -> Result<()> {
    sqlx::query(&format!(
        "INSERT INTO {name}({keys},{SCHEDULE_COLUMNS}) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT({keys}) DO UPDATE SET stability=excluded.stability,difficulty=excluded.difficulty,last_reviewed_at=excluded.last_reviewed_at,due_at=excluded.due_at,interval_days=excluded.interval_days,review_count=excluded.review_count,lapses=excluded.lapses",
        name = table.name,
        keys = table.keys
    ))
    .bind(key.0)
    .bind(key.1)
    .bind(memory.stability)
    .bind(memory.difficulty)
    .bind(memory.reviewed)
    .bind(schedule.due_at)
    .bind(schedule.interval_days)
    .bind(schedule.review_count)
    .bind(schedule.lapses)
    .execute(&mut *connection)
    .await
    .map(drop)
    .map_err(db)
}

/// The patterns of a material that have a schedule, as (pattern id, due at), by pattern id.
pub(super) async fn scheduled_patterns_in(
    connection: &mut SqliteConnection,
    material: &str,
) -> Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "SELECT pattern_id,due_at FROM learning_drill_pattern_schedules WHERE material=? ORDER BY pattern_id",
    )
    .bind(material)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The patterns planted by a drill of the material that is ready and built from a lesson, or whose attempt is submitted
/// and not yet graded.
pub(super) async fn held_patterns_in(
    connection: &mut SqliteConnection,
    material: &str,
    language: Option<&str>,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT DISTINCT p.pattern_id FROM learning_drill_patterns p JOIN learning_drills d ON d.id=p.drill_id WHERE d.material=?1 AND d.language IS ?2 AND ((d.status='ready' AND d.lesson_id IS NOT NULL) OR EXISTS(SELECT 1 FROM learning_drill_attempts a WHERE a.drill_id=d.id AND a.status='submitted')) ORDER BY p.pattern_id",
    )
    .bind(material)
    .bind(language)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The course's lessons other than replaced or skipped ones, in course order, as (lesson id, completed, current,
/// due at).
pub(super) async fn lesson_rows_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LessonRow>> {
    sqlx::query_as(
        "SELECT l.id,l.completed,COALESCE(l.id = p.current_lesson_id,0),s.due_at FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id JOIN learning_programs p ON p.id=l.program_id LEFT JOIN learning_drill_lesson_schedules s ON s.program_id=l.program_id AND s.lesson_id=l.id WHERE l.program_id=? AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY m.ordinal,l.ordinal",
    )
    .bind(program_id)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The lessons of the course a drill built from them is ready for, or whose attempt is submitted and not yet graded.
pub(super) async fn held_lessons_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT DISTINCT d.lesson_id FROM learning_drills d WHERE d.program_id=? AND d.lesson_id IS NOT NULL AND (d.status='ready' OR EXISTS(SELECT 1 FROM learning_drill_attempts a WHERE a.drill_id=d.id AND a.status='submitted'))",
    )
    .bind(program_id)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// Whether the course drills the lesson: it is one of the course's, and not replaced or skipped.
pub(crate) async fn drills_lesson_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=? AND id=? AND curriculum_state NOT IN ('replaced','skipped'))",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_one(connection).boxed()
    .await
    .map_err(db)
}

/// What is recorded about whether the lesson fits the material, as (fits, basis).
pub(super) async fn recorded_fit_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    material: &str,
) -> Result<Option<(i64, String)>> {
    sqlx::query_as(
        "SELECT fits,basis FROM learning_drill_lesson_fits WHERE program_id=? AND lesson_id=? AND material=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(material)
    .fetch_optional(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The lesson's module title, lesson title and objective, when it is one of the course's.
pub(super) async fn lesson_facts_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<(String, String, String)>> {
    sqlx::query_as(
        "SELECT m.title,l.title,l.objective FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.id=? AND l.program_id=?",
    )
    .bind(lesson_id)
    .bind(program_id)
    .fetch_optional(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// Whether the code fit check turned the lesson down on `basis`.
pub(super) async fn code_turned_down_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    basis: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM learning_drill_code_checks WHERE program_id=? AND lesson_id=? AND basis=? AND teaches=0)",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(basis)
    .fetch_one(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The lesson's blocks as (ordinal, kind, title, body, source ids as stored), in order.
pub(super) async fn blocks_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<Vec<BlockRow>> {
    sqlx::query_as(
        "SELECT ordinal,kind,title,body,source_ids_json FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(lesson_id)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// The active version of each of the course's sources that is a source, by version id.
pub(super) async fn active_source_versions_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT s.active_version_id FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.active_version_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection).boxed()
    .await
    .map_err(db)
}

/// Records whether the lesson fits `material` (`code` or `passage`), and the basis the decision rests on, replacing an
/// earlier record; nothing, when the lesson is no longer one of the course's.
pub(super) async fn record_fit_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    material: &str,
    fits: bool,
    basis: &str,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_drill_lesson_fits(program_id,lesson_id,material,fits,basis,decided_at) SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=?1 AND id=?2) ON CONFLICT(program_id,lesson_id,material) DO UPDATE SET fits=excluded.fits,basis=excluded.basis,decided_at=excluded.decided_at")
        .bind(program_id)
        .bind(lesson_id)
        .bind(material)
        .bind(i64::from(fits))
        .bind(basis)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map(drop)
        .map_err(db)
}
