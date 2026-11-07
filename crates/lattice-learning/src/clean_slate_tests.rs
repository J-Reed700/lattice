//! The clean-slate migration (claim classes design §3.9), over a database
//! the old preparation pipeline filled: unfinished preparation ends with its
//! checkpoints, the old pipeline's unpublished state is gone, and every
//! program, outline, source version and ready lesson is as it was.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// The migration under test.
const CLEAN_SLATE: i64 = 20_261_102_100_000;

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

async fn exec(pool: &SqlitePool, statements: &[&str]) {
    for statement in statements {
        sqlx::query(statement)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

async fn scalar(pool: &SqlitePool, sql: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .unwrap_or_default()
}

async fn count(pool: &SqlitePool, from: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {from}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// What must not change: the course, its outline and its lessons, the source
/// library and its versions, as one string per table.
async fn kept(pool: &SqlitePool) -> Vec<String> {
    let mut tables = Vec::new();
    for sql in [
        "SELECT group_concat(id || title || goal || status || revision, ';') FROM (SELECT * FROM learning_programs ORDER BY id)",
        "SELECT group_concat(id || title || summary || outcomes_json, ';') FROM (SELECT * FROM learning_modules ORDER BY id)",
        "SELECT group_concat(id || title || objective || preparation || completed, ';') FROM (SELECT * FROM learning_lessons ORDER BY id)",
        "SELECT group_concat(lesson_id || ordinal || title || body, ';') FROM (SELECT * FROM learning_blocks ORDER BY lesson_id, ordinal)",
        "SELECT group_concat(id || active_version_id || origin, ';') FROM (SELECT * FROM learning_source_library ORDER BY id)",
        "SELECT group_concat(id || source_id || full_text || content_sha256, ';') FROM (SELECT * FROM learning_source_versions ORDER BY id)",
        "SELECT group_concat(lesson_id || content_sha256, ';') FROM (SELECT * FROM learning_lesson_verifications ORDER BY lesson_id)",
        "SELECT group_concat(id || claim, ';') FROM (SELECT * FROM learning_checker_examples ORDER BY id)",
    ] {
        tables.push(scalar(pool, sql).await);
    }
    tables
}

const COURSE: &[&str] = &[
    "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
     VALUES ('p1', 'Tides', 'Read tide tables', 'active', 1, 'none', 30, 'qwen', 1000)",
    "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json) \
     VALUES ('m1', 'p1', 0, 'Reading tables', 'How to read them', '[]')",
    // One lesson not yet prepared, one held for review, one ready.
    "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed) \
     VALUES ('l-out', 'p1', 'm1', 0, 'Outline only', 'Know the basics', 20, 'outline', 0)",
    "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed) \
     VALUES ('l-held', 'p1', 'm1', 1, 'Held for review', 'Read a table', 20, 'outline', 0)",
    "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed) \
     VALUES ('l-ready', 'p1', 'm1', 2, 'Ready', 'Find high water', 20, 'ready', 0)",
    "INSERT INTO learning_blocks (lesson_id, ordinal, kind, title, body, source_ids_json) \
     VALUES ('l-ready', 0, 'explanation', 'High water', 'High water is at noon.', '[\"v1\"]')",
    "INSERT INTO learning_source_library (id, program_id, kind, origin, requested_url, freshness_policy, active_version_id, pending_version_id, revision, created_at, updated_at) \
     VALUES ('s1', 'p1', 'web', 'https://example.org/tides', 'https://example.org/tides', 'fixed', 'v1', NULL, 0, 1000, 1000)",
    "INSERT INTO learning_source_versions (id, program_id, source_id, version_number, title, publisher, requested_url, resolved_url, full_text, excerpt, content_sha256, word_count, truncated, extraction_version, acquired_at) \
     VALUES ('v1', 'p1', 's1', 1, 'Tide tables', NULL, NULL, NULL, 'High water at noon.', 'High water', 'sha1', 4, 0, 'v1', 1000)",
    "INSERT INTO learning_lesson_verifications (lesson_id, program_id, policy, content_sha256, report_json, checked_at) \
     VALUES ('l-ready', 'p1', 'lesson-evidence-v21', 'sha-ready', '{}', 1000)",
];

/// Jobs as the old pipeline left them: running, waiting to retry, completed
/// with a lesson held, failed, and another feature's job.
const JOBS: &[&str] = &[
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_current, progress_total, progress_message, created_at, started_at) \
     VALUES ('j-running', 'learning.lesson_preparation', 'p1', 'op-1', 'h', 'running', '{}', 0, 2, 'Checking claims', 1000, 1001)",
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_current, progress_total, progress_message, retry_not_before, created_at) \
     VALUES ('j-pending', 'learning.lesson_preparation', 'p1', 'op-2', 'h', 'pending', '{}', 0, 1, 'Waiting to retry', 9999999999999, 1000)",
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_current, progress_total, progress_message, result_ref, created_at, finished_at, staged_result_json) \
     VALUES ('j-done', 'learning.lesson_preparation', 'p1', 'op-3', 'h', 'completed', '{}', 2, 2, 'Needs your review', 'ref', 1000, 2000, \
     '{\"drafts\":{\"l-held\":{\"input_hash\":\"a\",\"candidate\":\"{}\",\"claim_inventory\":\"[]\"},\"l-ready\":{\"input_hash\":\"b\",\"candidate\":\"{}\"}}}')",
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_current, progress_total, progress_message, error_code, error_message, created_at, finished_at, staged_result_json) \
     VALUES ('j-failed', 'learning.lesson_preparation', 'p1', 'op-4', 'h', 'failed', '{}', 0, 1, 'Failed', 'x', 'y', 1000, 2000, \
     '{\"drafts\":{\"l-out\":{\"input_hash\":\"c\",\"candidate\":\"{}\",\"pending_repair\":\"{}\"}}}')",
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_current, progress_total, progress_message, created_at, started_at) \
     VALUES ('j-other', 'indexing.import', NULL, 'op-5', 'h', 'running', '{}', 0, 1, 'Importing', 1000, 1001)",
    "INSERT INTO learning_jobs (job_id, program_id) VALUES ('j-running', 'p1')",
    "INSERT INTO job_events (job_id, ordinal, status, progress_current, message, created_at) VALUES ('j-running', 0, 'pending', 0, 'Queued', 1000)",
    // Lesson checkpoints are saved as `<lesson>/<key>`.
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-running', 'l-out/prepared-v1', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-running', 'l-ready/claim-v1:abc', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-running', 'numbered-sources', '[]', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-pending', 'l-out/inventory-section-v1:x', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-done', 'l-held/claim-walk-v1:y', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-done', 'l-ready/prepared-v1', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-failed', 'l-out/prepared-v1', '{}', 1)",
    "INSERT INTO job_checkpoints (job_id, key, data_json, updated_at) VALUES ('j-other', 'index/progress', '{}', 1)",
];

/// The review rows, claims, events, audits and teaching findings of a lesson.
fn review_of(lesson: &str) -> Vec<String> {
    let claim = format!("claim-{lesson}");
    vec![
        format!("INSERT INTO learning_lesson_reviews (lesson_id, program_id, job_id, state, revision, draft_sha256, checker_model, opened_at, updated_at) VALUES ('{lesson}', 'p1', 'j-done', 'needs_review', 0, 'sha', 'qwen', 1000, 1000)"),
        format!("INSERT INTO learning_lesson_claims (claim_id, lesson_id, unit_index, unit_kind, field, quote, statement, state_json, evidence_json, updated_at) VALUES ('{claim}', '{lesson}', 0, 'block', 'body', 'High water is at noon.', 'High water is at noon.', '{{}}', '[]', 1000)"),
        format!("INSERT INTO learning_lesson_claim_events (claim_id, ordinal, actor, kind, payload_json, job_id, at) VALUES ('{claim}', 0, 'machine', 'extracted', '{{}}', 'j-done', 1000)"),
        format!("INSERT INTO learning_lesson_audits (lesson_id, draft_sha256, verifier_model, policy, state, batches_json, started_at) VALUES ('{lesson}', 'sha', 'verifier', 'p', 'completed', '[]', 1000)"),
        format!("INSERT INTO learning_lesson_teaching_findings (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, repairs, state, found_at) VALUES ('{lesson}', 'teaching:section-0', 'teaching', 'section', 0, 'High water', 'sha', 'A note', 1, 'waiting', 1000)"),
    ]
}

async fn rows_of(pool: &SqlitePool, lesson: &str) -> Vec<i64> {
    let mut counts = Vec::new();
    for table in [
        "learning_lesson_reviews",
        "learning_lesson_claims",
        "learning_lesson_audits",
        "learning_lesson_teaching_findings",
    ] {
        counts.push(count(pool, &format!("{table} WHERE lesson_id = '{lesson}'")).await);
    }
    counts.push(
        count(
            pool,
            &format!("learning_lesson_claim_events WHERE claim_id = 'claim-{lesson}'"),
        )
        .await,
    );
    counts
}

#[tokio::test]
async fn the_clean_slate_ends_unfinished_preparation_and_drops_the_old_pipelines_unpublished_state()
{
    let pool = memory_pool().await;
    let migrator = migrator();
    assert!(migrator.iter().any(|m| m.version == CLEAN_SLATE));
    {
        let mut conn = pool.acquire().await.unwrap();
        conn.ensure_migrations_table().await.unwrap();
        for migration in migrator
            .iter()
            .filter(|m| m.version < CLEAN_SLATE && !m.migration_type.is_down_migration())
        {
            conn.apply(migration).await.unwrap();
        }
    }
    exec(&pool, COURSE).await;
    exec(&pool, JOBS).await;
    for lesson in ["l-held", "l-ready"] {
        let rows = review_of(lesson);
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        exec(&pool, &rows).await;
    }
    exec(
        &pool,
        &["INSERT INTO learning_checker_examples (id, program_id, lesson_id, claim_id, checker_model, context_window, policy, claim, passages_json, local_verdict, label, label_source, at) \
           VALUES ('ex1', 'p1', 'l-held', 'claim-l-held', 'qwen', 4096, 'p', 'A claim', '[]', 'unsupported', 'true', 'user_confirm', 1)"],
    )
    .await;
    let before = kept(&pool).await;
    let ready_rows = rows_of(&pool, "l-ready").await;
    assert_eq!(ready_rows, [1, 1, 1, 1, 1]);

    migrator.run(&pool).await.unwrap();

    // Unfinished preparation ends, says why, and has no checkpoint left.
    for job in ["j-running", "j-pending"] {
        let state = scalar(
            &pool,
            &format!("SELECT status || '|' || progress_message || '|' || (finished_at IS NOT NULL) || '|' || coalesce(retry_not_before, 'none') FROM jobs WHERE id = '{job}'"),
        )
        .await;
        assert_eq!(
            state, "cancelled|Preparation starts again under the new pipeline|1|none",
            "{job}"
        );
        assert_eq!(
            count(&pool, &format!("job_checkpoints WHERE job_id = '{job}'")).await,
            0,
            "{job}"
        );
    }
    assert_eq!(
        scalar(
            &pool,
            "SELECT status || ':' || message FROM job_events WHERE job_id = 'j-running' ORDER BY ordinal DESC LIMIT 1"
        )
        .await,
        "cancelled:Preparation starts again under the new pipeline"
    );
    // A job that had finished keeps its status; its checkpoints and staged
    // drafts of lessons that are not ready go, and a ready lesson's stay.
    assert_eq!(
        scalar(&pool, "SELECT status FROM jobs WHERE id = 'j-done'").await,
        "completed"
    );
    assert_eq!(
        scalar(&pool, "SELECT status FROM jobs WHERE id = 'j-failed'").await,
        "failed"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT group_concat(job_id || ':' || key, ';') FROM (SELECT * FROM job_checkpoints WHERE job_id <> 'j-other' ORDER BY job_id, key)"
        )
        .await,
        "j-done:l-ready/prepared-v1"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT group_concat(key, ',') FROM json_each((SELECT staged_result_json FROM jobs WHERE id = 'j-done'), '$.drafts')"
        )
        .await,
        "l-ready"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT json_type(staged_result_json, '$.drafts') FROM jobs WHERE id = 'j-failed'"
        )
        .await,
        "object"
    );
    assert_eq!(
        count(
            &pool,
            "json_each((SELECT staged_result_json FROM jobs WHERE id = 'j-failed'), '$.drafts')"
        )
        .await,
        0
    );
    // Another feature's job is not touched.
    assert_eq!(
        scalar(
            &pool,
            "SELECT status || ':' || (SELECT count(*) FROM job_checkpoints WHERE job_id = 'j-other') FROM jobs WHERE id = 'j-other'"
        )
        .await,
        "running:1"
    );

    // The old pipeline's state for a lesson that is not ready is gone: the
    // review, its claims and their events, the audits and the findings.
    assert_eq!(rows_of(&pool, "l-held").await, [0, 0, 0, 0, 0]);
    assert_eq!(rows_of(&pool, "l-ready").await, ready_rows);
    // Everything else is as it was.
    assert_eq!(kept(&pool).await, before);
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
}
