//! Each folder change lands as one unit of work, and what it checks is
//! checked once the write lock is the writer's own.
use super::{folders, repository};
use crate::features::conversation::repository::ConversationRepository;
use lattice_core::shared::persistence::{Database, Transactions};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::time::Duration;

const ROOT: &str = "/w/a";

async fn database() -> (SqlitePool, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::tests::sqlite::file_options(
            directory.path().join("explorer.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_spaces (id, name) VALUES ('space_music', 'Music')")
        .execute(&pool)
        .await
        .unwrap();
    repository::record_folder_opened(&pool, ROOT, "a", "2026-10-01T10:00:00.000Z")
        .await
        .unwrap();
    (pool, directory)
}

async fn thread(pool: &SqlitePool) -> String {
    ConversationRepository::new(pool.clone())
        .create_conversation("Voices", None)
        .await
        .unwrap()
        .id
        .to_string()
}

async fn root_of(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT explorer_root FROM conversations WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Runs `change` in another writer that commits after the call under test
/// has started waiting for the write lock.
async fn after_another_writer<F, Fut, T>(pool: &SqlitePool, statement: &str, call: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query(statement).execute(&mut *tx).await.unwrap();
    let other = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        tx.commit().await.unwrap();
    });
    let result = call().await;
    other.await.unwrap();
    result
}

#[tokio::test]
async fn a_thread_that_cannot_be_filed_in_its_folders_space_is_not_bound() {
    let (pool, _directory) = database().await;
    folders::set_settings(&Database::from(pool.clone()), ROOT, "", "space_music")
        .await
        .unwrap();
    let id = thread(&pool).await;
    sqlx::query(
        "CREATE TRIGGER refuse_move BEFORE UPDATE OF space_id ON conversations \
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = folders::bind_thread(&Database::from(pool.clone()), &id, Some(ROOT)).await;

    assert!(result.is_err());
    assert_eq!(root_of(&pool, &id).await, None);
}

#[tokio::test]
async fn a_space_archived_meanwhile_is_not_given_to_a_folder() {
    let (pool, _directory) = database().await;

    let result = after_another_writer(
        &pool,
        "UPDATE conversation_spaces SET is_archived = 1 WHERE id = 'space_music'",
        || folders::set_settings(&Database::from(pool.clone()), ROOT, "", "space_music"),
    )
    .await;

    assert!(result.is_err());
    let space: Option<String> =
        sqlx::query_scalar("SELECT space_id FROM explorer_folders WHERE root = ?")
            .bind(ROOT)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(space, None);
}

#[tokio::test]
async fn a_thread_unbound_meanwhile_is_not_remembered_as_the_folders() {
    let (pool, _directory) = database().await;
    let id = thread(&pool).await;
    repository::set_conversation_root(&pool, &id, Some(ROOT))
        .await
        .unwrap();
    let unbind = format!("UPDATE conversations SET explorer_root = NULL WHERE id = '{id}'");

    let result = after_another_writer(&pool, &unbind, || {
        folders::remember_thread(&Database::from(pool.clone()), ROOT, &id)
    })
    .await;

    assert!(result.is_err());
    let last: Option<String> =
        sqlx::query_scalar("SELECT last_thread_id FROM explorer_folders WHERE root = ?")
            .bind(ROOT)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last, None);
}
