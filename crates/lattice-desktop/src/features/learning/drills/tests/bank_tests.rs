//! The bank: one drill ready per language, the build of the next queued when one is taken, and the rotation of
//! domains.

use super::support::{
    code, files, held_back, planted, proved, quiet, review, schedule_pattern, spec, MODEL, SUITE,
};
use crate::features::learning::drills::bank::{
    least_recently_used, next_domain, queue_build, take, DOMAINS,
};
use crate::features::learning::drills::build::Evidence;
use crate::features::learning::drills::jobs::{spec_of, DRILL_BUILD};
use crate::features::learning::drills::repository::{self, DrillStatus, NewDrill};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::tests::pool;
use lattice_core::domain::learning::drills::{targets, Drill, Material, PatternDue};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};
use serde_json::json;
use sqlx::SqlitePool;

/// Puts a ready drill of `language` into the bank, built at `at`, from a spec with `lines` and `patterns`.
async fn insert(
    pool: &SqlitePool,
    id: &str,
    language: &str,
    domain: &str,
    at: i64,
    lines: u32,
    patterns: &[&str],
) -> Result<()> {
    let proved = proved();
    let drill = Drill {
        id: id.into(),
        material: Material::code(language),
        title: "A limiter".into(),
        files: proved.files.clone(),
        key: proved.planted_key(),
    };
    let mut spec = spec();
    spec.material = Material::code(language);
    spec.lines = lines;
    spec.patterns = patterns
        .iter()
        .map(|pattern| (*pattern).to_owned())
        .collect();
    let evidence = Evidence::new(&code(), &review(), &planted(), &proved, &quiet());
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::insert_in(
        &mut connection,
        &NewDrill {
            drill: &drill,
            clean_files: &files(),
            spec: &spec,
            domain,
            suite: Some(SUITE),
            source: None,
            runner: Some("fake-python"),
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await?;
    sqlx::query("UPDATE learning_drills SET created_at=? WHERE id=?")
        .bind(at)
        .bind(id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

async fn builds(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=?")
        .bind(DRILL_BUILD)
        .fetch_one(pool)
        .await
        .map_err(db)
}

#[tokio::test]
async fn with_nothing_ready_nothing_is_given_and_nothing_is_queued() -> Result<()> {
    let pool = pool().await?;
    insert(
        &pool,
        "other",
        "javascript",
        DOMAINS[0],
        1,
        40,
        &["readability.magic_number"],
    )
    .await?;
    assert_eq!(take(&pool, "python").await?, None);
    assert_eq!(builds(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn the_oldest_ready_drill_of_the_language_is_taken_and_marked() -> Result<()> {
    let pool = pool().await?;
    insert(
        &pool,
        "newer",
        "python",
        DOMAINS[1],
        200,
        40,
        &["readability.magic_number"],
    )
    .await?;
    insert(
        &pool,
        "older",
        "python",
        DOMAINS[0],
        100,
        40,
        &["readability.magic_number"],
    )
    .await?;
    insert(
        &pool,
        "elsewhere",
        "javascript",
        DOMAINS[0],
        50,
        40,
        &["readability.magic_number"],
    )
    .await?;

    let first = take(&pool, "python").await?.expect("a ready drill");
    assert_eq!(first.drill.drill.id, "older");
    assert_eq!(first.drill.status, DrillStatus::Taken);
    assert!(first.drill.taken_at.is_some());
    let status: String = sqlx::query_scalar("SELECT status FROM learning_drills WHERE id='older'")
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(status, "taken");

    let second = take(&pool, "python").await?.expect("a second ready drill");
    assert_eq!(second.drill.drill.id, "newer");
    assert_eq!(take(&pool, "python").await?, None, "a drill is given once");
    let elsewhere: String =
        sqlx::query_scalar("SELECT status FROM learning_drills WHERE id='elsewhere'")
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(
        elsewhere, "ready",
        "another language's drill stays in the bank"
    );
    Ok(())
}

#[tokio::test]
async fn taking_a_drill_queues_the_next_build_with_its_size_and_the_targets_of_the_schedule(
) -> Result<()> {
    let pool = pool().await?;
    insert(
        &pool,
        "older",
        "python",
        DOMAINS[0],
        100,
        70,
        &[
            "readability.magic_number",
            "correctness.wrong_operator",
            "errors.swallowed_exception",
        ],
    )
    .await?;
    let taken = take(&pool, "python").await?.expect("a ready drill");

    let job = JobStore::new(pool.clone()).get(&taken.next_build).await?;
    assert_eq!(job.kind, DRILL_BUILD);
    assert_eq!(job.subject_id.as_deref(), Some("python"));
    assert_eq!(job.status, JobStatus::Pending);
    let queued = spec_of(&job)?;
    let expected: Vec<String> = targets(
        &Material::code("python"),
        &held_back(&taken.drill.drill),
        0,
        3,
    )
    .iter()
    .map(|pattern| pattern.id.to_owned())
    .collect();
    assert_eq!(
        queued,
        DrillSpec {
            material: Material::code("python"),
            domain: None,
            lines: 70,
            patterns: expected,
            program_id: None,
            lesson_id: None,
        }
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
    insert(
        &pool,
        "older",
        "python",
        DOMAINS[0],
        100,
        70,
        &[
            "readability.magic_number",
            "correctness.wrong_operator",
            "errors.swallowed_exception",
        ],
    )
    .await?;
    let due = "concurrency.check_then_act";
    schedule_pattern(&pool, "code:python", due, 1).await?;
    schedule_pattern(&pool, "code:rust", "correctness.off_by_one", 1).await?;
    let taken = take(&pool, "python").await?.expect("a ready drill");

    let queued = spec_of(&JobStore::new(pool.clone()).get(&taken.next_build).await?)?;
    assert_eq!(queued.patterns.first().map(String::as_str), Some(due));
    let mut states = held_back(&taken.drill.drill);
    states.push(PatternDue {
        pattern: due.to_owned(),
        due_at: Some(1),
        pending: false,
    });
    let expected: Vec<String> = targets(&Material::code("python"), &states, now(), 3)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert_eq!(queued.patterns, expected);
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
async fn queueing_a_build_twice_with_one_seed_finds_the_same_job() -> Result<()> {
    let pool = pool().await?;
    let first = queue_build(&pool, &spec(), "refill").await?;
    let again = queue_build(&pool, &spec(), "refill").await?;
    assert_eq!(first, again);
    assert_eq!(builds(&pool).await?, 1);
    let other = queue_build(&pool, &spec(), "another").await?;
    assert_ne!(first, other);

    let mut different = spec();
    different.lines += 1;
    let error = queue_build(&pool, &different, "refill").await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("reused")),
        "{error:?}"
    );
    let job = JobStore::new(pool.clone()).get(&first).await?;
    assert_eq!(job.progress_total, 6);
    assert_eq!(spec_of(&job)?, spec());
    Ok(())
}

#[tokio::test]
async fn a_spec_that_names_no_drill_is_refused_before_a_job_is_queued() -> Result<()> {
    let pool = pool().await?;
    for bad in [
        DrillSpec {
            material: Material::code("klingon"),
            ..spec()
        },
        DrillSpec {
            patterns: Vec::new(),
            ..spec()
        },
        DrillSpec {
            patterns: vec!["errors.unwrap_on_fallible".into()],
            ..spec()
        },
        DrillSpec {
            patterns: vec!["no.such_pattern".into()],
            ..spec()
        },
        DrillSpec { lines: 0, ..spec() },
    ] {
        let error = queue_build(&pool, &bad, "bad").await.unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(_)), "{bad:?}");
    }
    assert_eq!(builds(&pool).await?, 0);
    Ok(())
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

    let built = DrillSpec {
        program_id: Some("course".into()),
        lesson_id: Some("lesson".into()),
        ..spec()
    };
    let stored = serde_json::to_value(&built).expect("a spec is JSON");
    assert_eq!(stored["lessonId"], "lesson");
    let read: DrillSpec = serde_json::from_value(stored).expect("a spec with a lesson");
    assert_eq!(read, built);
}

#[test]
fn a_lesson_must_say_something_and_be_of_a_course() {
    let built = DrillSpec {
        program_id: Some("course".into()),
        lesson_id: Some("lesson".into()),
        ..spec()
    };
    assert!(built.validate().is_ok());
    let blank = DrillSpec {
        lesson_id: Some("  ".into()),
        ..built.clone()
    };
    let no_course = DrillSpec {
        program_id: None,
        ..built
    };
    for bad in [blank, no_course] {
        assert!(
            matches!(bad.validate(), Err(AppError::InvalidInput(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn the_domain_used_least_recently_comes_first_and_ties_keep_the_order_of_the_list() {
    assert_eq!(least_recently_used(&[]), DOMAINS[0]);
    assert_eq!(
        least_recently_used(&[(DOMAINS[0].to_owned(), 10)]),
        DOMAINS[1]
    );
    assert_eq!(
        least_recently_used(&[(DOMAINS[1].to_owned(), 10), (DOMAINS[0].to_owned(), 20)]),
        DOMAINS[2],
        "one never used comes before any used"
    );
    let all_used = |at: &dyn Fn(usize) -> i64| -> Vec<(String, i64)> {
        DOMAINS
            .iter()
            .enumerate()
            .map(|(position, domain)| ((*domain).to_owned(), at(position)))
            .collect()
    };
    assert_eq!(
        least_recently_used(&all_used(&|_| 5)),
        DOMAINS[0],
        "ties: list order"
    );
    assert_eq!(
        least_recently_used(&all_used(&|position| 100 - position as i64)),
        DOMAINS[DOMAINS.len() - 1],
        "the oldest latest use"
    );
    // The latest use of a domain counts, not its first.
    let mut twice = all_used(&|position| position as i64);
    twice.push((DOMAINS[0].to_owned(), 1_000));
    assert_eq!(least_recently_used(&twice), DOMAINS[1]);
}

#[test]
fn the_domains_are_distinct_and_say_something() {
    for (position, domain) in DOMAINS.iter().enumerate() {
        assert!(!domain.trim().is_empty());
        assert!(
            !DOMAINS[position + 1..].contains(domain),
            "{domain} is listed twice"
        );
    }
}

#[tokio::test]
async fn the_next_domain_of_a_language_follows_the_drills_it_already_has() -> Result<()> {
    let pool = pool().await?;
    assert_eq!(next_domain(&pool, "python").await?, DOMAINS[0]);
    insert(
        &pool,
        "a",
        "python",
        DOMAINS[0],
        1,
        40,
        &["readability.magic_number"],
    )
    .await?;
    insert(
        &pool,
        "b",
        "python",
        DOMAINS[1],
        2,
        40,
        &["readability.magic_number"],
    )
    .await?;
    assert_eq!(next_domain(&pool, "python").await?, DOMAINS[2]);
    assert_eq!(
        next_domain(&pool, "javascript").await?,
        DOMAINS[0],
        "each language has its own"
    );
    // A drill that is taken still counts as the domain's latest use.
    take(&pool, "python").await?;
    assert_eq!(next_domain(&pool, "python").await?, DOMAINS[2]);
    Ok(())
}
