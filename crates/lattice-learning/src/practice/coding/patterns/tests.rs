use super::*;
use crate::repository::LearningRepository;
use sqlx::SqlitePool;

const DAY_MS: i64 = 86_400_000;
const NOW: i64 = 1_800_000_000_000;

fn outcome(
    all_passed: bool,
    highest_rung: Option<CodeHintRung>,
    solution_seen_before_passing: bool,
) -> ProblemOutcome {
    ProblemOutcome {
        all_passed,
        highest_rung,
        solution_seen_before_passing,
    }
}

#[test]
fn every_case_passed_without_help_is_easy() {
    assert_eq!(rating(&outcome(true, None, false)), PatternRating::Easy);
}

#[test]
fn every_case_passed_after_a_nudge_is_good() {
    let outcome = outcome(true, Some(CodeHintRung::Nudge), false);
    assert_eq!(rating(&outcome), PatternRating::Good);
}

#[test]
fn every_case_passed_after_an_idea_is_hard() {
    let outcome = outcome(true, Some(CodeHintRung::Idea), false);
    assert_eq!(rating(&outcome), PatternRating::Hard);
}

#[test]
fn every_case_passed_after_a_plan_is_hard() {
    let outcome = outcome(true, Some(CodeHintRung::Plan), false);
    assert_eq!(rating(&outcome), PatternRating::Hard);
}

#[test]
fn a_failing_case_is_again_whatever_the_help() {
    for rung in [
        None,
        Some(CodeHintRung::Nudge),
        Some(CodeHintRung::Idea),
        Some(CodeHintRung::Plan),
        Some(CodeHintRung::Line),
    ] {
        assert_eq!(rating(&outcome(false, rung, false)), PatternRating::Again);
    }
}

#[test]
fn a_line_hint_is_again_even_when_every_case_passed() {
    let outcome = outcome(true, Some(CodeHintRung::Line), false);
    assert_eq!(rating(&outcome), PatternRating::Again);
}

#[test]
fn the_solution_seen_before_passing_is_again() {
    assert_eq!(rating(&outcome(true, None, true)), PatternRating::Again);
    let nudged = outcome(true, Some(CodeHintRung::Nudge), true);
    assert_eq!(rating(&nudged), PatternRating::Again);
}

async fn course() -> (SqlitePool, crate::dto::LearningProgramDto) {
    let pool = crate::test_support::pool().await.expect("test pool");
    let program = crate::test_support::fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await
        .expect("insert program");
    (pool, program)
}

async fn schedule(pool: &SqlitePool, material: &str, pattern: &str, due_at: i64) {
    sqlx::query(
        "INSERT INTO learning_drill_pattern_schedules(material,pattern_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,?,?,1,1,0)",
    )
    .bind(material)
    .bind(pattern)
    .bind(NOW - 10 * DAY_MS)
    .bind(due_at)
    .execute(pool)
    .await
    .expect("insert a pattern schedule");
}

async fn due_of(pool: &SqlitePool, material: &str, pattern: &str) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT due_at FROM learning_drill_pattern_schedules WHERE material=? AND pattern_id=?",
    )
    .bind(material)
    .bind(pattern)
    .fetch_optional(pool)
    .await
    .expect("read a pattern schedule")
}

/// The first `count` catalogue patterns of pairwise different families.
fn of_different_families(count: usize) -> Vec<&'static ProblemPattern> {
    let mut chosen: Vec<&'static ProblemPattern> = Vec::new();
    for pattern in PROBLEM_PATTERNS {
        if chosen.iter().all(|picked| picked.family != pattern.family) {
            chosen.push(pattern);
        }
    }
    assert!(chosen.len() >= count, "the catalogue has {count} families");
    chosen.truncate(count);
    chosen
}

#[tokio::test]
async fn ratings_are_recorded_under_the_problem_material_and_leave_the_drills_alone() {
    let (pool, program) = course().await;
    let pattern = of_different_families(1)[0];
    schedule(&pool, "code:python", pattern.id, 777).await;
    let mut connection = pool.acquire().await.expect("connection");

    let rated = record_ratings_in(
        &mut connection,
        &program.summary.id,
        "python",
        &[pattern.id.to_owned()],
        PatternRating::Good,
        NOW,
        None,
    )
    .await
    .expect("record");
    drop(connection);

    assert_eq!(rated.len(), 1);
    assert_eq!(rated[0].pattern_id, pattern.id);
    assert_eq!(rated[0].name, pattern.name);
    assert_eq!(rated[0].cue, pattern.cue);
    assert_eq!(rated[0].rating, PatternRating::Good);
    assert!(rated[0].due_at > NOW);
    assert_eq!(
        due_of(&pool, "problem:python", pattern.id).await,
        Some(rated[0].due_at)
    );
    assert_eq!(due_of(&pool, "code:python", pattern.id).await, Some(777));
    let drill_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM learning_drill_pattern_schedules WHERE material='code:python'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(drill_rows, 1);
}

#[tokio::test]
async fn a_second_review_moves_the_same_schedule_on() {
    let (pool, program) = course().await;
    let id = of_different_families(1)[0].id.to_owned();
    let mut connection = pool.acquire().await.expect("connection");
    for at in [NOW, NOW + 2 * DAY_MS] {
        record_ratings_in(
            &mut connection,
            &program.summary.id,
            "Python",
            std::slice::from_ref(&id),
            PatternRating::Easy,
            at,
            None,
        )
        .await
        .expect("record");
    }
    drop(connection);
    let reviews: i64 = sqlx::query_scalar(
        "SELECT review_count FROM learning_drill_pattern_schedules WHERE material='problem:python' AND pattern_id=?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .expect("read");
    assert_eq!(reviews, 2);
}

#[tokio::test]
async fn the_interview_date_pulls_the_next_review_in() {
    let (pool, program) = course().await;
    let id = of_different_families(1)[0].id.to_owned();
    let target = NOW + 3 * DAY_MS;
    let mut connection = pool.acquire().await.expect("connection");
    let rated = record_ratings_in(
        &mut connection,
        &program.summary.id,
        "python",
        &[id],
        PatternRating::Easy,
        NOW,
        Some(target),
    )
    .await
    .expect("record");
    assert!(rated[0].due_at > NOW);
    assert!(rated[0].due_at <= target);
}

#[tokio::test]
async fn an_id_the_catalogue_does_not_hold_is_left_out() {
    let (pool, program) = course().await;
    let mut connection = pool.acquire().await.expect("connection");
    let rated = record_ratings_in(
        &mut connection,
        &program.summary.id,
        "python",
        &["not_a_pattern".to_owned()],
        PatternRating::Good,
        NOW,
        None,
    )
    .await
    .expect("record");
    assert!(rated.is_empty());
}

#[tokio::test]
async fn targets_rank_due_then_never_practised_then_the_rest() {
    let (pool, program) = course().await;
    let families = of_different_families(3);
    let (most_overdue, overdue, never) = (families[0], families[1], families[2]);
    for (order, pattern) in PROBLEM_PATTERNS.iter().enumerate() {
        let at = if pattern.id == most_overdue.id {
            NOW - 5 * DAY_MS
        } else if pattern.id == overdue.id {
            NOW - DAY_MS
        } else if pattern.id == never.id {
            continue;
        } else {
            NOW + (order as i64 + 1) * DAY_MS
        };
        schedule(&pool, "problem:python", pattern.id, at).await;
    }
    let mut connection = pool.acquire().await.expect("connection");

    let ranked = targets_in(&mut connection, &program.summary.id, "python", NOW)
        .await
        .expect("targets");

    assert_eq!(ranked.len(), PROBLEM_PATTERNS.len());
    assert_eq!(ranked[..3], [most_overdue.id, overdue.id, never.id]);
}

#[tokio::test]
async fn targets_ignore_the_drills_schedules_and_other_languages() {
    let (pool, program) = course().await;
    let first = PROBLEM_PATTERNS.first().expect("a catalogue");
    schedule(&pool, "code:python", first.id, NOW + 30 * DAY_MS).await;
    schedule(&pool, "problem:rust", first.id, NOW + 30 * DAY_MS).await;
    let mut connection = pool.acquire().await.expect("connection");

    let ranked = targets_in(&mut connection, &program.summary.id, "python", NOW)
        .await
        .expect("targets");

    assert_eq!(ranked.first().map(String::as_str), Some(first.id));
}

#[tokio::test]
async fn targets_interleave_the_families() {
    let (pool, program) = course().await;
    let mut connection = pool.acquire().await.expect("connection");

    let ranked = targets_in(&mut connection, &program.summary.id, "python", NOW)
        .await
        .expect("targets");

    let families = of_different_families(6).len();
    let leading: Vec<&ProblemPattern> = ranked
        .iter()
        .take(families)
        .filter_map(|id| problem_pattern(id))
        .collect();
    assert_eq!(leading.len(), families);
    for (at, pattern) in leading.iter().enumerate() {
        assert!(
            leading
                .iter()
                .skip(at + 1)
                .all(|other| other.family != pattern.family),
            "the first {families} picks hold each family once"
        );
    }
}

#[tokio::test]
async fn due_holds_nothing_when_nothing_is_due() {
    let (pool, program) = course().await;
    let id = of_different_families(1)[0].id;
    schedule(&pool, "problem:python", id, NOW + DAY_MS).await;
    let mut connection = pool.acquire().await.expect("connection");

    let due = practice_due_in(&mut connection, &program.summary.id, NOW, true)
        .await
        .expect("due");

    assert!(due.items.is_empty());
}

#[tokio::test]
async fn due_problem_patterns_are_one_item_that_names_no_pattern() {
    let (pool, program) = course().await;
    let families = of_different_families(2);
    schedule(&pool, "problem:python", families[0].id, NOW - 2 * DAY_MS).await;
    schedule(&pool, "problem:python", families[1].id, NOW - DAY_MS).await;
    let mut connection = pool.acquire().await.expect("connection");

    let due = practice_due_in(&mut connection, &program.summary.id, NOW, true)
        .await
        .expect("due");

    assert_eq!(due.items.len(), 1);
    let item = &due.items[0];
    assert_eq!(item.kind, DueKind::CodingPatterns);
    assert_eq!(item.count, 2);
    assert_eq!(item.label, "2 problem patterns");
    assert_eq!(item.due_at, NOW - 2 * DAY_MS);
    for pattern in PROBLEM_PATTERNS {
        assert!(
            !item.label.to_lowercase().contains(&pattern.name.to_lowercase()),
            "the label names {}",
            pattern.name
        );
    }
}

#[tokio::test]
async fn due_adds_the_drills_lessons_and_patterns_most_overdue_first() {
    let (pool, program) = course().await;
    let lesson = &program.modules[0].lessons[0];
    sqlx::query(
        "INSERT INTO learning_drill_lesson_schedules(program_id,lesson_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,?,?,1,1,0)",
    )
    .bind(&program.summary.id)
    .bind(&lesson.id)
    .bind(NOW - 10 * DAY_MS)
    .bind(NOW - 3 * DAY_MS)
    .execute(&pool)
    .await
    .expect("insert a lesson schedule");
    let pattern = of_different_families(1)[0].id;
    schedule(&pool, "problem:python", pattern, NOW - DAY_MS).await;
    schedule(&pool, "code:python", pattern, NOW - 5 * DAY_MS).await;
    let mut connection = pool.acquire().await.expect("connection");

    let due = practice_due_in(&mut connection, &program.summary.id, NOW, true)
        .await
        .expect("due");

    let kinds: Vec<DueKind> = due.items.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        [
            DueKind::DrillPatterns,
            DueKind::DrillLesson,
            DueKind::CodingPatterns
        ]
    );
    assert_eq!(due.items[0].material.as_deref(), Some("code:python"));
    assert_eq!(due.items[0].count, 1);
    assert_eq!(due.items[1].label, lesson.title);
    assert_eq!(due.items[1].lesson_id.as_deref(), Some(lesson.id.as_str()));
    assert_eq!(due.items[1].due_at, NOW - 3 * DAY_MS);
    for item in &due.items {
        for pattern in PROBLEM_PATTERNS {
            assert!(!item
                .label
                .to_lowercase()
                .contains(&pattern.name.to_lowercase()));
        }
    }
}

#[tokio::test]
async fn due_coding_items_are_left_out_of_a_course_where_code_does_not_fit() {
    let (pool, program) = course().await;
    let pattern = of_different_families(1)[0].id;
    schedule(&pool, "problem:python", pattern, NOW - DAY_MS).await;
    schedule(&pool, "code:python", pattern, NOW - 2 * DAY_MS).await;
    schedule(&pool, "passage", "any_passage_pattern", NOW - 3 * DAY_MS).await;
    let mut connection = pool.acquire().await.expect("connection");

    let without = practice_due_in(&mut connection, &program.summary.id, NOW, false)
        .await
        .expect("due");
    let with = practice_due_in(&mut connection, &program.summary.id, NOW, true)
        .await
        .expect("due");

    let kinds = |due: &PracticeDueDto| -> Vec<DueKind> {
        due.items.iter().map(|item| item.kind).collect()
    };
    assert_eq!(kinds(&without), [DueKind::DrillPatterns]);
    assert_eq!(without.items[0].material.as_deref(), Some("passage"));
    assert_eq!(
        kinds(&with),
        [
            DueKind::DrillPatterns,
            DueKind::DrillPatterns,
            DueKind::CodingPatterns
        ]
    );
}
