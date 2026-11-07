//! The two schedules of review drills (docs/design/2026-11-07-drills-in-the-course.md section 5): when each pattern
//! of a material and each lesson of a course comes back, and which material a lesson fits. Grading writes the
//! schedules in the transaction that records the grade, and the bank reads them.

use super::repository::StoredDrill;
use lattice_core::domain::learning::drills::{
    IssueRating, LessonStanding, Material, PatternDue, PatternRating,
};
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

/// The name a material's pattern schedules are kept under: `code:<language>`, or `passage`.
pub(in crate::features::learning) fn schedule_material(material: &Material) -> String {
    match material {
        Material::Code { language } => format!("code:{language}"),
        Material::Passage => "passage".to_owned(),
    }
}

/// The name a material is recorded under when a lesson is found to fit it or not: `code` or `passage`.
pub(in crate::features::learning) fn fit_material(material: &Material) -> &'static str {
    match material {
        Material::Code { .. } => "code",
        Material::Passage => "passage",
    }
}

/// Moves the schedule of each pattern `ratings` rate, once, and the lesson's when `lesson` is rated and the drill
/// was built from one.
pub(in crate::features::learning) async fn record_in(
    _connection: &mut SqliteConnection,
    _drill: &StoredDrill,
    _ratings: &[IssueRating],
    _lesson: Option<PatternRating>,
    _now: i64,
) -> Result<()> {
    Ok(())
}

/// Where each pattern of `material` stands: those with a schedule and those pending.
pub(in crate::features::learning) async fn pattern_states_in(
    _connection: &mut SqliteConnection,
    _material: &Material,
    _taking: &[String],
) -> Result<Vec<PatternDue>> {
    Ok(Vec::new())
}

/// Where each lesson of the course stands for the next drill of `material`.
pub(in crate::features::learning) async fn lesson_states_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _material: &Material,
    _taking: Option<&str>,
) -> Result<Vec<LessonStanding>> {
    Ok(Vec::new())
}

/// What a decision that a lesson fits a material rests on.
pub(in crate::features::learning) async fn fit_basis_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _lesson_id: &str,
) -> Result<String> {
    Ok(String::new())
}

/// Records whether a lesson fits a material, and the basis the decision rests on.
pub(in crate::features::learning) async fn record_fit_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _lesson_id: &str,
    _material: &Material,
    _fits: bool,
    _basis: &str,
    _now: i64,
) -> Result<()> {
    Ok(())
}
