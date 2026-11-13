//! The migration: the tables, what deleting a drill or a course does, the constraints, and the indexes the bank's
//! queries use.

use super::support::{code, files, planted, proved, quiet, review, spec, unchecked, MODEL, SUITE};
use crate::drills::build::Evidence;
use crate::drills::repository::{self, DrillStatus, NewDrill};
use crate::repository::LearningRepository;
use crate::tests::{fixture, pool};
use lattice_core::domain::learning::drills::{Drill, Material};
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::{Row, SqlitePool};

async fn add(database: &Database, id: &str, program_id: Option<&str>) -> Result<()> {
    let proved = proved();
    let drill = Drill {
        id: id.into(),
        material: Material::code("python"),
        title: "A limiter".into(),
        files: proved.files.clone(),
        key: proved.planted_key(&unchecked()),
    };
    let mut spec = spec();
    spec.program_id = program_id.map(str::to_owned);
    let evidence = Evidence::new(
        &code(),
        &review(),
        &planted(),
        &proved,
        &unchecked(),
        &quiet(),
    );
    let mut tx = database.begin_write().await?;
    repository::insert_in(
        &mut tx,
        &NewDrill {
            drill: &drill,
            clean_files: &files(),
            spec: &spec,
            domain: "rate limiting",
            suite: Some(SUITE),
            source: None,
            runner: Some("builtin-python"),
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn scalar(pool: &SqlitePool, sql: &str) -> Result<i64> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .map_err(AppError::from)
}

#[tokio::test]
async fn the_tables_and_their_indexes_exist() -> Result<()> {
    let pool = pool().await?;
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE name LIKE 'learning_drill%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;
    for name in [
        "learning_drill_code_checks",
        "learning_drill_lesson_fits",
        "learning_drill_lesson_fits_lesson",
        "learning_drill_lesson_schedules",
        "learning_drill_lesson_schedules_lesson",
        "learning_drill_passage_checks",
        "learning_drill_passage_checks_version",
        "learning_drill_pattern_schedules",
        "learning_drill_patterns",
        "learning_drill_patterns_pattern",
        "learning_drills",
        "learning_drills_lesson",
        "learning_drills_shelf",
    ] {
        assert!(
            names.iter().any(|found| found == name),
            "{name} in {names:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_drill_comes_back_as_it_went_in_with_a_row_for_each_pattern() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    add(&database, "drill", None).await?;
    let mut connection = pool.acquire().await?;
    let stored = repository::find_in(&mut connection, "drill")
        .await?
        .expect("the drill");
    drop(connection);
    assert_eq!(stored.drill.id, "drill");
    assert_eq!(stored.drill.files, proved().files);
    assert_eq!(stored.drill.key, proved().planted_key(&unchecked()));
    assert_eq!(stored.clean_files, files());
    assert_eq!(stored.spec, spec());
    assert_eq!(stored.status, DrillStatus::Ready);
    assert_eq!(
        stored.evidence,
        Evidence::new(
            &code(),
            &review(),
            &planted(),
            &proved(),
            &unchecked(),
            &quiet()
        )
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT count(*) FROM learning_drill_patterns WHERE drill_id='drill'"
        )
        .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn deleting_a_drill_deletes_its_pattern_rows() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    add(&database, "gone", None).await?;
    add(&database, "kept", None).await?;
    sqlx::query("DELETE FROM learning_drills WHERE id='gone'")
        .execute(&pool)
        .await?;
    assert_eq!(
        scalar(
            &pool,
            "SELECT count(*) FROM learning_drill_patterns WHERE drill_id='gone'"
        )
        .await?,
        0
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT count(*) FROM learning_drill_patterns WHERE drill_id='kept'"
        )
        .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn deleting_a_course_leaves_its_drills_with_no_course() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let program = fixture();
    LearningRepository::new(database.clone())
        .create(&program)
        .await?;
    let id = program.summary.id.clone();
    add(&database, "about-the-course", Some(&id)).await?;
    let before: Option<String> =
        sqlx::query_scalar("SELECT program_id FROM learning_drills WHERE id='about-the-course'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(before.as_deref(), Some(id.as_str()));

    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&id)
        .execute(&pool)
        .await?;
    let after: Option<String> =
        sqlx::query_scalar("SELECT program_id FROM learning_drills WHERE id='about-the-course'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(after, None);
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM learning_drills").await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_course_that_is_not_there_is_not_kept() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    add(&database, "orphan", Some("no-such-course")).await?;
    let program: Option<String> =
        sqlx::query_scalar("SELECT program_id FROM learning_drills WHERE id='orphan'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(program, None);
    Ok(())
}

#[tokio::test]
async fn a_row_must_agree_with_its_status_and_pair_its_suite_with_its_runner() -> Result<()> {
    let pool = pool().await?;
    let insert = |status: &'static str,
                  taken_at: Option<i64>,
                  suite: Option<&'static str>,
                  runner: Option<&'static str>| {
        let pool = pool.clone();
        async move {
            sqlx::query("INSERT INTO learning_drills(id,language,title,domain,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES('d','python','t','d','{}','[]','[]','[]','{}',?,?,'m',?,1,?)")
                .bind(suite)
                .bind(runner)
                .bind(status)
                .bind(taken_at)
                .execute(&pool)
                .await
        }
    };
    assert!(
        insert("ready", Some(5), None, None).await.is_err(),
        "ready and taken"
    );
    assert!(
        insert("taken", None, None, None).await.is_err(),
        "taken at no time"
    );
    assert!(
        insert("retired", None, None, None).await.is_err(),
        "an unknown status"
    );
    assert!(
        insert("ready", None, Some("suite"), None).await.is_err(),
        "a suite with no runner"
    );
    assert!(
        insert("ready", None, None, Some("builtin-python"))
            .await
            .is_err(),
        "a runner with no suite"
    );
    assert!(insert("taken", Some(5), None, None).await.is_ok());
    Ok(())
}

#[tokio::test]
async fn the_banks_queries_use_their_indexes() -> Result<()> {
    let pool = pool().await?;
    let oldest = format!(
        "EXPLAIN QUERY PLAN SELECT id {}",
        repository::OLDEST_READY_ON
    );
    let any = format!("EXPLAIN QUERY PLAN {}", repository::ANY_READY_IN);
    for (query, index) in [
        (
            sqlx::query(sqlx::AssertSqlSafe(oldest.as_str())).bind("p").bind("code").bind("python").bind(None::<&str>),
            "learning_drills_shelf",
        ),
        (
            sqlx::query(sqlx::AssertSqlSafe(oldest.as_str())).bind("p").bind("code").bind("python").bind(Some("l")),
            "learning_drills_shelf",
        ),
        (
            sqlx::query(sqlx::AssertSqlSafe(any.as_str())).bind("p").bind("code").bind("python"),
            "learning_drills_shelf",
        ),
        (
            sqlx::query("EXPLAIN QUERY PLAN SELECT drill_id FROM learning_drill_patterns WHERE pattern_id='errors.swallowed_exception'"),
            "learning_drill_patterns_pattern",
        ),
        (
            sqlx::query("EXPLAIN QUERY PLAN SELECT id FROM learning_drills WHERE program_id='p'"),
            "learning_drills_shelf",
        ),
        (
            sqlx::query("EXPLAIN QUERY PLAN SELECT id FROM learning_drills WHERE lesson_id='l'"),
            "learning_drills_lesson",
        ),
    ] {
        let plan: Vec<String> = query
            .fetch_all(&pool)
            .await?
            .iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect();
        assert!(
            plan.iter().any(|step| step.contains(index)),
            "{index} plans {plan:?}"
        );
    }
    Ok(())
}
