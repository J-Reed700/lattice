//! What a problem teaches the learner's schedule: a rating for each of its patterns by rule, the schedules those
//! ratings move, the patterns the next build should use, and what is due
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.9 and 6.2).

use super::dto::{CodeHintRung, DueItemDto, DueKind, PatternRatingDto, PracticeDueDto};
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::{problem_pattern, ProblemPattern, PROBLEM_PATTERNS};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

/// How a problem went, as far as its patterns are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemOutcome {
    pub all_passed: bool,
    pub highest_rung: Option<CodeHintRung>,
    pub solution_seen_before_passing: bool,
}

fn not_built() -> AppError {
    AppError::InvalidState("not built yet".into())
}

/// The rating every pattern of a problem gets.
pub fn rating(_outcome: &ProblemOutcome) -> PatternRating {
    PatternRating::Again
}

/// Moves the schedule of each of `pattern_ids` on by one review rated `rating` at `now`.
pub async fn record_ratings_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _language: &str,
    _pattern_ids: &[String],
    _rating: PatternRating,
    _now: i64,
    _target: Option<i64>,
) -> Result<Vec<PatternRatingDto>> {
    Err(not_built())
}

/// Pattern ids ranked for the next build.
pub async fn targets_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _language: &str,
    _now: i64,
) -> Result<Vec<String>> {
    Err(not_built())
}

/// What is due at `now` for the course.
pub async fn practice_due_in(
    _connection: &mut SqliteConnection,
    _program_id: &str,
    _now: i64,
) -> Result<PracticeDueDto> {
    Err(not_built())
}

#[cfg(test)]
mod tests;
