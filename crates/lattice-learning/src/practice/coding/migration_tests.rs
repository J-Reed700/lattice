//! The coding-sessions migration applies on a database that holds every practical kind, keeps every row, and lets
//! the new kind, the coding tables and the problem pattern schedules in.

use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// The migration under test.
const CODING_SESSIONS: i64 = 20_261_109_100_000;

/// The kinds a practical activity had before coding problems.
const EXISTING_KINDS: [&str; 10] = [
    "code_lab",
    "debugging",
    "code_review",
    "incident",
    "system_design",
    "project",
    "interview",
    "conversation",
    "writing_revision",
    "custom",
];

fn migrator() -> Migrator {
    sqlx::migrate!("../lattice-desktop/migrations")
}

async fn memory_pool() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap()
}

async fn exec(pool: &SqlitePool, statements: &[String]) {
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn activity(id: &str, kind: &str) -> String {
    format!(
        "INSERT INTO learning_practical_activities (id, program_id, lesson_id, kind, title, brief, status, practice_mode, allowed_aids_json, outcome_ids_json, source_version_ids_json, rubric_json, runtime_kind, generator_model, revision, created_at, updated_at) \
         VALUES ('{id}', 'p1', 'l1', '{kind}', 'Title', 'Brief', 'ready', 'practice', '[]', '[]', '[]', '[]', 'none', 'model', 0, 1000, 1000)"
    )
}

fn schedule(material: &str) -> String {
    format!(
        "INSERT INTO learning_drill_pattern_schedules (material, pattern_id, stability, difficulty, last_reviewed_at, due_at, interval_days, review_count, lapses) \
         VALUES ('{material}', 'pattern', 1.0, 5.0, 1000, 2000, 1, 1, 0)"
    )
}

fn course() -> Vec<String> {
    vec![
        "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES ('p1', 'An example course', 'Learn a skill', 'active', 1, 'none', 30, 'model', 1000)".into(),
        "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json, prerequisite_ids_json, project_json) \
         VALUES ('m1', 'p1', 1, 'Module', 'Summary', '[]', '[]', '{}')".into(),
        "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed, curriculum_state) \
         VALUES ('l1', 'p1', 'm1', 1, 'Lesson', 'Objective', 20, 'outline', 0, 'ready')".into(),
    ]
}

async fn database_before_the_migration() -> SqlitePool {
    let pool = memory_pool().await;
    let migrator = migrator();
    assert!(migrator.iter().any(|m| m.version == CODING_SESSIONS));
    let mut conn = pool.acquire().await.unwrap();
    conn.ensure_migrations_table("_sqlx_migrations")
        .await
        .unwrap();
    for migration in migrator
        .iter()
        .filter(|m| m.version < CODING_SESSIONS && !m.migration_type.is_down_migration())
    {
        conn.apply("_sqlx_migrations", migration).await.unwrap();
    }
    drop(conn);
    pool
}

#[tokio::test]
async fn the_migration_keeps_every_existing_practical_kind_and_accepts_exercise() {
    let pool = database_before_the_migration().await;
    exec(&pool, &course()).await;
    let kinds = EXISTING_KINDS
        .iter()
        .enumerate()
        .map(|(index, kind)| activity(&format!("a{index}"), kind))
        .collect::<Vec<_>>();
    exec(&pool, &kinds).await;
    assert!(
        sqlx::query(sqlx::AssertSqlSafe(activity("early", "exercise")))
            .execute(&pool)
            .await
            .is_err(),
        "the kind does not exist before the migration"
    );
    let before = count(&pool, "learning_practical_activities").await;
    assert_eq!(before, EXISTING_KINDS.len() as i64);

    migrator().run(&pool).await.unwrap();

    assert_eq!(count(&pool, "learning_practical_activities").await, before);
    let kept: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM learning_practical_activities ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for kind in EXISTING_KINDS {
        assert!(kept.iter().any(|each| each == kind), "{kind} kept");
    }
    exec(&pool, &[activity("coding", "exercise")]).await;
    assert!(
        sqlx::query(sqlx::AssertSqlSafe(activity("bogus", "not_a_kind")))
            .execute(&pool)
            .await
            .is_err(),
        "the check still refuses other kinds"
    );
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn the_migration_adds_the_coding_tables_and_run_columns() {
    let pool = memory_pool().await;
    migrator().run(&pool).await.unwrap();
    for table in [
        "learning_exercise_sessions",
        "learning_exercise_session_events",
        "learning_coding_examples",
        "learning_scorecards",
        "learning_exercise_debriefs",
        "learning_practice_profiles",
        "learning_practice_fits",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    let run_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('learning_practical_runs')")
            .fetch_all(&pool)
            .await
            .unwrap();
    for column in ["exercise_session_id", "scope"] {
        assert!(run_columns.iter().any(|each| each == column), "{column}");
    }
    let check_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('learning_practical_run_checks')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(check_columns.iter().any(|each| each == "detail_json"));
}

#[tokio::test]
async fn problem_pattern_schedules_are_accepted_beside_the_drills_and_other_materials_are_not() {
    let pool = database_before_the_migration().await;
    exec(&pool, &[schedule("code:python"), schedule("passage")]).await;
    assert!(
        sqlx::query(sqlx::AssertSqlSafe(schedule("problem:python")))
            .execute(&pool)
            .await
            .is_err(),
        "the drills' check refuses the new material before the migration"
    );
    let before = count(&pool, "learning_drill_pattern_schedules").await;

    migrator().run(&pool).await.unwrap();

    assert_eq!(
        count(&pool, "learning_drill_pattern_schedules").await,
        before
    );
    exec(&pool, &[schedule("problem:python")]).await;
    for refused in ["bogus", "problem:", "problem", "code:"] {
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(schedule(refused)))
                .execute(&pool)
                .await
                .is_err(),
            "{refused}"
        );
    }
    assert_eq!(
        count(&pool, "learning_drill_pattern_schedules").await,
        before + 1
    );
}

#[tokio::test]
async fn deleting_a_session_removes_its_events_and_examples_and_keeps_its_runs() {
    let pool = memory_pool().await;
    migrator().run(&pool).await.unwrap();
    exec(&pool, &course()).await;
    exec(&pool, &[activity("coding", "exercise")]).await;
    exec(
        &pool,
        &[
            "INSERT INTO learning_simulation_sessions (id, program_id, activity_id, activity_revision, activity_snapshot_json, practice_session_id, operation_id, payload_hash, learner_role, counterpart_role, status, revision, created_at, updated_at, submitted_at) \
             VALUES ('s1', 'p1', 'coding', 0, '{}', NULL, 'op1', 'hash', 'Candidate', 'Coach', 'active', 0, 1000, 1000, NULL)".into(),
            "INSERT INTO learning_exercise_sessions (id, program_id, activity_id, kind, form, setting, stage, draft_json, created_at) \
             VALUES ('s1', 'p1', 'coding', 'program', 'build', 'coached', 'understand', '[]', 1000)".into(),
            "INSERT INTO learning_exercise_session_events (session_id, ordinal, kind, payload_json, at) VALUES ('s1', 0, 'stage', '{}', 1000)".into(),
            "INSERT INTO learning_coding_examples (session_id, ordinal, args_json, expected, verdict, catches_json, created_at) \
             VALUES ('s1', 1, '[]', '1', 'right', '[]', 1000)".into(),
            "INSERT INTO learning_practical_runs (id, program_id, activity_id, activity_revision, activity_snapshot_json, operation_id, payload_hash, status, learner_files_json, created_at, exercise_session_id, scope) \
             VALUES ('r1', 'p1', 'coding', 0, '{}', 'op2', 'hash', 'running', '[]', 1000, 's1', 'all')".into(),
        ],
    )
    .await;
    for refused in [
        "INSERT INTO learning_exercise_session_events (session_id, ordinal, kind, payload_json, at) VALUES ('s1', 1, 'unknown', '{}', 1)",
        "INSERT INTO learning_coding_examples (session_id, ordinal, args_json, expected, verdict, catches_json, created_at) VALUES ('s1', 2, '[]', '1', 'maybe', '[]', 1)",
        "UPDATE learning_exercise_sessions SET stage = 'sleep'",
        "UPDATE learning_practical_runs SET scope = 'some'",
    ] {
        assert!(sqlx::query(refused).execute(&pool).await.is_err(), "{refused}");
    }

    exec(
        &pool,
        &["DELETE FROM learning_exercise_sessions WHERE id = 's1'".into()],
    )
    .await;

    assert_eq!(count(&pool, "learning_exercise_session_events").await, 0);
    assert_eq!(count(&pool, "learning_coding_examples").await, 0);
    let session: Option<String> = sqlx::query_scalar(
        "SELECT exercise_session_id FROM learning_practical_runs WHERE id = 'r1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(session, None, "the run stays, unlinked");
}

#[tokio::test]
async fn the_session_tables_are_named_for_exercise_sessions_and_the_old_names_are_gone() {
    let pool = memory_pool().await;
    migrator().run(&pool).await.unwrap();
    for table in [
        "learning_exercise_sessions",
        "learning_exercise_session_events",
        "learning_exercise_debriefs",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap();
    for gone in [
        "learning_coding_sessions",
        "learning_coding_events",
        "learning_coding_debriefs",
    ] {
        assert!(!tables.iter().any(|each| each == gone), "{gone}");
    }
    let session_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('learning_exercise_sessions')")
            .fetch_all(&pool)
            .await
            .unwrap();
    for column in ["kind", "form"] {
        assert!(
            session_columns.iter().any(|each| each == column),
            "{column}"
        );
    }
    assert!(!session_columns.iter().any(|each| each == "task"));
    let run_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('learning_practical_runs')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(run_columns.iter().any(|each| each == "exercise_session_id"));
    assert!(!run_columns.iter().any(|each| each == "coding_session_id"));
}
