#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::unwrap_used
)]

use lattice::features::learning::repository::LearningRepository;
use sqlx::migrate::Migrate;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::collections::BTreeMap;
use std::str::FromStr;

/// Version of the migration that switches the learning delete actions.
const DELETE_CASCADES_VERSION: i64 = 20_261_016_100_000;

#[tokio::test]
async fn test_course_cascade_deletion_with_all_assessment_and_revisions_data(
) -> Result<(), Box<dyn std::error::Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.pragma("foreign_keys", "ON");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    // Run all migrations including the cascade fix migration
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Ensure foreign keys are active
    let fk_status: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        fk_status, 1,
        "Foreign keys must be enabled on SQLite connection"
    );

    // Seed Course A and Course B
    for course_id in ["course_a", "course_b"] {
        seed_course(&pool, course_id).await?;
    }

    // Verify both courses exist
    let course_count: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_programs;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(course_count, 2);

    // Delete Course A via production LearningRepository
    let repo = LearningRepository::new(pool.clone());
    repo.delete("course_a").await?;

    // 1. Verify Course A is gone and Course B remains
    let remaining_courses: Vec<String> =
        sqlx::query("SELECT id FROM learning_programs ORDER BY id;")
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|r| r.get("id"))
            .collect();
    assert_eq!(remaining_courses, vec!["course_b".to_string()]);

    // 2. Verify all 9 rebuilt tables have 0 rows for course_a and preserved rows for course_b
    let count_bp_slots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_assessment_blueprint_slots WHERE blueprint_id LIKE '%course_a%';"
    ).fetch_one(&pool).await?;
    assert_eq!(count_bp_slots, 0);

    let count_cand: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_assessment_candidates WHERE program_id = 'course_a';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count_cand, 0);

    let count_forms: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_assessment_forms WHERE program_id = 'course_a';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count_forms, 0);

    let count_curriculum: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id = 'course_a';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count_curriculum, 0);

    let count_sessions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_practice_sessions WHERE program_id = 'course_a';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count_sessions, 0);

    // Verify Course B data is completely intact
    let b_sessions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_practice_sessions WHERE program_id = 'course_b';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(b_sessions, 2);

    let b_curriculum: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id = 'course_b';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(b_curriculum, 2);

    // 3. Verify PRAGMA foreign_key_check is completely clean
    let fk_violations = sqlx::query("PRAGMA foreign_key_check;")
        .fetch_all(&pool)
        .await?;
    assert!(
        fk_violations.is_empty(),
        "Expected 0 FK violations after cascade delete, got {} violations",
        fk_violations.len()
    );

    Ok(())
}

#[tokio::test]
async fn test_delete_cascades_migration_preserves_existing_rows(
) -> Result<(), Box<dyn std::error::Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.pragma("foreign_keys", "ON");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let migrator = sqlx::migrate!("./migrations");
    assert!(
        migrator
            .iter()
            .any(|m| m.version == DELETE_CASCADES_VERSION),
        "the delete-cascades migration must exist"
    );

    // Bring the database to the schema just before the migration under test.
    {
        let mut conn = pool.acquire().await?;
        conn.ensure_migrations_table().await?;
        for migration in migrator.iter().filter(|m| {
            m.version < DELETE_CASCADES_VERSION && !m.migration_type.is_down_migration()
        }) {
            conn.apply(migration).await?;
        }
    }

    for course_id in ["course_a", "course_b"] {
        seed_course(&pool, course_id).await?;
    }
    let before = learning_row_counts(&pool).await?;
    assert_eq!(before["learning_assessment_answer_keys"], 2);
    assert_eq!(before["learning_assessment_response_revisions"], 2);
    assert_eq!(before["learning_curriculum_revisions"], 4);
    assert_eq!(before["learning_practice_sessions"], 4);

    // Apply the rest, including the delete-cascades migration.
    migrator.run(&pool).await?;

    // Later migrations may add learning tables; they add no rows.
    let (after, created): (BTreeMap<_, _>, BTreeMap<_, _>) = learning_row_counts(&pool)
        .await?
        .into_iter()
        .partition(|(table, _)| before.contains_key(table));
    assert_eq!(
        before, after,
        "the migration must not drop or add learning rows"
    );
    assert!(
        created.values().all(|rows| *rows == 0),
        "a table a later migration creates starts empty: {created:?}"
    );

    let fk_status: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        fk_status, 1,
        "foreign keys must stay enabled after the migration"
    );
    let violations = sqlx::query("PRAGMA foreign_key_check;")
        .fetch_all(&pool)
        .await?;
    assert!(
        violations.is_empty(),
        "migration left {} FK violations",
        violations.len()
    );

    // The migrating connection sees the new actions without reconnecting.
    let predecessor_action: String = sqlx::query_scalar(
        "SELECT on_delete FROM pragma_foreign_key_list('learning_curriculum_revisions') WHERE \"from\" = 'predecessor_id';",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(predecessor_action, "SET NULL");
    LearningRepository::new(pool.clone())
        .delete("course_a")
        .await?;
    let after_delete = learning_row_counts(&pool).await?;
    assert_eq!(after_delete["learning_programs"], 1);
    assert_eq!(after_delete["learning_assessment_answer_keys"], 1);
    assert_eq!(after_delete["learning_assessment_item_results"], 1);
    assert_eq!(after_delete["learning_practice_sessions"], 2);

    Ok(())
}

/// Row count of every `learning_*` table, keyed by table name.
async fn learning_row_counts(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, i64>, Box<dyn std::error::Error>> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name LIKE 'learning\\_%' ESCAPE '\\' ORDER BY name;",
    )
    .fetch_all(pool)
    .await?;
    let mut counts = BTreeMap::new();
    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM \"{table}\";"))
            .fetch_one(pool)
            .await?;
        counts.insert(table, count);
    }
    Ok(counts)
}

/// Seeds one course with a row in every table whose delete action the
/// cascade migration changes, plus their child rows.
async fn seed_course(pool: &SqlitePool, course_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES (?, 'Course', 'Goal', 'active', 1, 'none', 30, 'gpt-4o', 1000);"
    )
    .bind(course_id)
    .execute(pool)
    .await?;

    let module_id = format!("mod_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json, prerequisite_ids_json, project_json) \
         VALUES (?, ?, 1, 'Mod 1', 'Summary', '[]', '[]', '{}');"
    )
    .bind(&module_id)
    .bind(course_id)
    .execute(pool)
    .await?;

    let lesson_id = format!("les_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed, curriculum_state) \
         VALUES (?, ?, ?, 1, 'Les 1', 'Obj', 20, 'ready', 0, 'ready');"
    )
    .bind(&lesson_id)
    .bind(course_id)
    .bind(&module_id)
    .execute(pool)
    .await?;

    let outcome_id = format!("out_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_outcome_definitions (id, program_id, module_id, lesson_id, title, description, ordinal, created_at) \
         VALUES (?, ?, ?, ?, 'Outcome 1', 'Desc', 1, 1000);"
    )
    .bind(&outcome_id)
    .bind(course_id)
    .bind(&module_id)
    .bind(&lesson_id)
    .execute(pool)
    .await?;

    let bp_id = format!("bp_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_assessment_blueprints (id, program_id, revision, purpose, title, instructions, expected_minutes, allowed_aids_json, passing_score, rubric_json, feedback_timing, source_version_ids_json, status, change_reason, created_at) \
         VALUES (?, ?, 1, 'practice', 'BP 1', 'Inst', 30, '[]', 0.7, '{}', 'immediate', '[]', 'accepted', 'initial', 1000);"
    )
    .bind(&bp_id)
    .bind(course_id)
    .execute(pool)
    .await?;

    // 1. Blueprint slot
    sqlx::query(
        "INSERT INTO learning_assessment_blueprint_slots (blueprint_id, blueprint_revision, ordinal, outcome_id, format, difficulty, difficulty_max, points) \
         VALUES (?, 1, 0, ?, 'single_choice', 1, 2, 5.0);"
    )
    .bind(&bp_id)
    .bind(&outcome_id)
    .execute(pool)
    .await?;

    // 2. Candidate
    let cand_id = format!("cand_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_assessment_candidates (id, program_id, outcome_id, format, difficulty, prompt, options_json, rubric_json, source_version_ids_json, authored_by, content_sha256, created_at) \
         VALUES (?, ?, ?, 'single_choice', 1, 'Prompt', '[]', '{}', '[]', 'model', ?, 1000);"
    )
    .bind(&cand_id)
    .bind(course_id)
    .bind(&outcome_id)
    .bind(format!("sha_{}", course_id))
    .execute(pool)
    .await?;

    // 3. Candidate outcome
    sqlx::query(
        "INSERT INTO learning_assessment_candidate_outcomes (candidate_id, outcome_id) \
         VALUES (?, ?);",
    )
    .bind(&cand_id)
    .bind(&outcome_id)
    .execute(pool)
    .await?;

    // 4. Form
    let form_id = format!("form_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_assessment_forms (id, program_id, blueprint_id, blueprint_revision, status, revision, title, instructions, expected_minutes, allowed_aids_json, purpose, passing_score, rubric_json, feedback_timing, source_version_ids_json, created_at, updated_at, submitted_at) \
         VALUES (?, ?, ?, 1, 'submitted', 0, 'Form 1', 'Inst', 30, '[]', 'practice', 0.7, '{}', 'immediate', '[]', 1000, 1000, 1000);"
    )
    .bind(&form_id)
    .bind(course_id)
    .bind(&bp_id)
    .execute(pool)
    .await?;

    // 5. Form item
    sqlx::query(
        "INSERT INTO learning_assessment_form_items (form_id, ordinal, candidate_id, outcome_id, format, difficulty, prompt, options_json, rubric_json, points, previously_exposed) \
         VALUES (?, 0, ?, ?, 'single_choice', 1, 'Prompt', '[]', '{}', 1.0, 0);"
    )
    .bind(&form_id)
    .bind(&cand_id)
    .bind(&outcome_id)
    .execute(pool)
    .await?;

    // 6. Form item outcome
    sqlx::query(
        "INSERT INTO learning_assessment_form_item_outcomes (form_id, item_ordinal, outcome_id) \
         VALUES (?, 0, ?);",
    )
    .bind(&form_id)
    .bind(&outcome_id)
    .execute(pool)
    .await?;

    // Submission
    sqlx::query(
        "INSERT INTO learning_assessment_submissions (form_id, program_id, operation_id, payload_hash, score, passed, grade_status, feedback, submitted_at, grader_disagreement_json) \
         VALUES (?, ?, ?, ?, 1.0, 1, 'deterministic', 'Feedback', 1000, '[]');"
    )
    .bind(&form_id)
    .bind(course_id)
    .bind(format!("op_{}", course_id))
    .bind(format!("hash_{}", course_id))
    .execute(pool)
    .await?;

    // 7. Item result
    sqlx::query(
        "INSERT INTO learning_assessment_item_results (form_id, item_ordinal, score, correct, grade_status, feedback) \
         VALUES (?, 0, 1.0, 1, 'deterministic', 'Good');"
    )
    .bind(&form_id)
    .execute(pool)
    .await?;

    // 8. Curriculum revision chain
    let rev1_id = format!("rev1_{}", course_id);
    let rev2_id = format!("rev2_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_curriculum_revisions (id, program_id, revision, predecessor_id, status, reason, snapshot_json, snapshot_sha256, required_lesson_count, created_at, accepted_at) \
         VALUES (?, ?, 1, NULL, 'accepted', 'initial', '{}', ?, 1, 1000, 1000);"
    )
    .bind(&rev1_id)
    .bind(course_id)
    .bind(format!("sha_rev1_{}", course_id))
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO learning_curriculum_revisions (id, program_id, revision, predecessor_id, status, reason, snapshot_json, snapshot_sha256, required_lesson_count, created_at, accepted_at) \
         VALUES (?, ?, 2, ?, 'superseded', 'update', '{}', ?, 1, 1000, 1000);"
    )
    .bind(&rev2_id)
    .bind(course_id)
    .bind(&rev1_id)
    .bind(format!("sha_rev2_{}", course_id))
    .execute(pool)
    .await?;

    // 9. Practice session revision chain
    let sess1_id = format!("sess1_{}", course_id);
    let sess2_id = format!("sess2_{}", course_id);
    sqlx::query(
        "INSERT INTO learning_practice_sessions (id, program_id, lesson_id, lesson_title, lesson_objective, task_prompt, status, mode, source_version_ids_json, rubric_json, created_at, updated_at, task_kind, revises_session_id) \
         VALUES (?, ?, ?, 'Les 1', 'Obj', 'Prompt', 'active', 'practice', '[]', '{}', 1000, 1000, 'independent', NULL);"
    )
    .bind(&sess1_id)
    .bind(course_id)
    .bind(&lesson_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO learning_practice_sessions (id, program_id, lesson_id, lesson_title, lesson_objective, task_prompt, status, mode, source_version_ids_json, rubric_json, created_at, updated_at, task_kind, revises_session_id) \
         VALUES (?, ?, ?, 'Les 1', 'Obj', 'Prompt', 'active', 'practice', '[]', '{}', 1000, 1000, 'independent', ?);"
    )
    .bind(&sess2_id)
    .bind(course_id)
    .bind(&lesson_id)
    .bind(&sess1_id)
    .execute(pool)
    .await?;

    // Children of the candidate and form-item rows.
    sqlx::query(
        "INSERT INTO learning_assessment_answer_keys (candidate_id, key_json, explanation) \
         VALUES (?, '{\"kind\":\"choice\",\"value\":0}', 'Because');",
    )
    .bind(&cand_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO learning_assessment_response_revisions (form_id, item_ordinal, revision, operation_id, payload_hash, selected_indices_json, ordered_values_json, created_at) \
         VALUES (?, 0, 1, ?, 'hash', '[0]', '[]', 1000);",
    )
    .bind(&form_id)
    .bind(format!("resp_{course_id}"))
    .execute(pool)
    .await?;

    Ok(())
}
