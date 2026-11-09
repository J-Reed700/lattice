use super::schema::rebuild_fts5_index;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn a_rebuild_that_cannot_refill_the_index_leaves_it_as_it_was() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE VIRTUAL TABLE documents_fts USING fts5(document_id UNINDEXED, content);
         INSERT INTO documents_fts(document_id, content) VALUES ('doc', 'kept');",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert!(rebuild_fts5_index(&pool).await.is_err());

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}
