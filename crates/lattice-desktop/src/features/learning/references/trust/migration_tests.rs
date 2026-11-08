//! The provenance-and-trust migration applies after every existing one, on
//! a fresh database and on one the previous release filled, keeps every
//! row, holds exactly the domain's vocabularies, and enforces the override
//! rule: Primary only for a source or for a site within a subject class.

use std::str::FromStr;

use lattice_core::domain::provenance::{
    ClaimKind, ExtractedFrom, LabelSource, RelationKind, SiteStanding, Standing, SubjectRelation,
    TrustLevel,
};
use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// The migration under test.
const SOURCE_TRUST: i64 = 20_261_018_120_000;
/// The claim-review migration (L0), which the trust tables must follow.
const CLAIM_REVIEWS: i64 = 20_261_017_150_000;
const _: () = assert!(SOURCE_TRUST > CLAIM_REVIEWS);

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

async fn refused(pool: &SqlitePool, statement: &str) -> bool {
    sqlx::query(statement).execute(pool).await.is_err()
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A course with two sources of one version each: the rows the new tables
/// hang from.
const COURSE: &[&str] = &[
    "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
     VALUES ('p1', 'Tides', 'Read tide tables', 'active', 1, 'none', 30, 'qwen3.5-27b', 1000)",
    "INSERT INTO learning_source_library (id, program_id, kind, origin, requested_url, freshness_policy, active_version_id, pending_version_id, revision, created_at, updated_at) \
     VALUES ('s1', 'p1', 'web', 'https://example.org/tides', 'https://example.org/tides', 'fixed', 'v1', NULL, 0, 1000, 1000)",
    "INSERT INTO learning_source_versions (id, program_id, source_id, version_number, title, publisher, requested_url, resolved_url, full_text, excerpt, content_sha256, word_count, truncated, extraction_version, acquired_at) \
     VALUES ('v1', 'p1', 's1', 1, 'Tide tables', NULL, NULL, NULL, 'High water at noon.', 'High water', 'sha1', 4, 0, 'v1', 1000)",
    "INSERT INTO learning_source_library (id, program_id, kind, origin, requested_url, freshness_policy, active_version_id, pending_version_id, revision, created_at, updated_at) \
     VALUES ('s2', 'p1', 'web', 'https://mirror.example.net/tides', 'https://mirror.example.net/tides', 'fixed', 'v2', NULL, 0, 1000, 1000)",
    "INSERT INTO learning_source_versions (id, program_id, source_id, version_number, title, publisher, requested_url, resolved_url, full_text, excerpt, content_sha256, word_count, truncated, extraction_version, acquired_at) \
     VALUES ('v2', 'p1', 's2', 1, 'Tide tables (mirror)', NULL, NULL, NULL, 'High water at noon.', 'High water', 'sha2', 4, 0, 'v1', 1000)",
];

/// A 512-byte sketch literal: 128 little-endian u32 values.
const SKETCH: &str = "zeroblob(512)";

fn provenance(version: &str, site: &str) -> String {
    format!(
        "INSERT INTO source_provenance (program_id, source_version_id, origin_key, site, origin_json, canonical_url, signals_json, signals_sha256, sketch, links_json, extracted_from, extraction_version, extracted_at) \
         VALUES ('p1', '{version}', 'web:{site}', '{site}', '{{}}', NULL, '{{}}', 'sha', {SKETCH}, '[]', 'fetch', 'page-signals-v1', 1)"
    )
}

fn override_row(scope: &str, columns: &str) -> String {
    format!(
        "INSERT INTO source_trust_overrides (scope, program_id, source_id, registrable_domain, class_key, class_json, level, standing, note, set_at) VALUES ('{scope}', {columns}, NULL, 1)"
    )
}

const RELATION: &str = "INSERT INTO source_relations (program_id, from_version_id, to_version_id, to_url, kind, evidence_json, detected_at) \
     VALUES ('p1', 'v2', 'v1', NULL, 'copy_of', '{}', 1)";

#[tokio::test]
async fn the_migration_applies_after_the_claim_reviews_on_a_fresh_database() {
    let migrator = migrator();
    assert!(migrator.iter().any(|m| m.version == SOURCE_TRUST));

    let pool = memory_pool().await;
    migrator.run(&pool).await.unwrap();
    exec(&pool, COURSE).await;
    exec(&pool, &[&provenance("v1", "example.org"), RELATION]).await;

    // Shape checks.
    for statement in [
        "UPDATE source_provenance SET sketch = zeroblob(511)",
        "UPDATE source_provenance SET signals_json = 'not json'",
        "UPDATE source_relations SET to_url = 'https://example.org/'",
        "UPDATE source_relations SET to_version_id = NULL",
        // An edge to a version the course does not have.
        "INSERT INTO source_relations (program_id, from_version_id, to_version_id, to_url, kind, evidence_json, detected_at) \
         VALUES ('p1', 'v1', 'missing', NULL, 'cites', '{}', 1)",
    ] {
        assert!(refused(&pool, statement).await, "{statement}");
    }
    // One edge per (from, kind, to).
    assert!(refused(&pool, RELATION).await);
}

#[tokio::test]
async fn the_checks_hold_exactly_the_domain_vocabularies() {
    let pool = memory_pool().await;
    migrator().run(&pool).await.unwrap();
    exec(&pool, COURSE).await;
    exec(&pool, &[&provenance("v1", "example.org"), RELATION]).await;
    exec(
        &pool,
        &[
            "INSERT INTO source_standing_assessments VALUES ('sha', 'p', 'm', 'unknown', '{}', 0, 0, 1)",
            "INSERT INTO source_subject_authority VALUES ('p1', 'web:example.org', 'python', 'p', 'm', 'in', 'unknown', '{}', 0, 0, 1)",
            "INSERT INTO source_trust_outcomes VALUES ('o1', 'p1', 'l1', 'c1', 'v1', 'web:example.org', 'unknown', 'general', 0, 0, 'user_confirm', 'p', 1)",
        ],
    )
    .await;
    let columns: Vec<(&str, &str, Vec<&str>)> = vec![
        (
            "source_provenance",
            "extracted_from",
            ExtractedFrom::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_standing_assessments",
            "standing",
            Standing::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_subject_authority",
            "relation",
            SubjectRelation::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_relations",
            "kind",
            RelationKind::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_trust_outcomes",
            "level",
            TrustLevel::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_trust_outcomes",
            "claim_kind",
            ClaimKind::ALL.iter().map(|v| v.as_str()).collect(),
        ),
        (
            "source_trust_outcomes",
            "label_source",
            LabelSource::ALL.iter().map(|v| v.as_str()).collect(),
        ),
    ];
    for (table, column, values) in columns {
        for value in values {
            let accept = format!("UPDATE {table} SET {column} = '{value}'");
            assert!(!refused(&pool, &accept).await, "{accept}");
        }
        let refuse = format!("UPDATE {table} SET {column} = 'invented'");
        assert!(refused(&pool, &refuse).await, "{refuse}");
    }
    for value in TrustLevel::ALL {
        let row = override_row(
            "source",
            &format!("'p1', 's1', NULL, NULL, NULL, '{}', NULL", value.as_str()),
        );
        exec(&pool, &["DELETE FROM source_trust_overrides", &row]).await;
    }
    for value in SiteStanding::ALL {
        let row = override_row(
            "site",
            &format!(
                "NULL, NULL, 'example.org', NULL, NULL, NULL, '{}'",
                value.as_str()
            ),
        );
        exec(&pool, &["DELETE FROM source_trust_overrides", &row]).await;
    }
}

#[tokio::test]
async fn primary_is_allowed_only_for_a_source_or_a_site_within_a_subject_class() {
    let pool = memory_pool().await;
    migrator().run(&pool).await.unwrap();
    exec(&pool, COURSE).await;
    let class = r#"'{"label":"Web platform APIs","key":"web platform apis","kinds":[]}'"#;
    let allowed = [
        override_row("source", "'p1', 's1', NULL, NULL, NULL, 'primary', NULL"),
        override_row(
            "site_subject",
            &format!("NULL, NULL, 'mozilla.org', 'web platform apis', {class}, 'primary', NULL"),
        ),
        override_row(
            "site",
            "NULL, NULL, 'example.org', NULL, NULL, NULL, 'institutional'",
        ),
    ];
    for row in &allowed {
        exec(&pool, &[row]).await;
    }
    let refused_rows = [
        // A site without a class sets standing, never a level.
        override_row(
            "site",
            "NULL, NULL, 'other.org', NULL, NULL, 'primary', NULL",
        ),
        override_row(
            "site",
            "NULL, NULL, 'other.org', NULL, NULL, 'primary', 'institutional'",
        ),
        override_row(
            "site",
            "NULL, NULL, 'other.org', NULL, NULL, NULL, 'primary'",
        ),
        // A class override needs its class, with the key it is stored under.
        override_row(
            "site_subject",
            "NULL, NULL, 'other.org', NULL, NULL, 'primary', NULL",
        ),
        override_row(
            "site_subject",
            &format!("NULL, NULL, 'other.org', 'another key', {class}, 'primary', NULL"),
        ),
        // A source override names its course and source, and a level.
        override_row("source", "NULL, 's1', NULL, NULL, NULL, 'primary', NULL"),
        override_row(
            "source",
            "'p1', 's1', NULL, NULL, NULL, NULL, 'institutional'",
        ),
        override_row(
            "source",
            "'p1', 'missing', NULL, NULL, NULL, 'primary', NULL",
        ),
        // One override per scope key.
        override_row(
            "site",
            "NULL, NULL, 'example.org', NULL, NULL, NULL, 'community'",
        ),
        override_row(
            "unscoped",
            "NULL, NULL, 'example.org', NULL, NULL, 'primary', NULL",
        ),
    ];
    for row in &refused_rows {
        assert!(refused(&pool, row).await, "{row}");
    }
}

#[tokio::test]
async fn the_migration_keeps_a_populated_database_and_its_rows_go_with_their_parents() {
    let pool = memory_pool().await;
    let migrator = migrator();
    {
        let mut conn = pool.acquire().await.unwrap();
        conn.ensure_migrations_table().await.unwrap();
        for migration in migrator
            .iter()
            .filter(|m| m.version < SOURCE_TRUST && !m.migration_type.is_down_migration())
        {
            conn.apply(migration).await.unwrap();
        }
    }
    exec(&pool, COURSE).await;
    let tables = [
        "learning_programs",
        "learning_source_library",
        "learning_source_versions",
    ];
    let mut before = Vec::new();
    for table in tables {
        before.push(count(&pool, table).await);
    }
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

    exec(
        &pool,
        &[
            &provenance("v1", "example.org"),
            &provenance("v2", "example.net"),
            RELATION,
            &override_row("source", "'p1', 's2', NULL, NULL, NULL, 'community', NULL"),
            &override_row("site", "NULL, NULL, 'example.org', NULL, NULL, NULL, 'identified'"),
            "INSERT INTO source_subject_authority VALUES ('p1', 'web:example.org', 'tides', 'p', 'm', 'in', 'owner', '{}', 0, 0, 1)",
            "INSERT INTO source_trust_outcomes VALUES ('o1', 'p1', 'l1', 'c1', 'v1', 'web:example.org', 'primary', 'specification', 1, 1, 'verifier_agree', 'p', 1)",
            "INSERT INTO source_standing_assessments VALUES ('sha', 'p', 'm', 'institutional', '{}', 0, 0, 1)",
            "INSERT INTO claim_profiles VALUES ('in', 'p', 'm', '{}', 0, 0, 1)",
        ],
    )
    .await;
    // A version takes its provenance and its edges; its source takes its
    // override.
    exec(
        &pool,
        &["DELETE FROM learning_source_library WHERE id = 's2'"],
    )
    .await;
    assert_eq!(count(&pool, "source_provenance").await, 1);
    assert_eq!(count(&pool, "source_relations").await, 0);
    assert_eq!(count(&pool, "source_trust_overrides").await, 1);
    // The course takes the rest of its rows; site overrides and the shared
    // judgements stay.
    exec(&pool, &["DELETE FROM learning_programs WHERE id = 'p1'"]).await;
    for table in [
        "source_provenance",
        "source_subject_authority",
        "source_trust_outcomes",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    for table in [
        "source_trust_overrides",
        "source_standing_assessments",
        "claim_profiles",
    ] {
        assert_eq!(count(&pool, table).await, 1, "{table}");
    }
}
