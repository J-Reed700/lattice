//! The claim-review migration applies after every existing migration, on a
//! fresh database and on one the previous release filled, keeps every row,
//! and gives the new tables the keys, checks and cascades the design needs.

use std::str::FromStr;

use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// The migration under test.
const CLAIM_REVIEWS: i64 = 20_261_017_150_000;

fn migrator() -> Migrator {
    sqlx::migrate!("./migrations")
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

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A course with one lesson, a source with one version, and a lesson job:
/// the rows the new tables hang from.
const COURSE: &[&str] = &[
    "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
     VALUES ('p1', 'Thermodynamics', 'Understand phase changes', 'active', 1, 'none', 30, 'qwen3.5-27b', 1000)",
    "INSERT INTO learning_modules (id, program_id, ordinal, title, summary, outcomes_json, prerequisite_ids_json, project_json) \
     VALUES ('m1', 'p1', 1, 'Phases', 'Summary', '[]', '[]', '{}')",
    "INSERT INTO learning_lessons (id, program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, completed, curriculum_state) \
     VALUES ('l1', 'p1', 'm1', 1, 'Boiling', 'Explain boiling', 20, 'outline', 0, 'ready')",
    "INSERT INTO learning_source_library (id, program_id, kind, origin, requested_url, freshness_policy, active_version_id, pending_version_id, revision, created_at, updated_at) \
     VALUES ('s1', 'p1', 'web', 'https://example.org/steam', 'https://example.org/steam', 'fixed', 'v1', NULL, 0, 1000, 1000)",
    "INSERT INTO learning_source_versions (id, program_id, source_id, version_number, title, publisher, requested_url, resolved_url, full_text, excerpt, content_sha256, word_count, truncated, extraction_version, acquired_at) \
     VALUES ('v1', 'p1', 's1', 1, 'Steam tables', NULL, NULL, NULL, 'Water boils at 99.97 C.', 'Water boils', 'sha', 5, 0, 'v1', 1000)",
    "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) \
     VALUES ('j1', 'lesson_preparation', 'l1', 'op-1', 'hash', 'pending', '{}', 1, 'Queued', 1000)",
    "INSERT INTO learning_jobs (job_id, program_id) VALUES ('j1', 'p1')",
];

/// One row in each new table, chained by their keys.
const LEDGER: &[&str] = &[
    "INSERT INTO learning_lesson_reviews (lesson_id, program_id, job_id, state, draft_sha256, checker_model, stop_reason_json, opened_at, updated_at) \
     VALUES ('l1', 'p1', 'j1', 'needs_review', 'draft', 'qwen3.5-27b', NULL, 2000, 2000)",
    "INSERT INTO learning_lesson_claims (claim_id, lesson_id, unit_index, unit_kind, field, quote, span_start, span_end, statement, state_json, evidence_fingerprint, verdict_kind, reason, evidence_json, updated_at) \
     VALUES ('c1', 'l1', 0, 'block', 'body', 'Water boils at 100 C.', 0, 21, 'Water boils at 100 C.', '{\"state\":\"unchecked\"}', NULL, NULL, NULL, '[]', 2000)",
    "INSERT INTO learning_lesson_claim_events (claim_id, ordinal, actor, kind, payload_json, job_id, at) \
     VALUES ('c1', 0, 'machine', 'extracted', '{}', 'j1', 2000)",
    "INSERT INTO learning_lesson_audits (lesson_id, draft_sha256, verifier_model, policy, state, batches_json, started_at) \
     VALUES ('l1', 'draft', 'claude-verifier', 'audit-v1', 'running', '[]', 2000)",
    "INSERT INTO learning_checker_examples (id, program_id, lesson_id, claim_id, checker_model, context_window, policy, claim, passages_json, local_verdict, label, label_source, at) \
     VALUES ('e1', 'p1', 'l1', 'c1', 'qwen3.5-27b', 32768, 'strict-v1', 'Water boils at 100 C.', '[]', 'contradicted', 'true', 'user_confirm', 2000)",
];

#[tokio::test]
async fn the_migration_applies_after_every_existing_one_on_a_fresh_database() {
    let migrator = migrator();
    assert!(migrator.iter().any(|m| m.version == CLAIM_REVIEWS));

    let pool = memory_pool().await;
    migrator.run(&pool).await.unwrap();
    exec(&pool, COURSE).await;
    exec(&pool, LEDGER).await;

    // The checks hold the vocabulary the domain writes.
    for refused in [
        "UPDATE learning_lesson_reviews SET state = 'failed'",
        "UPDATE learning_lesson_reviews SET stop_reason_json = 'not json'",
        "UPDATE learning_lesson_claims SET unit_kind = 'answer'",
        "UPDATE learning_lesson_claims SET state_json = 'not json'",
        "UPDATE learning_lesson_claim_events SET actor = 'system'",
        "UPDATE learning_source_library SET added_by = 'someone'",
        "UPDATE learning_programs SET verifier_audit = 'maybe'",
        "UPDATE learning_programs SET verifier_web_search = 2",
        "UPDATE learning_lesson_audits SET state = 'failed'",
        "UPDATE learning_checker_examples SET label = 'maybe'",
        "UPDATE learning_checker_examples SET label_source = 'guess'",
        "INSERT INTO learning_lesson_claim_events (claim_id, ordinal, actor, kind, payload_json, at) \
         VALUES ('c1', 0, 'user', 'undone', '{}', 3000)",
        "INSERT INTO learning_lesson_claims (claim_id, lesson_id, unit_index, unit_kind, field, quote, statement, state_json, evidence_json, updated_at) \
         VALUES ('c2', 'no-review', 0, 'block', 'body', 'q', 's', '{}', '[]', 1)",
    ] {
        assert!(
            sqlx::query(refused).execute(&pool).await.is_err(),
            "{refused}"
        );
    }
    exec(
        &pool,
        &["UPDATE learning_source_library SET added_by = 'verifier'"],
    )
    .await;

    // A lesson takes its review, claims, events and audits with it; the
    // checker's examples stay until the course goes.
    exec(&pool, &["DELETE FROM learning_lessons WHERE id = 'l1'"]).await;
    for table in [
        "learning_lesson_reviews",
        "learning_lesson_claims",
        "learning_lesson_claim_events",
        "learning_lesson_audits",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    assert_eq!(count(&pool, "learning_checker_examples").await, 1);
    exec(&pool, &["DELETE FROM learning_programs WHERE id = 'p1'"]).await;
    assert_eq!(count(&pool, "learning_checker_examples").await, 0);
}

#[tokio::test]
async fn the_migration_keeps_a_populated_database_and_adds_the_defaults() {
    let pool = memory_pool().await;
    let migrator = migrator();
    {
        let mut conn = pool.acquire().await.unwrap();
        conn.ensure_migrations_table().await.unwrap();
        for migration in migrator
            .iter()
            .filter(|m| m.version < CLAIM_REVIEWS && !m.migration_type.is_down_migration())
        {
            conn.apply(migration).await.unwrap();
        }
    }
    exec(&pool, COURSE).await;
    let tables = [
        "learning_programs",
        "learning_lessons",
        "learning_source_library",
        "learning_source_versions",
        "jobs",
        "learning_jobs",
    ];
    let mut before = Vec::new();
    for table in tables {
        before.push(count(&pool, table).await);
    }
    let content_revision = "SELECT content_revision FROM learning_programs WHERE id = 'p1'";
    let revision_before: i64 = sqlx::query_scalar(content_revision)
        .fetch_one(&pool)
        .await
        .unwrap();

    migrator.run(&pool).await.unwrap();

    let mut after = Vec::new();
    for table in tables {
        after.push(count(&pool, table).await);
    }
    assert_eq!(before, after, "no row is dropped or added");
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());

    let defaults: (String, i64, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT verifier_audit, verifier_web_search, verifier_consent_at, verifier_consent_provider \
         FROM learning_programs WHERE id = 'p1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(defaults, ("off".into(), 0, None, None));
    let added_by: String =
        sqlx::query_scalar("SELECT added_by FROM learning_source_library WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(added_by, "research");
    let snapshot: Option<String> =
        sqlx::query_scalar("SELECT snapshot_hash FROM learning_source_versions WHERE id = 'v1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snapshot, None);

    // The opt-in is not course content: switching it leaves the course's
    // content revision alone.
    exec(
        &pool,
        &[
            "UPDATE learning_programs SET verifier_audit = 'on', verifier_web_search = 1, \
           verifier_consent_at = 3000, verifier_consent_provider = 'anthropic' WHERE id = 'p1'",
        ],
    )
    .await;
    let revision_after: i64 = sqlx::query_scalar(content_revision)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(revision_before, revision_after);
    exec(&pool, LEDGER).await;
}
