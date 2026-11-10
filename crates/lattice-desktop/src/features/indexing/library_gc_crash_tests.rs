use super::*;
use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
use crate::infrastructure::storage::ContentAddressedStorage;
use std::path::Path;
use tempfile::TempDir;

fn open_gate() -> Arc<SweepGate> {
    let gate = SweepGate::default();
    gate.open();
    Arc::new(gate)
}

async fn import(storage: &ContentAddressedStorage, dir: &Path, content: &str) -> (String, bool) {
    let source = dir.join("source.txt");
    tokio::fs::write(&source, content).await.unwrap();
    let blob = storage.import_file(&source).await.unwrap();
    (blob.hash.clone(), blob.path.exists())
}

async fn refuse_forgetting(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "CREATE TRIGGER refuse_forget BEFORE DELETE ON file_intents
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_release_that_cannot_forget_its_record_leaves_it_for_the_next_sweep() {
    let temp = TempDir::new().unwrap();
    let storage = Arc::new(ContentAddressedStorage::with_root(temp.path().join("lib")));
    let documents = Arc::new(MockDocumentRepository::new());
    let (pool, intents) = test_support::file_intents().await;
    let gc = LibraryGc::new(
        Arc::clone(&storage) as Arc<dyn ContentAddressedStoragePort>,
        Arc::clone(&documents) as Arc<dyn DocumentRepositoryPort>,
        open_gate(),
    )
    .with_intents(intents.clone());
    let (hash, existed) = import(&storage, temp.path(), "released").await;
    assert!(existed);
    sqlx::query(
        "INSERT INTO file_intents (store, kind, name, run_id)
         VALUES ('library', 'release', ?, 'an earlier run')",
    )
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();
    refuse_forgetting(&pool).await;

    let removal = gc.release(&hash).await.unwrap();

    assert!(removal.was_removed(), "the blob goes first");
    assert_eq!(
        intents.pending(FileStore::Library).await.unwrap().len(),
        1,
        "the record outlives the blob it named"
    );

    sqlx::query("DROP TRIGGER refuse_forget")
        .execute(&pool)
        .await
        .unwrap();
    gc.sweep().await.unwrap();
    assert!(
        intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty(),
        "the next sweep clears a record whose blob is already gone"
    );
}
