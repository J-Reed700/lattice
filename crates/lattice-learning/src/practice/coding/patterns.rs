//! Problem pattern ratings, schedules, and targets
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.9 and 7.3).

use super::hints::CodeHintRung;
use crate::recall::study_dto::StudyRating;
use lattice_core::domain::learning::practice::problem_patterns::ProblemPattern;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternOutcomeInput {
    pub all_cases_passed: bool,
    pub highest_hint_rung: Option<CodeHintRung>,
    pub solution_seen_before_pass: bool,
}

/// Pure rating rule (§4.9):
/// - Every case passes, with no hint and no worked solution -> StudyRating::Easy
/// - Every case passes, with at most a nudge -> StudyRating::Good
/// - Every case passes after an idea or plan hint -> StudyRating::Hard
/// - Not every case passes, a line hint was used, or the solution was seen before passing -> StudyRating::Again
pub fn rate_problem_pattern(_input: &PatternOutcomeInput) -> StudyRating {
    StudyRating::Again
}

/// Moves the schedule for a problem pattern on by one review rated `rating` at `now`.
/// Stored in `learning_drill_pattern_schedules` under material `format!("problem:{}", language.to_ascii_lowercase())`.
/// Uses `interview_at` from `learning_practice_profiles` for `program_id` as target horizon when set.
pub async fn record_problem_pattern_review(
    _pool: &SqlitePool,
    _program_id: &str,
    _language: &str,
    _pattern_id: &str,
    _rating: StudyRating,
    _now: i64,
) -> Result<i64> {
    Err(AppError::InvalidState(
        "record_problem_pattern_review stub".into(),
    ))
}

/// Ranks patterns from `PROBLEM_PATTERNS`: due ones first, then ones never practised,
/// then the rest, interleaved by `PatternFamily`.
pub async fn targets(
    _pool: &SqlitePool,
    _program_id: &str,
    _language: &str,
    _count: usize,
) -> Result<Vec<&'static ProblemPattern>> {
    Err(AppError::InvalidState("targets stub".into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DueItemDto {
    pub id: String,
    pub kind: String, // "pattern" | "lesson"
    pub title: String,
    pub subtitle: Option<String>,
    pub count: Option<u32>,
    pub overdue_text: Option<String>,
    pub is_overdue: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDueDto {
    pub items: Vec<DueItemDto>,
    pub total_due: u32,
}

/// Aggregates due items for practice home. Problem patterns that are due are grouped
/// under `"Problem patterns"`, hiding individual pattern names until the debrief.
pub async fn load_practice_due(
    _pool: &SqlitePool,
    _program_id: &str,
) -> Result<PracticeDueDto> {
    Err(AppError::InvalidState("load_practice_due stub".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::learning::practice::problem_patterns::PROBLEM_PATTERNS;

    #[test]
    fn test_rate_problem_pattern_easy() {
        let input = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: None,
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input), StudyRating::Easy);
    }

    #[test]
    fn test_rate_problem_pattern_good() {
        let input = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: Some(CodeHintRung::Nudge),
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input), StudyRating::Good);
    }

    #[test]
    fn test_rate_problem_pattern_hard() {
        let input_idea = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: Some(CodeHintRung::Idea),
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input_idea), StudyRating::Hard);

        let input_plan = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: Some(CodeHintRung::Plan),
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input_plan), StudyRating::Hard);
    }

    #[test]
    fn test_rate_problem_pattern_again() {
        // 1. Not all cases pass
        let input_not_passed = PatternOutcomeInput {
            all_cases_passed: false,
            highest_hint_rung: None,
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input_not_passed), StudyRating::Again);

        // 2. PointAtLine hint rung used
        let input_line_hint = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: Some(CodeHintRung::PointAtLine),
            solution_seen_before_pass: false,
        };
        assert_eq!(rate_problem_pattern(&input_line_hint), StudyRating::Again);

        // 3. Solution seen before passing
        let input_solution_seen = PatternOutcomeInput {
            all_cases_passed: true,
            highest_hint_rung: None,
            solution_seen_before_pass: true,
        };
        assert_eq!(rate_problem_pattern(&input_solution_seen), StudyRating::Again);
    }

    #[tokio::test]
    async fn test_record_problem_pattern_persistence_uses_interview_horizon() {
        let pool = crate::test_support::pool().await.expect("test pool");
        let prog = crate::test_support::fixture();
        let program_id = prog.summary.id.clone();
        let target_at = 1_800_000_000_000i64;
        let now = 1_700_000_000_000i64;

        crate::repository::LearningRepository::new(pool.clone())
            .create(&prog)
            .await
            .expect("insert program");

        sqlx::query("INSERT INTO learning_practice_profiles (program_id, details, language, interview_at, revision, updated_at) VALUES (?, 'Interview prep', 'python', ?, 1, ?)")
            .bind(&program_id)
            .bind(target_at)
            .bind(now)
            .execute(&pool)
            .await
            .expect("insert profile");

        let pattern_id = "hash_map_lookup";
        let due_at = record_problem_pattern_review(
            &pool,
            &program_id,
            "python",
            pattern_id,
            StudyRating::Good,
            now,
        )
        .await
        .expect("record review");

        assert!(due_at > now);
        assert!(due_at <= target_at);

        let stored_material: String = sqlx::query_scalar(
            "SELECT material FROM learning_drill_pattern_schedules WHERE pattern_id = ?",
        )
        .bind(pattern_id)
        .fetch_one(&pool)
        .await
        .expect("fetch stored schedule");

        assert_eq!(stored_material, "problem:python");
    }

    #[tokio::test]
    async fn test_targets_orders_due_first_then_unpractised_interleaved_by_family() {
        let pool = crate::test_support::pool().await.expect("test pool");
        let prog = crate::test_support::fixture();
        let program_id = prog.summary.id.clone();

        crate::repository::LearningRepository::new(pool.clone())
            .create(&prog)
            .await
            .expect("insert program");

        let ranked = targets(&pool, &program_id, "python", 6)
            .await
            .expect("rank targets");

        assert!(!ranked.is_empty());
        assert!(ranked.len() <= 6);

        // Every target must exist in catalogue
        for target in &ranked {
            assert!(PROBLEM_PATTERNS.iter().any(|p| p.id == target.id));
        }

        // Distinct families should be interleaved if multiple are available
        if ranked.len() >= 2 {
            let fam0 = ranked[0].family;
            let fam1 = ranked[1].family;
            assert_ne!(fam0, fam1, "Consecutive targets should interleave families when possible");
        }
    }

    #[tokio::test]
    async fn test_load_practice_due_hides_pattern_names_behind_problem_patterns() {
        let pool = crate::test_support::pool().await.expect("test pool");
        let prog = crate::test_support::fixture();
        let program_id = prog.summary.id.clone();
        let now = 1_700_000_000_000i64;

        crate::repository::LearningRepository::new(pool.clone())
            .create(&prog)
            .await
            .expect("insert program");

        sqlx::query("INSERT INTO learning_practice_profiles (program_id, details, language, revision, updated_at) VALUES (?, 'Interview prep', 'python', 1, ?)")
            .bind(&program_id)
            .bind(now)
            .execute(&pool)
            .await
            .expect("insert profile");

        // Insert two overdue problem pattern schedules
        sqlx::query(
            "INSERT INTO learning_drill_pattern_schedules (material, pattern_id, stability, difficulty, last_reviewed_at, due_at, interval_days, review_count, lapses)
             VALUES ('problem:python', 'hash_map_lookup', 1.0, 5.0, ?, ?, 1, 1, 0),
                    ('problem:python', 'two_pointers', 1.0, 5.0, ?, ?, 1, 1, 0)"
        )
        .bind(now - 100_000)
        .bind(now - 50_000)
        .bind(now - 100_000)
        .bind(now - 40_000)
        .execute(&pool)
        .await
        .expect("insert schedules");

        let due_dto = load_practice_due(&pool, &program_id)
            .await
            .expect("load due items");

        assert_eq!(due_dto.total_due, 2);
        assert_eq!(due_dto.items.len(), 1);

        let item = &due_dto.items[0];
        assert_eq!(item.title, "Problem patterns");
        assert_eq!(item.kind, "pattern");
        assert_eq!(item.count, Some(2));
        assert_eq!(item.is_overdue, Some(true));

        // CRITICAL CHECK: Individual secret pattern names must never leak!
        for pattern in PROBLEM_PATTERNS {
            assert!(
                !item.title.contains(pattern.name),
                "Due item title must not leak secret pattern name '{}'",
                pattern.name
            );
            if let Some(subtitle) = &item.subtitle {
                assert!(
                    !subtitle.contains(pattern.name),
                    "Due item subtitle must not leak secret pattern name '{}'",
                    pattern.name
                );
            }
        }
    }
}
