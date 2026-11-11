//! The drills' schedules: when each pattern of a material and each lesson of a course comes back, written once as a
//! review is graded; which patterns and lessons are pending; which material a lesson fits and on what basis; and the
//! lesson a drill is built from.

use super::support::{
    code, course, explanation_files, extra_finding, files, lessons_of, operator_finding,
    passage_spec, planted, proved, quiet, record_unfit, review, review_assessment,
    schedule_pattern, set_state, spacing_passage, spec, unchecked, FakeGrader, ScriptedModel,
    Waiting, LITERALS, MODEL, OPERATOR, SUITE,
};
use crate::drills::attempts::{self, AttemptMode, Graded, Submission};
use crate::drills::bank::Shelf;
use crate::drills::build::Evidence;
use crate::drills::repository::{self, NewDrill, StoredDrill};
use crate::drills::schedules;
use crate::drills::spec::DrillSpec;
use crate::dto::LearningProgramDto;
use crate::persistence::db;
use crate::recall::schedule::CardSchedule;
use crate::recall::study_dto::StudyRating;
use crate::repository::LearningRepository;
use crate::tests::{fixture, pool};
use lattice_core::domain::learning::drills::{
    ratings, score, Drill, IssueRating, LessonLink, LessonSentence, LessonStanding, Material,
    PatternDue, PatternRating,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobOutcome, JobStore};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const OFF_BY_ONE: &str = "correctness.off_by_one";
const BOUNDARY: &str = "correctness.boundary_and_empty_input";
const SWALLOWED: &str = "errors.swallowed_exception";
const CHECK_THEN_ACT: &str = "concurrency.check_then_act";
const MUTATED: &str = "correctness.mutated_argument";
const LOOSE: &str = "correctness.loose_equality";
const FIGURE: &str = "fidelity.misquoted_figure";
const REVERSED: &str = "reasoning.reversed_relation";

const DAY_MS: i64 = 86_400_000;

fn none() -> ScriptedModel {
    ScriptedModel::new(Vec::<String>::new())
}

fn never(now: i64) -> CardSchedule {
    CardSchedule {
        stability: None,
        difficulty: None,
        last_reviewed_at: None,
        due_at: now,
        interval_days: 0,
        review_count: 0,
        lapses: 0,
    }
}

/// The schedule after one review rated `rating` at `at`, from never reviewed.
fn once(rating: StudyRating, at: i64) -> CardSchedule {
    never(at)
        .review(rating, at, None)
        .expect("a schedule can be reviewed")
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

async fn pattern_schedule(
    pool: &SqlitePool,
    material: &str,
    pattern: &str,
) -> Result<Option<CardSchedule>> {
    let row = sqlx::query(
        "SELECT * FROM learning_drill_pattern_schedules WHERE material=? AND pattern_id=?",
    )
    .bind(material)
    .bind(pattern)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    Ok(row.as_ref().map(schedule_of))
}

async fn lesson_schedule(
    pool: &SqlitePool,
    program: &str,
    lesson: &str,
) -> Result<Option<CardSchedule>> {
    let row = sqlx::query(
        "SELECT * FROM learning_drill_lesson_schedules WHERE program_id=? AND lesson_id=?",
    )
    .bind(program)
    .bind(lesson)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    Ok(row.as_ref().map(schedule_of))
}

async fn count(pool: &SqlitePool, sql: &str) -> Result<i64> {
    sqlx::query_scalar(sql).fetch_one(pool).await.map_err(db)
}

/// A course of two modules of two lessons, the first lesson current.
async fn course_of(pool: &SqlitePool) -> Result<LearningProgramDto> {
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program)
}

/// The id of lesson `lesson` of module `module` of the course.
fn lesson_of(program: &LearningProgramDto, module: usize, lesson: usize) -> String {
    program.modules[module].lessons[lesson].id.clone()
}

/// A drill for the bank: code in python unless said otherwise, planting two patterns.
struct Put<'a> {
    id: &'a str,
    material: Material,
    program: Option<&'a str>,
    lesson: Option<&'a str>,
    /// The first issue of the key has a link to the lesson.
    link: bool,
    patterns: [&'a str; 2],
}

impl<'a> Put<'a> {
    fn new(id: &'a str) -> Self {
        Self {
            id,
            material: Material::code("python"),
            program: None,
            lesson: None,
            link: false,
            patterns: [OPERATOR, LITERALS],
        }
    }

    /// A drill built from `lesson` of the course `program`.
    fn of(id: &'a str, program: &'a str, lesson: &'a str) -> Self {
        Self {
            program: Some(program),
            lesson: Some(lesson),
            ..Self::new(id)
        }
    }

    /// Puts the drill into the bank, ready.
    async fn stock(self, pool: &SqlitePool) -> Result<()> {
        let passage = matches!(self.material, Material::Passage);
        let built = proved();
        let mut key = built.planted_key(&unchecked());
        for (issue, pattern) in key.iter_mut().zip(self.patterns) {
            issue.pattern = pattern.to_owned();
        }
        if let (true, Some(lesson), Some(issue)) = (self.link, self.lesson, key.first_mut()) {
            issue.lesson = Some(LessonLink {
                lesson_id: lesson.to_owned(),
                sentences: vec![LessonSentence {
                    text: "A sentence of the lesson.".into(),
                    source: None,
                }],
            });
        }
        let drill = Drill {
            id: self.id.into(),
            material: self.material.clone(),
            title: "A limiter".into(),
            files: if passage {
                explanation_files()
            } else {
                built.files.clone()
            },
            key,
        };
        let program_id = self.program.map(str::to_owned);
        let lesson_id = self.lesson.map(str::to_owned);
        let requested = if passage {
            DrillSpec {
                program_id,
                lesson_id,
                ..passage_spec("unused")
            }
        } else {
            DrillSpec {
                material: self.material.clone(),
                program_id,
                lesson_id,
                ..spec()
            }
        };
        let source = spacing_passage();
        let clean = if passage {
            explanation_files()
        } else {
            files()
        };
        let evidence = Evidence::new(
            &code(),
            &review(),
            &planted(),
            &built,
            &unchecked(),
            &quiet(),
        );
        let mut connection = pool.acquire().await.map_err(db)?;
        repository::insert_in(
            &mut connection,
            &NewDrill {
                drill: &drill,
                clean_files: &clean,
                spec: &requested,
                domain: "rate limiting",
                suite: if passage { None } else { Some(SUITE) },
                source: if passage { Some(&source) } else { None },
                runner: if passage { None } else { Some("fake-python") },
                model: MODEL,
                evidence: &evidence,
            },
        )
        .await
    }
}

/// Where a drill's attempt stands.
#[derive(Clone, Copy)]
enum State {
    Open,
    Submitted,
    Graded,
    CouldntGrade,
}

/// Takes a drill from the bank and opens an attempt on it, carried to `state`.
async fn hand_out(pool: &SqlitePool, drill: &str, state: State) -> Result<()> {
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::mark_taken_in(&mut connection, drill, 1).await?;
    let attempt =
        attempts::repository::open_in(&mut connection, drill, AttemptMode::Practice).await?;
    drop(connection);
    if matches!(state, State::Open) {
        return Ok(());
    }
    attempts::submit(
        &Database::from(pool.clone()),
        &attempt.id,
        Submission {
            findings: vec![extra_finding()],
            fix_files: None,
            time_spent_ms: 1_000,
        },
    )
    .await?;
    let mut connection = pool.acquire().await.map_err(db)?;
    match state {
        State::Graded => {
            let key = proved().planted_key(&unchecked());
            let assessment = review_assessment();
            let scored = score(&key, &[operator_finding(), extra_finding()], &assessment);
            let rated = ratings(&key, &assessment);
            attempts::repository::record_graded_in(
                &mut connection,
                &attempt.id,
                &Graded {
                    assessment: &assessment,
                    score: &scored,
                    ratings: &rated,
                    fix: None,
                    model: MODEL,
                },
            )
            .await?;
        }
        State::CouldntGrade => {
            attempts::repository::record_ungraded_in(
                &mut connection,
                &attempt.id,
                None,
                "No answer.",
            )
            .await?;
        }
        State::Open | State::Submitted => {}
    }
    Ok(())
}

async fn stored(pool: &SqlitePool, id: &str) -> Result<StoredDrill> {
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::find_in(&mut connection, id)
        .await?
        .ok_or_else(|| AppError::InternalError(format!("No drill {id}.")))
}

async fn record(
    pool: &SqlitePool,
    drill: &StoredDrill,
    rated: &[IssueRating],
    lesson: Option<PatternRating>,
    now: i64,
) -> Result<()> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::record_in(&mut connection, drill, rated, lesson, now).await
}

fn rate(pattern: &str, rating: PatternRating) -> IssueRating {
    IssueRating {
        pattern: pattern.to_owned(),
        rating,
    }
}

async fn pattern_states(
    pool: &SqlitePool,
    material: &Material,
    taking: &[&str],
) -> Result<Vec<PatternDue>> {
    let taking: Vec<String> = taking.iter().map(|id| (*id).to_owned()).collect();
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::pattern_states_in(&mut connection, material, &taking).await
}

async fn standings(
    pool: &SqlitePool,
    program: &str,
    material: &Material,
    taking: Option<&str>,
) -> Result<Vec<LessonStanding>> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::lesson_states_in(&mut connection, program, material, taking).await
}

fn standing<'a>(all: &'a [LessonStanding], lesson: &str) -> &'a LessonStanding {
    all.iter()
        .find(|found| found.lesson_id == lesson)
        .unwrap_or_else(|| panic!("{lesson} in {all:?}"))
}

fn ids(all: &[LessonStanding]) -> Vec<&str> {
    all.iter().map(|found| found.lesson_id.as_str()).collect()
}

fn pending_of(all: &[LessonStanding]) -> Vec<bool> {
    all.iter().map(|lesson| lesson.pending).collect()
}

async fn basis(pool: &SqlitePool, program: &str, lesson: &str) -> Result<String> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::fit_basis_in(&mut connection, program, lesson).await
}

async fn record_fit(
    pool: &SqlitePool,
    program: &str,
    lesson: &str,
    material: &Material,
    fits: bool,
    basis: &str,
) -> Result<()> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::record_fit_in(&mut connection, program, lesson, material, fits, basis, 10).await
}

fn due(pattern: &str, due_at: Option<i64>, pending: bool) -> PatternDue {
    PatternDue {
        pattern: pattern.to_owned(),
        due_at,
        pending,
    }
}

#[test]
fn a_material_is_scheduled_by_its_language_and_recorded_as_code_or_passage() {
    assert_eq!(
        schedules::schedule_material(&Material::code("python")),
        "code:python"
    );
    assert_eq!(schedules::schedule_material(&Material::Passage), "passage");
    assert_eq!(schedules::fit_material(&Material::code("rust")), "code");
    assert_eq!(schedules::fit_material(&Material::Passage), "passage");
}

#[tokio::test]
async fn a_graded_review_moves_each_rated_patterns_schedule_once() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let outcome = waiting
        .grade(&none(), None, &FakeGrader::new(review_assessment()))
        .await?;
    assert!(matches!(outcome, JobOutcome::Committed), "{outcome:?}");

    let operator = pattern_schedule(&waiting.pool, "code:python", OPERATOR)
        .await?
        .expect("the operator was rated");
    let at = operator.last_reviewed_at.expect("reviewed");
    assert_eq!(operator, once(StudyRating::Easy, at));
    let literals = pattern_schedule(&waiting.pool, "code:python", LITERALS)
        .await?
        .expect("the literals were rated");
    let at = literals.last_reviewed_at.expect("reviewed");
    assert_eq!(literals, once(StudyRating::Again, at));
    assert_eq!((literals.review_count, literals.lapses), (1, 1));
    assert_eq!(
        count(
            &waiting.pool,
            "SELECT count(*) FROM learning_drill_pattern_schedules"
        )
        .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn two_ratings_of_one_pattern_write_one_review_rated_hard() -> Result<()> {
    let pool = pool().await?;
    Put::new("drill").stock(&pool).await?;
    let drill = stored(&pool, "drill").await?;
    record(
        &pool,
        &drill,
        &[
            rate(OPERATOR, PatternRating::Easy),
            rate(OPERATOR, PatternRating::Again),
        ],
        None,
        5_000,
    )
    .await?;
    let row = pattern_schedule(&pool, "code:python", OPERATOR)
        .await?
        .expect("rated");
    assert_eq!(row, once(StudyRating::Hard, 5_000));
    assert_eq!(row.review_count, 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_drill_pattern_schedules"
        )
        .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_second_review_moves_a_schedule_on_from_where_it_stands() -> Result<()> {
    let pool = pool().await?;
    Put::new("drill").stock(&pool).await?;
    let drill = stored(&pool, "drill").await?;
    let first_at = 5_000;
    let second_at = first_at + 3 * DAY_MS;
    record(
        &pool,
        &drill,
        &[rate(OPERATOR, PatternRating::Easy)],
        None,
        first_at,
    )
    .await?;
    let first = pattern_schedule(&pool, "code:python", OPERATOR)
        .await?
        .expect("rated");
    assert_eq!(first, once(StudyRating::Easy, first_at));
    record(
        &pool,
        &drill,
        &[rate(OPERATOR, PatternRating::Good)],
        None,
        second_at,
    )
    .await?;
    let second = pattern_schedule(&pool, "code:python", OPERATOR)
        .await?
        .expect("rated again");
    assert_eq!(second.review_count, 2);
    assert_eq!(
        second,
        first
            .review(StudyRating::Good, second_at, None)
            .expect("reviewed")
    );
    Ok(())
}

#[tokio::test]
async fn a_passage_and_each_language_keep_their_own_schedule_of_a_pattern() -> Result<()> {
    let pool = pool().await?;
    Put::new("python").stock(&pool).await?;
    Put {
        material: Material::code("rust"),
        ..Put::new("rust")
    }
    .stock(&pool)
    .await?;
    Put {
        material: Material::Passage,
        patterns: [FIGURE, REVERSED],
        ..Put::new("passage")
    }
    .stock(&pool)
    .await?;
    let easy = [rate(OPERATOR, PatternRating::Easy)];
    record(&pool, &stored(&pool, "python").await?, &easy, None, 1_000).await?;
    record(&pool, &stored(&pool, "rust").await?, &easy, None, 2_000).await?;
    record(
        &pool,
        &stored(&pool, "passage").await?,
        &[rate(FIGURE, PatternRating::Good)],
        None,
        3_000,
    )
    .await?;
    assert_eq!(
        pattern_schedule(&pool, "code:python", OPERATOR).await?,
        Some(once(StudyRating::Easy, 1_000))
    );
    assert_eq!(
        pattern_schedule(&pool, "code:rust", OPERATOR).await?,
        Some(once(StudyRating::Easy, 2_000))
    );
    assert_eq!(
        pattern_schedule(&pool, "passage", FIGURE).await?,
        Some(once(StudyRating::Good, 3_000))
    );
    Ok(())
}

#[tokio::test]
async fn a_drill_of_a_lesson_moves_that_lessons_schedule_with_the_rating_given() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (first, second) = (lesson_of(&program, 0, 0), lesson_of(&program, 0, 1));
    Put::of("built", &course, &first).stock(&pool).await?;
    Put {
        program: Some(&course),
        ..Put::new("plain")
    }
    .stock(&pool)
    .await?;
    Put::of("unrated", &course, &second).stock(&pool).await?;

    let good = [rate(OPERATOR, PatternRating::Good)];
    record(
        &pool,
        &stored(&pool, "built").await?,
        &good,
        Some(PatternRating::Hard),
        9_000,
    )
    .await?;
    assert_eq!(
        lesson_schedule(&pool, &course, &first).await?,
        Some(once(StudyRating::Hard, 9_000))
    );

    record(
        &pool,
        &stored(&pool, "plain").await?,
        &good,
        Some(PatternRating::Easy),
        9_000,
    )
    .await?;
    record(&pool, &stored(&pool, "unrated").await?, &good, None, 9_000).await?;
    assert_eq!(lesson_schedule(&pool, &course, &second).await?, None);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_drill_lesson_schedules"
        )
        .await?,
        1,
        "a drill with no lesson, or a review that rated none, moves no lesson"
    );
    Ok(())
}

/// A course with a code drill built from its first lesson, with a link to the lesson on its first issue when `link`,
/// taken, submitted and graded; the pool, the course and the lesson.
async fn graded_code_drill(link: bool) -> Result<(SqlitePool, String, String)> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let lesson = lesson_of(&program, 0, 0);
    Put {
        link,
        ..Put::of("built", &course, &lesson)
    }
    .stock(&pool)
    .await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &course,
        material: &python,
    };
    let begun = attempts::start(
        &Database::from(pool.clone()),
        shelf,
        None,
        AttemptMode::Practice,
    )
    .await?
    .expect("a ready drill");
    let queued = attempts::submit(
        &Database::from(pool.clone()),
        &begun.attempt.id,
        Submission {
            findings: vec![operator_finding(), extra_finding()],
            fix_files: None,
            time_spent_ms: 90_000,
        },
    )
    .await?;
    let job = queued.job.expect("a grading job");
    let store = JobStore::new(pool.clone());
    Waiting::claim(&store, &job).await?;
    let waiting = Waiting {
        pool: pool.clone(),
        store,
        attempt: begun.attempt.id.clone(),
        job,
    };
    let outcome = waiting
        .grade(&none(), None, &FakeGrader::new(review_assessment()))
        .await?;
    assert!(matches!(outcome, JobOutcome::Committed), "{outcome:?}");
    Ok((pool, course, lesson))
}

#[tokio::test]
async fn only_an_issue_with_a_link_to_the_lesson_rates_it_when_a_code_drill_is_graded() -> Result<()>
{
    let (pool, course, lesson) = graded_code_drill(true).await?;

    let row = lesson_schedule(&pool, &course, &lesson)
        .await?
        .expect("the lesson was rated");
    let at = row.last_reviewed_at.expect("reviewed");
    assert_eq!(
        row,
        once(StudyRating::Easy, at),
        "the missed literals have no link, so only the operator counts"
    );
    assert!(pattern_schedule(&pool, "code:python", LITERALS)
        .await?
        .is_some());
    Ok(())
}

#[tokio::test]
async fn a_code_drill_with_no_lesson_issue_rates_its_lesson_by_every_rated_issue() -> Result<()> {
    let (pool, course, lesson) = graded_code_drill(false).await?;

    let row = lesson_schedule(&pool, &course, &lesson)
        .await?
        .expect("the lesson was rated");
    let at = row.last_reviewed_at.expect("reviewed");
    assert_eq!(
        row,
        once(StudyRating::Hard, at),
        "the operator was found and the literals missed, and none has a link"
    );
    let all = standings(&pool, &course, &Material::code("python"), None).await?;
    assert_eq!(
        standing(&all, &lesson).due_at,
        Some(row.due_at),
        "the lesson is no longer new"
    );
    Ok(())
}

async fn drilled_by(pool: &SqlitePool, program_id: &str, lesson_id: &str) -> Result<bool> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::repository::drills_lesson_in(&mut connection, program_id, lesson_id).await
}

#[tokio::test]
async fn a_course_drills_its_lessons_that_are_not_replaced_or_skipped() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let others = lessons_of(&pool, &other).await?;
    for lesson in &lessons {
        assert!(drilled_by(&pool, &program, lesson).await?, "{lesson}");
    }
    assert!(drilled_by(&pool, &other, &others[0]).await?);

    set_state(&pool, &lessons[0], "skipped").await?;
    set_state(&pool, &lessons[1], "replaced").await?;
    assert!(!drilled_by(&pool, &program, &lessons[0]).await?, "skipped");
    assert!(!drilled_by(&pool, &program, &lessons[1]).await?, "replaced");
    assert!(drilled_by(&pool, &program, &lessons[2]).await?);
    assert!(
        !drilled_by(&pool, &program, &others[0]).await?,
        "another course's lesson"
    );
    assert!(
        !drilled_by(&pool, &program, "no-such-lesson").await?,
        "no lesson"
    );
    assert!(drilled_by(&pool, &other, &others[0]).await?);
    Ok(())
}

#[tokio::test]
async fn grading_an_attempt_a_second_time_moves_no_schedule_again() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let (attempt, drill) =
        attempts::waiting(&Database::from(waiting.pool.clone()), &waiting.attempt)
            .await?
            .expect("waiting");
    let model = none();
    let grader = FakeGrader::new(review_assessment());
    let context = waiting.context().await?;

    let outcome = waiting
        .grading(&model, None, &grader)
        .run(&context, &attempt, &drill)
        .await?;
    assert!(matches!(outcome, JobOutcome::Committed), "{outcome:?}");
    let before = pattern_schedule(&waiting.pool, "code:python", OPERATOR)
        .await?
        .expect("rated");
    assert_eq!(before.review_count, 1);
    let _ = waiting
        .grading(&model, None, &grader)
        .run(&context, &attempt, &drill)
        .await;
    assert_eq!(
        pattern_schedule(&waiting.pool, "code:python", OPERATOR).await?,
        Some(before),
        "a second grading of the same attempt moves nothing"
    );
    Ok(())
}

#[tokio::test]
async fn a_grade_that_was_not_recorded_rates_nothing() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let (attempt, drill) =
        attempts::waiting(&Database::from(waiting.pool.clone()), &waiting.attempt)
            .await?
            .expect("waiting");
    let key = &drill.drill.key;
    let assessment = review_assessment();
    let scored = score(key, &[operator_finding(), extra_finding()], &assessment);
    let rated = ratings(key, &assessment);
    let mut connection = waiting.pool.acquire().await.map_err(db)?;
    let recorded = attempts::repository::record_graded_in(
        &mut connection,
        &attempt.id,
        &Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: None,
            model: "another-grader",
        },
    )
    .await?;
    drop(connection);
    assert!(recorded, "graded elsewhere while this job ran");

    let context = waiting.context().await?;
    let outcome = waiting
        .grading(&none(), None, &FakeGrader::new(review_assessment()))
        .run(&context, &attempt, &drill)
        .await?;
    assert!(
        matches!(outcome, JobOutcome::Completed { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        count(
            &waiting.pool,
            "SELECT count(*) FROM learning_drill_pattern_schedules"
        )
        .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn a_pattern_is_listed_with_its_schedule_and_as_pending_while_a_drill_holds_it() -> Result<()>
{
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let lesson = lesson_of(&program, 0, 0);
    schedule_pattern(&pool, "code:python", OPERATOR, 100).await?;
    schedule_pattern(&pool, "code:python", SWALLOWED, 200).await?;
    schedule_pattern(&pool, "code:rust", BOUNDARY, 300).await?;
    schedule_pattern(&pool, "passage", FIGURE, 400).await?;

    Put::of("ready-lesson", &course, &lesson)
        .stock(&pool)
        .await?;
    Put {
        patterns: [OFF_BY_ONE, BOUNDARY],
        ..Put::new("ready-old")
    }
    .stock(&pool)
    .await?;
    for (id, state, patterns) in [
        ("submitted", State::Submitted, [MUTATED, CHECK_THEN_ACT]),
        (
            "open",
            State::Open,
            ["errors.lost_context", "errors.unchecked_return_value"],
        ),
        (
            "graded",
            State::Graded,
            ["errors.overly_broad_catch", "errors.error_as_success"],
        ),
        (
            "couldnt",
            State::CouldntGrade,
            ["concurrency.lock_ordering", "concurrency.missing_await"],
        ),
    ] {
        Put {
            patterns,
            ..Put::new(id)
        }
        .stock(&pool)
        .await?;
        hand_out(&pool, id, state).await?;
    }
    Put {
        material: Material::code("rust"),
        patterns: ["errors.unwrap_on_fallible", "correctness.integer_overflow"],
        ..Put::of("ready-rust", &course, &lesson)
    }
    .stock(&pool)
    .await?;

    let python = pattern_states(&pool, &Material::code("python"), &[LITERALS, LOOSE]).await?;
    assert_eq!(
        python,
        [
            due(OPERATOR, Some(100), true),
            due(SWALLOWED, Some(200), false),
            due(CHECK_THEN_ACT, None, true),
            due(LOOSE, None, true),
            due(MUTATED, None, true),
            due(LITERALS, None, true),
        ],
        "rows first, then the pending ones with no row, each pattern once"
    );

    let rust = pattern_states(&pool, &Material::code("rust"), &[]).await?;
    assert_eq!(
        rust,
        [
            due(BOUNDARY, Some(300), false),
            due("correctness.integer_overflow", None, true),
            due("errors.unwrap_on_fallible", None, true),
        ]
    );
    let passage = pattern_states(&pool, &Material::Passage, &[]).await?;
    assert_eq!(passage, [due(FIGURE, Some(400), false)]);
    assert!(pattern_states(&pool, &Material::code("go"), &[])
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn lessons_come_in_course_order_without_replaced_or_skipped_ones() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let all = [
        lesson_of(&program, 0, 0),
        lesson_of(&program, 0, 1),
        lesson_of(&program, 1, 0),
        lesson_of(&program, 1, 1),
    ];
    let code = Material::code("python");
    let expected: Vec<LessonStanding> = all
        .iter()
        .map(|id| LessonStanding {
            lesson_id: id.clone(),
            completed: false,
            current: *id == all[0],
            due_at: None,
            pending: false,
            fits: true,
        })
        .collect();
    assert_eq!(standings(&pool, &course, &code, None).await?, expected);

    for (state, lesson) in [("replaced", &all[1]), ("skipped", &all[2])] {
        sqlx::query("UPDATE learning_lessons SET curriculum_state=? WHERE id=?")
            .bind(state)
            .bind(lesson)
            .execute(&pool)
            .await
            .map_err(db)?;
    }
    let left = standings(&pool, &course, &code, None).await?;
    assert_eq!(ids(&left), [all[0].as_str(), all[3].as_str()]);

    sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
        .bind(&all[3])
        .execute(&pool)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_programs SET current_lesson_id=? WHERE id=?")
        .bind(&all[3])
        .bind(&course)
        .execute(&pool)
        .await
        .map_err(db)?;
    let marked = standings(&pool, &course, &code, None).await?;
    let (first, last) = (standing(&marked, &all[0]), standing(&marked, &all[3]));
    assert_eq!((first.current, first.completed), (false, false));
    assert_eq!((last.current, last.completed), (true, true));
    Ok(())
}

#[tokio::test]
async fn a_lessons_due_date_is_the_one_in_its_schedule() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (first, last) = (lesson_of(&program, 0, 0), lesson_of(&program, 1, 1));
    Put::of("built", &course, &last).stock(&pool).await?;
    record(
        &pool,
        &stored(&pool, "built").await?,
        &[rate(OPERATOR, PatternRating::Good)],
        Some(PatternRating::Good),
        4_000,
    )
    .await?;
    let expected = lesson_schedule(&pool, &course, &last)
        .await?
        .expect("the lesson was rated");
    let all = standings(&pool, &course, &Material::code("python"), None).await?;
    assert_eq!(standing(&all, &last).due_at, Some(expected.due_at));
    assert_eq!(standing(&all, &first).due_at, None);
    Ok(())
}

#[tokio::test]
async fn a_lesson_is_pending_while_a_drill_of_it_is_ready_or_waits_for_its_grade() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let other = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let [a, b, c, d] = [
        lesson_of(&program, 0, 0),
        lesson_of(&program, 0, 1),
        lesson_of(&program, 1, 0),
        lesson_of(&program, 1, 1),
    ];
    let elsewhere = lesson_of(&other, 0, 0);
    Put::of("ready-code", &course, &a).stock(&pool).await?;
    Put {
        material: Material::Passage,
        ..Put::of("ready-passage", &course, &b)
    }
    .stock(&pool)
    .await?;
    Put::of("open", &course, &c).stock(&pool).await?;
    hand_out(&pool, "open", State::Open).await?;
    Put::of("submitted", &course, &d).stock(&pool).await?;
    hand_out(&pool, "submitted", State::Submitted).await?;
    Put::of("of-another", &other.summary.id, &elsewhere)
        .stock(&pool)
        .await?;

    let code = Material::code("python");
    let all = standings(&pool, &course, &code, None).await?;
    assert_eq!(pending_of(&all), [true, true, false, true]);
    let passages = standings(&pool, &course, &Material::Passage, None).await?;
    assert_eq!(
        pending_of(&passages),
        [true, true, false, true],
        "a drill of either material holds the lesson back"
    );
    let taking = standings(&pool, &course, &code, Some(c.as_str())).await?;
    assert_eq!(pending_of(&taking), [true, true, true, true]);
    let others = standings(&pool, &other.summary.id, &code, None).await?;
    assert_eq!(pending_of(&others), [true, false, false, false]);
    Ok(())
}

#[tokio::test]
async fn a_lesson_recorded_as_not_fitting_passages_on_the_current_basis_does_not_fit_them(
) -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (first, second) = (lesson_of(&program, 0, 0), lesson_of(&program, 0, 1));
    let passage = Material::Passage;
    let code = Material::code("python");

    let all = standings(&pool, &course, &passage, None).await?;
    assert!(
        standing(&all, &first).fits && standing(&all, &second).fits,
        "no record, no refusal"
    );

    let current = basis(&pool, &course, &first).await?;
    record_fit(&pool, &course, &first, &passage, false, &current).await?;
    let all = standings(&pool, &course, &passage, None).await?;
    assert!(!standing(&all, &first).fits);
    assert!(standing(&all, &second).fits);
    let all = standings(&pool, &course, &code, None).await?;
    assert!(
        standing(&all, &first).fits,
        "a lesson that does not fit passages can still be drilled in code"
    );

    record_fit(&pool, &course, &first, &passage, false, "another basis").await?;
    let all = standings(&pool, &course, &passage, None).await?;
    assert!(
        standing(&all, &first).fits,
        "a record on another basis no longer stands"
    );

    record_fit(&pool, &course, &first, &passage, true, &current).await?;
    let all = standings(&pool, &course, &passage, None).await?;
    assert!(standing(&all, &first).fits);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM learning_drill_lesson_fits").await?,
        1,
        "a record is replaced, not added to"
    );
    Ok(())
}

async fn fits_code(pool: &SqlitePool, course: &str, lesson: &str, language: &str) -> Result<bool> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::lesson_fits_in(&mut connection, course, lesson, &Material::code(language)).await
}

#[tokio::test]
async fn a_lesson_the_code_check_turned_down_on_its_current_basis_does_not_fit_code() -> Result<()>
{
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (first, second) = (lesson_of(&program, 0, 0), lesson_of(&program, 0, 1));
    let code = Material::code("python");

    let all = standings(&pool, &course, &code, None).await?;
    assert!(
        standing(&all, &first).fits && standing(&all, &second).fits,
        "no check, no refusal"
    );

    record_unfit(&pool, &course, &first, &code).await?;
    let all = standings(&pool, &course, &code, None).await?;
    assert!(!standing(&all, &first).fits);
    assert!(standing(&all, &second).fits);
    assert!(!fits_code(&pool, &course, &first, "python").await?);
    assert!(
        !fits_code(&pool, &course, &first, "rust").await?,
        "code is code in any language"
    );
    let all = standings(&pool, &course, &Material::Passage, None).await?;
    assert!(
        standing(&all, &first).fits,
        "a lesson that does not fit code can still be explained"
    );
    assert_eq!(count(&pool, CODE_CHECKS).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_code_check_made_on_another_basis_or_that_said_the_lesson_teaches_does_not_stop_it_fitting_code(
) -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let first = lesson_of(&program, 0, 0);
    record_unfit(&pool, &course, &first, &Material::code("python")).await?;
    assert!(!fits_code(&pool, &course, &first, "python").await?);
    let (current,): (String,) =
        sqlx::query_as("SELECT basis FROM learning_drill_code_checks WHERE lesson_id=?")
            .bind(&first)
            .fetch_one(&pool)
            .await
            .map_err(db)?;

    sqlx::query("UPDATE learning_drill_code_checks SET basis='another basis'")
        .execute(&pool)
        .await
        .map_err(db)?;
    assert!(
        fits_code(&pool, &course, &first, "python").await?,
        "a verdict on another basis no longer stands"
    );

    sqlx::query("UPDATE learning_drill_code_checks SET basis=?")
        .bind(&current)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert!(!fits_code(&pool, &course, &first, "python").await?);
    let objective: String = sqlx::query_scalar("SELECT objective FROM learning_lessons WHERE id=?")
        .bind(&first)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_lessons SET objective='Another objective' WHERE id=?")
        .bind(&first)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert!(
        fits_code(&pool, &course, &first, "python").await?,
        "the lesson's objective changed, so the question is asked again"
    );

    sqlx::query("UPDATE learning_lessons SET objective=? WHERE id=?")
        .bind(&objective)
        .bind(&first)
        .execute(&pool)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_drill_code_checks SET teaches=1")
        .execute(&pool)
        .await
        .map_err(db)?;
    assert!(fits_code(&pool, &course, &first, "python").await?);
    Ok(())
}

#[tokio::test]
async fn the_fit_table_no_longer_decides_whether_a_lesson_fits_code() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let first = lesson_of(&program, 0, 0);
    let code = Material::code("python");
    let current = basis(&pool, &course, &first).await?;
    record_fit(&pool, &course, &first, &code, false, &current).await?;
    assert!(fits_code(&pool, &course, &first, "python").await?);
    let all = standings(&pool, &course, &code, None).await?;
    assert!(standing(&all, &first).fits);
    Ok(())
}

#[tokio::test]
async fn changing_a_source_of_the_course_ends_a_record_made_before() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let lesson = lesson_of(&program, 0, 0);
    let passage = Material::Passage;
    let current = basis(&pool, &course, &lesson).await?;
    record_fit(&pool, &course, &lesson, &passage, false, &current).await?;
    let all = standings(&pool, &course, &passage, None).await?;
    assert!(!standing(&all, &lesson).fits);

    sqlx::query("UPDATE learning_source_library SET deleted_at=5 WHERE program_id=?")
        .bind(&course)
        .execute(&pool)
        .await
        .map_err(db)?;
    let all = standings(&pool, &course, &passage, None).await?;
    assert!(
        standing(&all, &lesson).fits,
        "the sources changed, so the question is asked again"
    );
    Ok(())
}

#[tokio::test]
async fn the_basis_of_a_fit_changes_with_the_lessons_blocks_and_the_courses_sources() -> Result<()>
{
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let other = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (lesson, neighbour) = (lesson_of(&program, 0, 0), lesson_of(&program, 0, 1));
    let start = basis(&pool, &course, &lesson).await?;
    assert!(!start.is_empty());
    assert_eq!(basis(&pool, &course, &lesson).await?, start);

    let block = "INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,0,'recap','Title','Body','[]')";
    sqlx::query(block)
        .bind(&neighbour)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        basis(&pool, &course, &lesson).await?,
        start,
        "another lesson's blocks are not this lesson's"
    );
    sqlx::query(block)
        .bind(&lesson)
        .execute(&pool)
        .await
        .map_err(db)?;
    let with_block = basis(&pool, &course, &lesson).await?;
    assert_ne!(with_block, start);
    sqlx::query("UPDATE learning_blocks SET body='Another body' WHERE lesson_id=?")
        .bind(&lesson)
        .execute(&pool)
        .await
        .map_err(db)?;
    let edited = basis(&pool, &course, &lesson).await?;
    assert_ne!(edited, with_block);

    sqlx::query("UPDATE learning_source_library SET deleted_at=5 WHERE program_id=?")
        .bind(&other.summary.id)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        basis(&pool, &course, &lesson).await?,
        edited,
        "another course's sources are not this course's"
    );
    sqlx::query("UPDATE learning_source_library SET deleted_at=5 WHERE program_id=?")
        .bind(&course)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_ne!(basis(&pool, &course, &lesson).await?, edited);
    Ok(())
}

#[tokio::test]
async fn a_drill_keeps_the_lesson_it_names_only_when_it_is_of_the_course_it_keeps() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let other = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let lesson = lesson_of(&program, 1, 0);
    let elsewhere = lesson_of(&other, 0, 0);

    Put::of("kept", &course, &lesson).stock(&pool).await?;
    Put::of("of-another-course", &course, &elsewhere)
        .stock(&pool)
        .await?;
    Put::of("not-there", &course, "no-such-lesson")
        .stock(&pool)
        .await?;
    Put {
        lesson: Some(&lesson),
        ..Put::new("no-course")
    }
    .stock(&pool)
    .await?;

    let kept = stored(&pool, "kept").await?;
    assert_eq!(kept.lesson_id.as_deref(), Some(lesson.as_str()));
    assert_eq!(kept.program_id.as_deref(), Some(course.as_str()));
    for id in ["of-another-course", "not-there", "no-course"] {
        assert_eq!(stored(&pool, id).await?.lesson_id, None, "{id}");
    }
    Ok(())
}

#[tokio::test]
async fn deleting_a_course_removes_its_schedules_and_fits_and_keeps_its_drills_with_no_lesson(
) -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let course = program.summary.id.clone();
    let (first, second) = (lesson_of(&program, 0, 0), lesson_of(&program, 0, 1));
    Put::of("first", &course, &first).stock(&pool).await?;
    Put::of("second", &course, &second).stock(&pool).await?;
    for lesson in [&first, &second] {
        sqlx::query("INSERT INTO learning_drill_lesson_schedules(program_id,lesson_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,0,1,1,1,0)")
            .bind(&course)
            .bind(lesson)
            .execute(&pool)
            .await
            .map_err(db)?;
        sqlx::query("INSERT INTO learning_drill_lesson_fits(program_id,lesson_id,material,fits,basis,decided_at) VALUES(?,?,'passage',0,'b',1)")
            .bind(&course)
            .bind(lesson)
            .execute(&pool)
            .await
            .map_err(db)?;
    }
    assert_eq!(
        stored(&pool, "first").await?.lesson_id.as_deref(),
        Some(first.as_str())
    );

    sqlx::query("DELETE FROM learning_lessons WHERE id=?")
        .bind(&second)
        .execute(&pool)
        .await
        .map_err(db)?;
    let orphaned = stored(&pool, "second").await?;
    assert_eq!(orphaned.lesson_id, None);
    assert_eq!(orphaned.program_id.as_deref(), Some(course.as_str()));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_drill_lesson_schedules"
        )
        .await?,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM learning_drill_lesson_fits").await?,
        1
    );

    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&course)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_drill_lesson_schedules"
        )
        .await?,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM learning_drill_lesson_fits").await?,
        0
    );
    for id in ["first", "second"] {
        let drill = stored(&pool, id).await?;
        assert_eq!((drill.program_id, drill.lesson_id), (None, None), "{id}");
    }
    Ok(())
}

#[tokio::test]
async fn the_schedule_tables_refuse_what_is_not_a_schedule_or_a_lesson_of_the_course() -> Result<()>
{
    let pool = pool().await?;
    let program = course_of(&pool).await?;
    let other = course_of(&pool).await?;
    let lesson = lesson_of(&program, 0, 0);
    let insert = |material: &'static str, reviews: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("INSERT INTO learning_drill_pattern_schedules(material,pattern_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,'p',1.0,5.0,0,1,1,?,0)")
                .bind(material)
                .bind(reviews)
                .execute(&pool)
                .await
        }
    };
    assert!(insert("code:", 1).await.is_err(), "code with no language");
    assert!(insert("other", 1).await.is_err(), "no such material");
    assert!(insert("passage", 0).await.is_err(), "never reviewed");
    assert!(insert("code:python", 1).await.is_ok());
    assert!(insert("passage", 1).await.is_ok());

    let wrong = sqlx::query("INSERT INTO learning_drill_lesson_schedules(program_id,lesson_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,0,1,1,1,0)")
        .bind(&other.summary.id)
        .bind(&lesson)
        .execute(&pool)
        .await;
    assert!(wrong.is_err(), "a lesson of another course");
    let wrong = sqlx::query("INSERT INTO learning_drill_lesson_fits(program_id,lesson_id,material,fits,basis,decided_at) VALUES(?,?,'video',1,'b',1)")
        .bind(&program.summary.id)
        .bind(&lesson)
        .execute(&pool)
        .await;
    assert!(wrong.is_err(), "no such material");
    Ok(())
}

/// The ids of the source versions of the course `program_id`, in id order.
async fn versions_of(pool: &SqlitePool, program_id: &str) -> Result<Vec<String>> {
    sqlx::query_scalar("SELECT id FROM learning_source_versions WHERE program_id=? ORDER BY id")
        .bind(program_id)
        .fetch_all(pool)
        .await
        .map_err(db)
}

/// Stores a verdict of the passage check on the bytes `start..end` of the version `version`.
async fn insert_check(
    pool: &SqlitePool,
    program: &str,
    lesson: &str,
    version: &str,
    (start, end): (i64, i64),
    teaches: i64,
) -> std::result::Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query("INSERT INTO learning_drill_passage_checks(program_id,lesson_id,version_id,start_byte,end_byte,basis,teaches,reason,model,checked_at) VALUES(?,?,?,?,?,'b',?,'r','m',1)")
        .bind(program)
        .bind(lesson)
        .bind(version)
        .bind(start)
        .bind(end)
        .bind(teaches)
        .execute(pool)
        .await
}

const CHECKS: &str = "SELECT count(*) FROM learning_drill_passage_checks";

const CODE_CHECKS: &str = "SELECT count(*) FROM learning_drill_code_checks";

#[tokio::test]
async fn deleting_a_lesson_removes_its_passage_checks() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let versions = versions_of(&pool, &program).await?;
    for lesson in lessons.iter().take(2) {
        insert_check(&pool, &program, lesson, &versions[0], (0, 10), 1)
            .await
            .map_err(db)?;
    }
    assert_eq!(count(&pool, CHECKS).await?, 2);

    sqlx::query("DELETE FROM learning_lessons WHERE id=?")
        .bind(&lessons[1])
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(count(&pool, CHECKS).await?, 1);
    let kept: String = sqlx::query_scalar("SELECT lesson_id FROM learning_drill_passage_checks")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(kept, lessons[0]);
    Ok(())
}

#[tokio::test]
async fn deleting_a_course_removes_its_passage_checks() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    for owner in [&program, &other] {
        let lesson = lessons_of(&pool, owner).await?[0].clone();
        let version = versions_of(&pool, owner).await?[0].clone();
        insert_check(&pool, owner, &lesson, &version, (0, 10), 0)
            .await
            .map_err(db)?;
    }
    assert_eq!(count(&pool, CHECKS).await?, 2);

    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&program)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(count(&pool, CHECKS).await?, 1);
    let kept: String = sqlx::query_scalar("SELECT program_id FROM learning_drill_passage_checks")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(kept, other);
    Ok(())
}

#[tokio::test]
async fn deleting_a_source_version_removes_its_passage_checks() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lessons_of(&pool, &program).await?[0].clone();
    let versions = versions_of(&pool, &program).await?;
    assert!(versions.len() >= 2, "{versions:?}");
    for version in versions.iter().take(2) {
        insert_check(&pool, &program, &lesson, version, (0, 10), 1)
            .await
            .map_err(db)?;
    }
    assert_eq!(count(&pool, CHECKS).await?, 2);

    sqlx::query("DELETE FROM learning_source_versions WHERE id=?")
        .bind(&versions[0])
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(count(&pool, CHECKS).await?, 1);
    let kept: String = sqlx::query_scalar("SELECT version_id FROM learning_drill_passage_checks")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(kept, versions[1]);
    Ok(())
}

#[tokio::test]
async fn the_passage_check_table_refuses_a_lesson_or_version_of_another_course_and_what_is_not_a_verdict_or_a_span(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let lesson = lessons_of(&pool, &program).await?[0].clone();
    let version = versions_of(&pool, &program).await?[0].clone();
    let elsewhere_lesson = lessons_of(&pool, &other).await?[0].clone();
    let elsewhere_version = versions_of(&pool, &other).await?[0].clone();

    assert!(
        insert_check(&pool, &program, &elsewhere_lesson, &version, (0, 10), 1)
            .await
            .is_err(),
        "a lesson of another course"
    );
    assert!(
        insert_check(&pool, &program, &lesson, &elsewhere_version, (0, 10), 1)
            .await
            .is_err(),
        "a version of another course"
    );
    assert!(
        insert_check(&pool, &program, &lesson, &version, (0, 10), 2)
            .await
            .is_err(),
        "teaches is 0 or 1"
    );
    assert!(
        insert_check(&pool, &program, &lesson, &version, (10, 10), 1)
            .await
            .is_err(),
        "an empty span"
    );
    assert!(
        insert_check(&pool, &program, &lesson, &version, (10, 5), 1)
            .await
            .is_err(),
        "a span that ends before it starts"
    );
    assert!(
        insert_check(&pool, &program, &lesson, &version, (-1, 5), 1)
            .await
            .is_err(),
        "a span that starts before the text"
    );
    assert_eq!(count(&pool, CHECKS).await?, 0);
    assert!(insert_check(&pool, &program, &lesson, &version, (0, 10), 1)
        .await
        .is_ok());
    Ok(())
}

/// Stores a verdict of the code fit check on the lesson `lesson` of the course `program`.
async fn insert_code_check(
    pool: &SqlitePool,
    program: &str,
    lesson: &str,
    teaches: i64,
) -> std::result::Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query("INSERT INTO learning_drill_code_checks(program_id,lesson_id,basis,teaches,reason,model,checked_at) VALUES(?,?,'b',?,'r','m',1)")
        .bind(program)
        .bind(lesson)
        .bind(teaches)
        .execute(pool)
        .await
}

#[tokio::test]
async fn deleting_a_lesson_removes_its_code_check() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    for lesson in lessons.iter().take(2) {
        insert_code_check(&pool, &program, lesson, 1)
            .await
            .map_err(db)?;
    }
    assert_eq!(count(&pool, CODE_CHECKS).await?, 2);

    sqlx::query("DELETE FROM learning_lessons WHERE id=?")
        .bind(&lessons[1])
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(count(&pool, CODE_CHECKS).await?, 1);
    let kept: String = sqlx::query_scalar("SELECT lesson_id FROM learning_drill_code_checks")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(kept, lessons[0]);
    Ok(())
}

#[tokio::test]
async fn deleting_a_course_removes_its_code_checks() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    for owner in [&program, &other] {
        let lesson = lessons_of(&pool, owner).await?[0].clone();
        insert_code_check(&pool, owner, &lesson, 0)
            .await
            .map_err(db)?;
    }
    assert_eq!(count(&pool, CODE_CHECKS).await?, 2);

    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&program)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(count(&pool, CODE_CHECKS).await?, 1);
    let kept: String = sqlx::query_scalar("SELECT program_id FROM learning_drill_code_checks")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(kept, other);
    Ok(())
}

#[tokio::test]
async fn the_code_check_table_refuses_a_lesson_of_another_course_and_what_is_not_a_verdict(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let lesson = lessons_of(&pool, &program).await?[0].clone();
    let elsewhere = lessons_of(&pool, &other).await?[0].clone();

    assert!(
        insert_code_check(&pool, &program, &elsewhere, 1)
            .await
            .is_err(),
        "a lesson of another course"
    );
    assert!(
        insert_code_check(&pool, &program, &lesson, 2)
            .await
            .is_err(),
        "teaches is 0 or 1"
    );
    assert_eq!(count(&pool, CODE_CHECKS).await?, 0);
    assert!(insert_code_check(&pool, &program, &lesson, 1).await.is_ok());
    assert!(
        insert_code_check(&pool, &program, &lesson, 0)
            .await
            .is_err(),
        "one verdict for a lesson"
    );
    Ok(())
}
