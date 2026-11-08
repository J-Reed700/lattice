//! The bank: one drill ready on each shelf of a course, and the build of the lesson next up queued when the last is
//! taken.

use super::support::{
    code, course, files, held_back, lesson_spec, lessons_of, passage_spec, planted, proved, quiet,
    record_unfit, review, schedule_pattern, set_state, spec, unchecked, MODEL, SUITE,
};
use crate::drills::bank::{ensure_build, queue_build, take, Shelf, Taken};
use crate::drills::build::Evidence;
use crate::drills::jobs::{spec_of, DRILL_BUILD};
use crate::drills::repository::{self, DrillStatus, NewDrill};
use crate::drills::spec::{check_request, DrillSpec};
use crate::persistence::{db, now};
use crate::tests::pool;
use lattice_core::domain::learning::drills::{targets, Drill, Material, PatternDue};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus, JobStore};
use serde_json::json;
use sqlx::SqlitePool;

/// A ready drill for the bank: code in python, of a course and a lesson of it, built at a time.
struct Ready<'a> {
    id: &'a str,
    program: &'a str,
    lesson: Option<&'a str>,
    language: &'a str,
    at: i64,
    lines: u32,
    patterns: &'a [&'a str],
}

impl<'a> Ready<'a> {
    fn new(id: &'a str, program: &'a str, lesson: Option<&'a str>) -> Self {
        Self {
            id,
            program,
            lesson,
            language: "python",
            at: 100,
            lines: 40,
            patterns: &["readability.magic_number"],
        }
    }

    /// Puts the drill into the bank.
    async fn put(self, pool: &SqlitePool) -> Result<()> {
        let proved = proved();
        let drill = Drill {
            id: self.id.into(),
            material: Material::code(self.language),
            title: "A limiter".into(),
            files: proved.files.clone(),
            key: proved.planted_key(&unchecked()),
        };
        let mut requested = spec();
        requested.material = Material::code(self.language);
        requested.lines = self.lines;
        requested.patterns = self
            .patterns
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect();
        requested.program_id = Some(self.program.to_owned());
        requested.lesson_id = self.lesson.map(str::to_owned);
        let evidence = Evidence::new(
            &code(),
            &review(),
            &planted(),
            &proved,
            &unchecked(),
            &quiet(),
        );
        let mut connection = pool.acquire().await.map_err(db)?;
        repository::insert_in(
            &mut connection,
            &NewDrill {
                drill: &drill,
                clean_files: &files(),
                spec: &requested,
                domain: "payments and refunds",
                suite: Some(SUITE),
                source: None,
                runner: Some("fake-python"),
                model: MODEL,
                evidence: &evidence,
            },
        )
        .await?;
        sqlx::query("UPDATE learning_drills SET created_at=? WHERE id=?")
            .bind(self.at)
            .bind(self.id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        Ok(())
    }
}

/// A spec of a drill of `material` on the course `program`, built from `lesson`, with the patterns the schedule
/// offers first.
fn build_spec(program: &str, lesson: &str, material: Material) -> DrillSpec {
    DrillSpec {
        patterns: targets(&material, &[], 0, 2)
            .iter()
            .map(|pattern| pattern.id.to_owned())
            .collect(),
        material,
        lines: 40,
        program_id: Some(program.to_owned()),
        lesson_id: Some(lesson.to_owned()),
    }
}

/// A course and its lessons, in course order.
async fn course_with_lessons(pool: &SqlitePool) -> Result<(String, Vec<String>)> {
    let program = course(pool).await?;
    let lessons = lessons_of(pool, &program).await?;
    Ok((program, lessons))
}

async fn builds(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=?")
        .bind(DRILL_BUILD)
        .fetch_one(pool)
        .await
        .map_err(db)
}

/// The build queued when `taken` was taken.
async fn queued_for(pool: &SqlitePool, taken: &Taken) -> Result<JobRecord> {
    let id = taken.next_build.as_deref().expect("a build was queued");
    JobStore::new(pool.clone()).get(id).await
}

/// Ends the job as `status` says: `failed` or `completed`.
async fn end(pool: &SqlitePool, job: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET status=?,finished_at=1,result_ref='ended' WHERE id=?")
        .bind(status)
        .bind(job)
        .execute(pool)
        .await
        .map(drop)
        .map_err(db)
}

async fn status_of(pool: &SqlitePool, id: &str) -> Result<String> {
    sqlx::query_scalar("SELECT status FROM learning_drills WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(db)
}

fn pattern_ids(material: &Material, states: &[PatternDue], now: i64, count: usize) -> Vec<String> {
    targets(material, states, now, count)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect()
}

#[tokio::test]
async fn with_nothing_ready_on_the_shelf_nothing_is_given_and_nothing_is_queued() -> Result<()> {
    let pool = pool().await?;
    let (first, first_lessons) = course_with_lessons(&pool).await?;
    let (second, second_lessons) = course_with_lessons(&pool).await?;
    Ready {
        language: "javascript",
        ..Ready::new("other language", &first, Some(first_lessons[0].as_str()))
    }
    .put(&pool)
    .await?;
    Ready::new("other course", &second, Some(second_lessons[0].as_str()))
        .put(&pool)
        .await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &first,
        material: &python,
    };
    assert_eq!(take(&pool, shelf, None).await?, None);
    assert_eq!(builds(&pool).await?, 0);

    let there = Shelf {
        program_id: &second,
        material: &python,
    };
    let taken = take(&pool, there, None)
        .await?
        .expect("the other course's drill");
    assert_eq!(taken.drill.drill.id, "other course");
    Ok(())
}

#[tokio::test]
async fn the_oldest_ready_drill_of_the_shelf_built_from_a_lesson_is_taken_and_marked() -> Result<()>
{
    let pool = pool().await?;
    let (first, lessons) = course_with_lessons(&pool).await?;
    let (second, others) = course_with_lessons(&pool).await?;
    Ready::new("older", &first, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    Ready {
        at: 200,
        ..Ready::new("newer", &first, Some(lessons[1].as_str()))
    }
    .put(&pool)
    .await?;
    Ready {
        at: 10,
        ..Ready::new("before-lessons", &first, None)
    }
    .put(&pool)
    .await?;
    Ready {
        at: 50,
        ..Ready::new("elsewhere", &second, Some(others[0].as_str()))
    }
    .put(&pool)
    .await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &first,
        material: &python,
    };

    let one = take(&pool, shelf, None).await?.expect("a ready drill");
    assert_eq!(one.drill.drill.id, "older");
    assert_eq!(one.drill.status, DrillStatus::Taken);
    assert!(one.drill.taken_at.is_some());
    assert_eq!(status_of(&pool, "older").await?, "taken");
    let taken_at: Option<i64> =
        sqlx::query_scalar("SELECT taken_at FROM learning_drills WHERE id='older'")
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(taken_at, one.drill.taken_at);

    let two = take(&pool, shelf, None)
        .await?
        .expect("a second ready drill");
    assert_eq!(two.drill.drill.id, "newer");
    assert_eq!(
        take(&pool, shelf, None).await?,
        None,
        "a drill is given once"
    );
    assert_eq!(
        status_of(&pool, "before-lessons").await?,
        "ready",
        "a drill from before lessons is never served"
    );
    assert_eq!(
        status_of(&pool, "elsewhere").await?,
        "ready",
        "another course's drill stays in the bank"
    );
    Ok(())
}

#[tokio::test]
async fn asking_for_a_lesson_takes_only_a_drill_built_from_it() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    Ready::new("first", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    Ready {
        at: 200,
        ..Ready::new("second", &program, Some(lessons[1].as_str()))
    }
    .put(&pool)
    .await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };

    let asked = take(&pool, shelf, Some(lessons[1].as_str()))
        .await?
        .expect("the second lesson's drill");
    assert_eq!(asked.drill.drill.id, "second");
    assert_eq!(
        take(&pool, shelf, Some(lessons[1].as_str())).await?,
        None,
        "it was given once"
    );
    assert_eq!(
        take(&pool, shelf, Some(lessons[2].as_str())).await?,
        None,
        "no drill was built from the third lesson"
    );
    assert_eq!(status_of(&pool, "first").await?, "ready");
    let first = take(&pool, shelf, Some(lessons[0].as_str()))
        .await?
        .expect("the first lesson's drill");
    assert_eq!(first.drill.drill.id, "first");
    Ok(())
}

#[tokio::test]
async fn taking_the_last_ready_drill_queues_the_build_of_the_lesson_next_up() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    Ready {
        lines: 70,
        patterns: &[
            "readability.magic_number",
            "correctness.wrong_operator",
            "errors.swallowed_exception",
        ],
        ..Ready::new("older", &program, Some(lessons[0].as_str()))
    }
    .put(&pool)
    .await?;
    let python = Material::code("python");
    let taken = take(
        &pool,
        Shelf {
            program_id: &program,
            material: &python,
        },
        None,
    )
    .await?
    .expect("a ready drill");

    let job = queued_for(&pool, &taken).await?;
    assert_eq!(job.kind, DRILL_BUILD);
    assert_eq!(job.subject_id, Some(format!("drill:{program}:code:python")));
    assert_eq!(job.status, JobStatus::Pending);
    let queued = spec_of(&job)?;
    assert_eq!(
        queued,
        DrillSpec {
            material: python.clone(),
            lines: 70,
            patterns: pattern_ids(&python, &held_back(&taken.drill.drill), 0, 3),
            program_id: Some(program.clone()),
            lesson_id: Some(lessons[1].clone()),
        },
        "the taken drill's lesson is pending, so a lesson never drilled comes first"
    );
    assert_eq!(
        queued.patterns.len(),
        3,
        "the taken drill's number of patterns"
    );
    assert_eq!(builds(&pool).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_pattern_due_in_the_schedule_is_the_first_the_next_build_plants() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    Ready {
        lines: 70,
        patterns: &[
            "readability.magic_number",
            "correctness.wrong_operator",
            "errors.swallowed_exception",
        ],
        ..Ready::new("older", &program, Some(lessons[0].as_str()))
    }
    .put(&pool)
    .await?;
    let due = "concurrency.check_then_act";
    schedule_pattern(&pool, "code:python", due, 1).await?;
    schedule_pattern(&pool, "code:rust", "correctness.off_by_one", 1).await?;
    let python = Material::code("python");
    let taken = take(
        &pool,
        Shelf {
            program_id: &program,
            material: &python,
        },
        None,
    )
    .await?
    .expect("a ready drill");

    let queued = spec_of(&queued_for(&pool, &taken).await?)?;
    assert_eq!(queued.patterns.first().map(String::as_str), Some(due));
    let mut states = held_back(&taken.drill.drill);
    states.push(PatternDue {
        pattern: due.to_owned(),
        due_at: Some(1),
        pending: false,
    });
    assert_eq!(queued.patterns, pattern_ids(&python, &states, now(), 3));
    for held in held_back(&taken.drill.drill) {
        assert!(
            !queued.patterns.contains(&held.pattern),
            "{} is held back while its drill is out",
            held.pattern
        );
    }
    Ok(())
}

#[tokio::test]
async fn taking_queues_no_build_while_another_drill_of_the_shelf_is_ready() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    Ready::new("first", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    Ready {
        at: 200,
        ..Ready::new("second", &program, Some(lessons[1].as_str()))
    }
    .put(&pool)
    .await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };
    let first = take(&pool, shelf, None).await?.expect("the first drill");
    assert_eq!(first.next_build, None, "the second drill is ready");
    assert_eq!(builds(&pool).await?, 0);
    let second = take(&pool, shelf, None).await?.expect("the second drill");
    assert!(second.next_build.is_some(), "the shelf is empty now");
    assert_eq!(builds(&pool).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_ready_drill_from_before_lessons_holds_nothing_back() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    Ready {
        at: 50,
        ..Ready::new("before-lessons", &program, None)
    }
    .put(&pool)
    .await?;
    Ready::new("only", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };
    let only = take(&pool, shelf, None).await?.expect("the only drill");
    assert_eq!(only.drill.drill.id, "only");
    assert!(
        only.next_build.is_some(),
        "a drill from before lessons is never served, so it holds nothing back"
    );
    Ok(())
}

#[tokio::test]
async fn taking_queues_no_build_while_a_build_of_the_shelf_is_under_way() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };
    Ready::new("first", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    let under_way = queue_build(
        &pool,
        &build_spec(&program, &lessons[1], python.clone()),
        "under way",
    )
    .await?;
    let pending = take(&pool, shelf, None).await?.expect("the first drill");
    assert_eq!(pending.drill.drill.id, "first");
    assert_eq!(pending.next_build, None, "a build is pending");
    assert_eq!(builds(&pool).await?, 1);

    JobStore::new(pool.clone())
        .claim(&under_way)
        .await?
        .expect("the build is claimed");
    Ready {
        at: 200,
        ..Ready::new("second", &program, Some(lessons[0].as_str()))
    }
    .put(&pool)
    .await?;
    let running = take(&pool, shelf, None).await?.expect("the second drill");
    assert_eq!(running.drill.drill.id, "second");
    assert_eq!(running.next_build, None, "a build is running");
    assert_eq!(builds(&pool).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_build_of_another_shelf_or_one_that_has_ended_does_not_hold_the_next_back() -> Result<()>
{
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let (other, others) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    Ready::new("only", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    queue_build(
        &pool,
        &build_spec(&program, &lessons[1], Material::code("javascript")),
        "another language",
    )
    .await?;
    queue_build(
        &pool,
        &build_spec(&other, &others[1], python.clone()),
        "another course",
    )
    .await?;
    let failed = queue_build(
        &pool,
        &build_spec(&program, &lessons[1], python.clone()),
        "failed",
    )
    .await?;
    end(&pool, &failed, "failed").await?;
    let completed = queue_build(
        &pool,
        &build_spec(&program, &lessons[2], python.clone()),
        "completed",
    )
    .await?;
    end(&pool, &completed, "completed").await?;
    assert_eq!(builds(&pool).await?, 4);

    let taken = take(
        &pool,
        Shelf {
            program_id: &program,
            material: &python,
        },
        None,
    )
    .await?
    .expect("the only drill");
    assert!(taken.next_build.is_some());
    assert_eq!(builds(&pool).await?, 5);
    Ok(())
}

#[tokio::test]
async fn a_course_of_one_lesson_drills_it_again() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    for skipped in &lessons[1..] {
        set_state(&pool, skipped, "skipped").await?;
    }
    Ready::new("only", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    let python = Material::code("python");
    let taken = take(
        &pool,
        Shelf {
            program_id: &program,
            material: &python,
        },
        None,
    )
    .await?
    .expect("the only drill");
    let queued = spec_of(&queued_for(&pool, &taken).await?)?;
    assert_eq!(queued.lesson_id.as_deref(), Some(lessons[0].as_str()));
    Ok(())
}

#[tokio::test]
async fn with_no_lesson_left_to_drill_on_the_material_no_build_is_queued() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    for lesson in &lessons {
        record_unfit(&pool, &program, lesson, &python).await?;
    }
    Ready::new("only", &program, Some(lessons[0].as_str()))
        .put(&pool)
        .await?;
    let taken = take(
        &pool,
        Shelf {
            program_id: &program,
            material: &python,
        },
        None,
    )
    .await?
    .expect("the drill is given all the same");
    assert_eq!(taken.drill.drill.id, "only");
    assert_eq!(taken.next_build, None);
    assert_eq!(builds(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn queueing_a_build_twice_with_one_seed_finds_the_same_job() -> Result<()> {
    let pool = pool().await?;
    let first = queue_build(&pool, &lesson_spec(), "refill").await?;
    let again = queue_build(&pool, &lesson_spec(), "refill").await?;
    assert_eq!(first, again);
    assert_eq!(builds(&pool).await?, 1);
    let other = queue_build(&pool, &lesson_spec(), "another").await?;
    assert_ne!(first, other);

    let mut different = lesson_spec();
    different.lines += 1;
    let error = queue_build(&pool, &different, "refill").await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("reused")),
        "{error:?}"
    );
    let job = JobStore::new(pool.clone()).get(&first).await?;
    assert_eq!(job.progress_total, 7);
    assert_eq!(job.subject_id.as_deref(), Some("drill:course:code:python"));
    assert_eq!(spec_of(&job)?, lesson_spec());
    Ok(())
}

#[tokio::test]
async fn a_spec_that_names_no_drill_is_refused_before_a_job_is_queued() -> Result<()> {
    let pool = pool().await?;
    for bad in [
        DrillSpec {
            material: Material::code("klingon"),
            ..lesson_spec()
        },
        DrillSpec {
            patterns: Vec::new(),
            ..lesson_spec()
        },
        DrillSpec {
            patterns: vec!["errors.unwrap_on_fallible".into()],
            ..lesson_spec()
        },
        DrillSpec {
            patterns: vec!["no.such_pattern".into()],
            ..lesson_spec()
        },
        DrillSpec {
            lines: 0,
            ..lesson_spec()
        },
        DrillSpec {
            program_id: None,
            ..lesson_spec()
        },
        DrillSpec {
            lesson_id: None,
            ..lesson_spec()
        },
    ] {
        let error = queue_build(&pool, &bad, "bad").await.unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(_)), "{bad:?}");
    }
    assert_eq!(builds(&pool).await?, 0);
    Ok(())
}

#[test]
fn a_request_is_checked_for_its_material_its_size_and_its_patterns() {
    assert!(check_request(&Material::code("python"), 40, 2).is_ok());
    assert!(check_request(&Material::Passage, 4, 2).is_ok());
    for (material, lines, patterns, says) in [
        (
            Material::code("klingon"),
            40,
            2,
            "klingon is not a language drills are written in.",
        ),
        (
            Material::code("python"),
            0,
            2,
            "A drill needs at least one line of code.",
        ),
        (
            Material::Passage,
            0,
            2,
            "A drill needs at least one sentence to review.",
        ),
        (
            Material::code("python"),
            40,
            0,
            "A drill needs at least one pattern to plant.",
        ),
    ] {
        let error = check_request(&material, lines, patterns).unwrap_err();
        assert!(
            matches!(&error, AppError::InvalidInput(message) if message == says),
            "{error:?}"
        );
    }
}

#[test]
fn a_spec_without_a_lesson_reads_as_none_and_one_with_a_lesson_round_trips() {
    let plain = serde_json::to_value(spec()).expect("a spec is JSON");
    assert!(plain.get("lessonId").is_none(), "{plain}");
    let read: DrillSpec = serde_json::from_value(plain).expect("a spec without a lesson");
    assert_eq!(read, spec());
    assert_eq!(read.lesson_id, None);

    let old = json!({
        "language": "python",
        "domain": null,
        "lines": 40,
        "patterns": ["correctness.wrong_operator"],
        "programId": null,
    });
    let read: DrillSpec = serde_json::from_value(old).expect("a spec from before lessons");
    assert_eq!(read.lesson_id, None);

    let built = lesson_spec();
    let stored = serde_json::to_value(&built).expect("a spec is JSON");
    assert_eq!(stored["lessonId"], "lesson");
    let read: DrillSpec = serde_json::from_value(stored).expect("a spec with a lesson");
    assert_eq!(read, built);
}

#[test]
fn every_spec_names_its_course_and_a_lesson_of_it() {
    let passage = DrillSpec {
        lesson_id: Some("lesson".into()),
        ..passage_spec("course")
    };
    for built in [lesson_spec(), passage] {
        assert!(built.validate().is_ok(), "{built:?}");
        let no_lesson = DrillSpec {
            lesson_id: None,
            ..built.clone()
        };
        assert!(
            matches!(no_lesson.validate(), Err(AppError::InvalidInput(message)) if message.contains("lesson")),
            "{no_lesson:?}"
        );
        for bad in [
            DrillSpec {
                lesson_id: Some("  ".into()),
                ..built.clone()
            },
            DrillSpec {
                program_id: None,
                ..built.clone()
            },
            DrillSpec {
                program_id: Some("  ".into()),
                ..built.clone()
            },
        ] {
            assert!(
                matches!(bad.validate(), Err(AppError::InvalidInput(_))),
                "{bad:?}"
            );
        }
    }
}

#[test]
fn a_shelf_is_a_course_and_a_material_and_its_builds_share_a_subject() -> Result<()> {
    let code = lesson_spec();
    let python = Material::code("python");
    assert_eq!(
        Shelf::of(&code)?,
        Shelf {
            program_id: "course",
            material: &python,
        }
    );
    assert_eq!(Shelf::of(&code)?.subject(), "drill:course:code:python");
    let passage = DrillSpec {
        lesson_id: Some("lesson".into()),
        ..passage_spec("course")
    };
    assert_eq!(Shelf::of(&passage)?.subject(), "drill:course:passage");
    for lost in [None, Some("  ".to_owned())] {
        let spec = DrillSpec {
            program_id: lost,
            ..lesson_spec()
        };
        assert!(
            matches!(Shelf::of(&spec), Err(AppError::InvalidInput(_))),
            "{spec:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_build_under_way_is_found_again_for_its_lesson_or_for_any() -> Result<()> {
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    let javascript = Material::code("javascript");
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };
    let store = JobStore::new(pool.clone());

    let first = ensure_build(&pool, shelf, None, 40, 2).await?;
    let job = store.get(&first).await?;
    assert_eq!(job.kind, DRILL_BUILD);
    assert_eq!(job.subject_id, Some(format!("drill:{program}:code:python")));
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(
        spec_of(&job)?,
        DrillSpec {
            material: python.clone(),
            lines: 40,
            patterns: pattern_ids(&python, &[], now(), 2),
            program_id: Some(program.clone()),
            lesson_id: Some(lessons[0].clone()),
        },
        "the lesson next up, with the patterns the schedule offers"
    );
    assert_eq!(ensure_build(&pool, shelf, None, 40, 2).await?, first);

    let second = ensure_build(&pool, shelf, Some(lessons[1].as_str()), 40, 2).await?;
    assert_ne!(second, first);
    assert_eq!(
        spec_of(&store.get(&second).await?)?.lesson_id.as_deref(),
        Some(lessons[1].as_str())
    );
    assert_eq!(
        ensure_build(&pool, shelf, Some(lessons[1].as_str()), 40, 2).await?,
        second
    );
    assert_eq!(
        ensure_build(&pool, shelf, Some(lessons[0].as_str()), 40, 2).await?,
        first
    );

    let elsewhere = ensure_build(
        &pool,
        Shelf {
            program_id: &program,
            material: &javascript,
        },
        None,
        40,
        2,
    )
    .await?;
    assert!(![&first, &second].contains(&&elsewhere));
    assert_eq!(
        store.get(&elsewhere).await?.subject_id,
        Some(format!("drill:{program}:code:javascript"))
    );

    end(&pool, &first, "failed").await?;
    let again = ensure_build(&pool, shelf, Some(lessons[0].as_str()), 40, 2).await?;
    assert!(![&first, &second, &elsewhere].contains(&&again));
    assert_eq!(
        spec_of(&store.get(&again).await?)?.lesson_id.as_deref(),
        Some(lessons[0].as_str())
    );
    Ok(())
}

#[tokio::test]
async fn ensuring_a_build_with_no_lesson_to_drill_is_refused_unless_one_is_asked_for() -> Result<()>
{
    let pool = pool().await?;
    let (program, lessons) = course_with_lessons(&pool).await?;
    let python = Material::code("python");
    for lesson in &lessons {
        record_unfit(&pool, &program, lesson, &python).await?;
    }
    let shelf = Shelf {
        program_id: &program,
        material: &python,
    };
    let error = ensure_build(&pool, shelf, None, 40, 2).await.unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(_)), "{error:?}");
    assert_eq!(builds(&pool).await?, 0);

    let asked = ensure_build(&pool, shelf, Some(lessons[2].as_str()), 40, 2).await?;
    let queued = spec_of(&JobStore::new(pool.clone()).get(&asked).await?)?;
    assert_eq!(queued.lesson_id.as_deref(), Some(lessons[2].as_str()));
    assert_eq!(builds(&pool).await?, 1);
    Ok(())
}
