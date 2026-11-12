use super::chunks::store_document;
use crate::features::indexing::engine::chunker::TextChunk;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn setup_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_document_whose_mentions_cannot_be_saved_is_not_stored() {
    let pool = setup_pool().await;
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let text = "Met @[Alice] about [[Plan]].";
    std::fs::write(&path, text).unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse_mention_links BEFORE INSERT ON document_mentions BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let chunk = TextChunk {
        text: text.to_string(),
        start_idx: 0,
        end_idx: text.len(),
        token_count: 6,
        document_path: String::new(),
        chunk_index: 0,
    };

    let result = store_document(
        &db,
        &path,
        "text/markdown",
        vec![chunk],
        vec![vec![0.1, 0.2]],
    )
    .await;

    assert!(result.is_err());
    assert_eq!(count(&pool, "documents").await, 0);
    assert_eq!(count(&pool, "text_chunks").await, 0);
}
