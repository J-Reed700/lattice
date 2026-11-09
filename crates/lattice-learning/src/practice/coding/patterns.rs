//! Problem pattern ratings, schedules, and targets
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.9 and 7.3).

use super::hints::CodeHintRung;
use crate::recall::schedule::CardSchedule;
use crate::recall::study_dto::StudyRating;
use lattice_core::domain::learning::practice::problem_patterns::{
    ProblemPattern, ProblemPatternFamily, PROBLEM_PATTERNS,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

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
/// - Not every case passes, a line hint was used, or the solution was seen
///   before passing -> StudyRating::Again
pub fn rate_problem_pattern(input: &PatternOutcomeInput) -> StudyRating {
    if !input.all_cases_passed
        || input.solution_seen_before_pass
        || matches!(input.highest_hint_rung, Some(CodeHintRung::PointAtLine))
    {
        StudyRating::Again
    } else if matches!(
        input.highest_hint_rung,
        Some(CodeHintRung::Idea | CodeHintRung::Plan)
    ) {
        StudyRating::Hard
    } else if matches!(input.highest_hint_rung, Some(CodeHintRung::Nudge)) {
        StudyRating::Good
    } else {
        StudyRating::Easy
    }
}

fn db_err(e: sqlx::Error) -> AppError {
    AppError::Database(e.to_string())
}

/// Moves the schedule for a problem pattern on by one review rated `rating` at `now`.
/// Stored in `learning_drill_pattern_schedules` under material
/// `format!("problem:{}", language.to_ascii_lowercase())`.
/// Uses `interview_at` from `learning_practice_profiles` for `program_id` as target
/// horizon when set.
pub async fn record_problem_pattern_review(
    pool: &SqlitePool,
    program_id: &str,
    language: &str,
    pattern_id: &str,
    rating: StudyRating,
    now: i64,
) -> Result<i64> {
    let material = format!("problem:{}", language.to_ascii_lowercase());

    let target_at: Option<i64> = sqlx::query_scalar(
        "SELECT interview_at FROM learning_practice_profiles WHERE program_id = ?",
    )
    .bind(program_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .flatten();

    let row = sqlx::query(
        "SELECT stability, difficulty, last_reviewed_at, due_at, interval_days, \
         review_count, lapses FROM learning_drill_pattern_schedules \
         WHERE material = ? AND pattern_id = ?",
    )
    .bind(&material)
    .bind(pattern_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;

    let before = match row {
        Some(r) => CardSchedule {
            stability: Some(r.get("stability")),
            difficulty: Some(r.get("difficulty")),
            last_reviewed_at: Some(r.get("last_reviewed_at")),
            due_at: r.get("due_at"),
            interval_days: r.get("interval_days"),
            review_count: r.get("review_count"),
            lapses: r.get("lapses"),
        },
        None => CardSchedule {
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        },
    };

    let after = before.review(rating, now, target_at)?;
    let mut due_at = after.due_at;

    if let Some(target) = target_at {
        if target > now {
            if due_at <= now {
                due_at = (now + 60_000).min(target);
            } else if due_at > target {
                due_at = target;
            }
        }
    }

    let stability = match after.stability {
        Some(s) => s,
        None => 1.0,
    };
    let difficulty = match after.difficulty {
        Some(d) => d,
        None => 5.0,
    };
    let last_reviewed_at = match after.last_reviewed_at {
        Some(r) => r,
        None => now,
    };

    sqlx::query(
        "INSERT INTO learning_drill_pattern_schedules (
            material, pattern_id, stability, difficulty, last_reviewed_at, due_at,
            interval_days, review_count, lapses
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(material, pattern_id) DO UPDATE SET
            stability = excluded.stability,
            difficulty = excluded.difficulty,
            last_reviewed_at = excluded.last_reviewed_at,
            due_at = excluded.due_at,
            interval_days = excluded.interval_days,
            review_count = excluded.review_count,
            lapses = excluded.lapses",
    )
    .bind(&material)
    .bind(pattern_id)
    .bind(stability)
    .bind(difficulty)
    .bind(last_reviewed_at)
    .bind(due_at)
    .bind(after.interval_days)
    .bind(after.review_count.max(1))
    .bind(after.lapses)
    .execute(pool)
    .await
    .map_err(db_err)?;

    Ok(due_at)
}

/// Ranks patterns from `PROBLEM_PATTERNS`: due ones first, then ones never practised,
/// then the rest, interleaved by `PatternFamily`.
pub async fn targets(
    pool: &SqlitePool,
    _program_id: &str,
    language: &str,
    count: usize,
) -> Result<Vec<&'static ProblemPattern>> {
    if count == 0 {
        return Ok(Vec::new());
    }

    let material = format!("problem:{}", language.to_ascii_lowercase());
    let now = chrono::Utc::now().timestamp_millis();

    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT pattern_id, due_at FROM learning_drill_pattern_schedules WHERE material = ?",
    )
    .bind(&material)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut schedule_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (pat, due) in rows {
        schedule_map.insert(pat, due);
    }

    let mut due: Vec<(&'static ProblemPattern, i64)> = Vec::new();
    let mut unpractised: Vec<&'static ProblemPattern> = Vec::new();
    let mut future: Vec<(&'static ProblemPattern, i64)> = Vec::new();

    for pattern in PROBLEM_PATTERNS {
        match schedule_map.get(pattern.id) {
            Some(&due_at) => {
                if due_at <= now {
                    due.push((pattern, due_at));
                } else {
                    future.push((pattern, due_at));
                }
            }
            None => {
                unpractised.push(pattern);
            }
        }
    }

    due.sort_by_key(|(_, due_at)| *due_at);
    future.sort_by_key(|(_, due_at)| *due_at);

    let mut ordered: Vec<&'static ProblemPattern> = Vec::with_capacity(PROBLEM_PATTERNS.len());
    for (p, _) in due {
        ordered.push(p);
    }
    for p in unpractised {
        ordered.push(p);
    }
    for (p, _) in future {
        ordered.push(p);
    }

    let mut result: Vec<&'static ProblemPattern> = Vec::with_capacity(count.min(ordered.len()));
    let mut last_family: Option<ProblemPatternFamily> = None;

    while result.len() < count && !ordered.is_empty() {
        let next_idx = match last_family {
            Some(prev_fam) => match ordered.iter().position(|p| p.family != prev_fam) {
                Some(idx) => idx,
                None => 0,
            },
            None => 0,
        };

        let picked = ordered.remove(next_idx);
        last_family = Some(picked.family);
        result.push(picked);
    }

    Ok(result)
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
pub async fn load_practice_due(pool: &SqlitePool, _program_id: &str) -> Result<PracticeDueDto> {
    let now = chrono::Utc::now().timestamp_millis();
    let due_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM learning_drill_pattern_schedules \
         WHERE material LIKE 'problem:%' AND due_at <= ?",
    )
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    if due_count <= 0 {
        return Ok(PracticeDueDto {
            items: Vec::new(),
            total_due: 0,
        });
    }

    let count = due_count as u32;
    let item = DueItemDto {
        id: "problem_patterns".to_string(),
        kind: "pattern".to_string(),
        title: "Problem patterns".to_string(),
        subtitle: Some(format!("{} problem patterns due", count)),
        count: Some(count),
        overdue_text: Some(format!("{} due", count)),
        is_overdue: Some(true),
    };

    Ok(PracticeDueDto {
        items: vec![item],
        total_due: count,
    })
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
        assert_eq!(
            rate_problem_pattern(&input_solution_seen),
            StudyRating::Again
        );
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

        sqlx::query(
            "INSERT INTO learning_practice_profiles \
             (program_id, details, language, interview_at, revision, updated_at) \
             VALUES (?, 'Interview prep', 'python', ?, 1, ?)",
        )
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
            assert_ne!(
                fam0, fam1,
                "Consecutive targets should interleave families when possible"
            );
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

        sqlx::query(
            "INSERT INTO learning_practice_profiles \
             (program_id, details, language, revision, updated_at) \
             VALUES (?, 'Interview prep', 'python', 1, ?)",
        )
        .bind(&program_id)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert profile");

        // Insert two overdue problem pattern schedules
        sqlx::query(
            "INSERT INTO learning_drill_pattern_schedules (
                material, pattern_id, stability, difficulty, last_reviewed_at, due_at,
                interval_days, review_count, lapses
             ) VALUES ('problem:python', 'hash_map_lookup', 1.0, 5.0, ?, ?, 1, 1, 0),
                      ('problem:python', 'two_pointers', 1.0, 5.0, ?, ?, 1, 1, 0)",
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
