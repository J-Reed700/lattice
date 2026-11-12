//! The migration that marks which outline sources the outline's research
//! found (claim classes design §3.5(4)), over a database from before it: a
//! page an outline saved from the web stays under the admission sweep, as
//! every outline source was, and a document is left alone.
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::shared::persistence::Database;
use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// The migration under test.
const FOUND_BY_SEARCH: i64 = 20_261_102_110_000;

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

/// An outline source of the course with its one version, as a database from
/// before the migration holds it.
async fn outline_source(
    pool: &SqlitePool,
    (id, kind, url): (&str, &str, Option<&str>),
    (title, text): (&str, &str),
) {
    let policy = if url.is_some() { "manual" } else { "fixed" };
    sqlx::query("INSERT INTO learning_source_library (id, program_id, kind, origin, requested_url, freshness_policy, active_version_id, pending_version_id, revision, created_at, updated_at, added_by) VALUES (?, 'p1', ?, ?, ?, ?, ?, NULL, 0, 1000, 1000, 'outline')")
        .bind(id)
        .bind(kind)
        .bind(url.unwrap_or(id))
        .bind(url)
        .bind(policy)
        .bind(format!("v-{id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_source_versions (id, program_id, source_id, version_number, title, publisher, requested_url, resolved_url, full_text, excerpt, content_sha256, word_count, truncated, extraction_version, acquired_at) VALUES (?, 'p1', ?, 1, ?, NULL, ?, ?, ?, ?, ?, ?, 0, 'v1', 1000)")
        .bind(format!("v-{id}"))
        .bind(id)
        .bind(title)
        .bind(url)
        .bind(url)
        .bind(text)
        .bind(text.chars().take(200).collect::<String>())
        .bind(format!("sha-{id}"))
        .bind(text.split_whitespace().count() as i64)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_outline_page_saved_from_the_web_before_the_mark_stays_under_the_sweep_and_a_document_does_not(
) {
    let pool = memory_pool().await;
    let db = Database::new(pool.clone());
    let migrator = migrator();
    assert!(migrator.iter().any(|m| m.version == FOUND_BY_SEARCH));
    {
        let mut conn = pool.acquire().await.unwrap();
        conn.ensure_migrations_table("_sqlx_migrations")
            .await
            .unwrap();
        for migration in migrator
            .iter()
            .filter(|m| m.version < FOUND_BY_SEARCH && !m.migration_type.is_down_migration())
        {
            conn.apply("_sqlx_migrations", migration).await.unwrap();
        }
    }
    sqlx::query("INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) VALUES ('p1', 'Retrieval', 'Understand retrieval-augmented generation', 'active', 1, 'none', 30, 'qwen', 1000)")
        .execute(&pool)
        .await
        .unwrap();
    // The abstract of a survey, saved by the outline's research, and a short
    // document the user chose.
    let survey = "A survey of retrieval augmented generation methods";
    outline_source(
        &pool,
        (
            "s-web",
            "web",
            Some("https://www.sciencedirect.com/science/article/pii/S0000000000000000"),
        ),
        (survey, &format!("Abstract. {}", "word ".repeat(293))),
    )
    .await;
    outline_source(
        &pool,
        ("s-doc", "document", None),
        (
            "My notes on retrieval",
            "Short notes on what retrieval adds to a model.",
        ),
    )
    .await;

    migrator.run(&pool).await.unwrap();

    let marks: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, found_by_search FROM learning_source_library ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(marks, [("s-doc".to_owned(), 0), ("s-web".to_owned(), 1)]);
    let refused = LearningSourceLibraryRepository::new(db.clone())
        .sweep_admission("p1")
        .await
        .unwrap();
    let titles: Vec<&str> = refused.iter().map(|page| page.title.as_str()).collect();
    assert_eq!(titles, [survey]);
    assert!(
        refused[0].reason.starts_with("abstract only"),
        "{}",
        refused[0].reason
    );
    let document: Option<String> = sqlx::query_scalar(
        "SELECT not_a_source FROM learning_source_versions WHERE id = 'v-s-doc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(document, None);
}
