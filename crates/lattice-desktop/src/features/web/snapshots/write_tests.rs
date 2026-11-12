//! The library conversation is filed into a space that still exists once the
//! write lock is the writer's own.
use super::repository::SnapshotRepository;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::test]
async fn the_library_conversation_follows_a_space_replaced_meanwhile() {
    let directory = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::tests::sqlite::file_options(
            directory.path().join("snapshots.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let db = Database::new(pool.clone());
    sqlx::query("INSERT INTO conversation_spaces (id, name) VALUES ('space_other', 'Other')")
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("DELETE FROM conversation_spaces WHERE id = 'space_general'")
        .execute(&mut *tx)
        .await
        .unwrap();
    let other = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        tx.commit().await.unwrap();
    });

    let id = SnapshotRepository::new(db)
        .library_conversation()
        .await
        .unwrap();

    other.await.unwrap();
    let space: String = sqlx::query_scalar("SELECT space_id FROM conversations WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(space, "space_other");
}
