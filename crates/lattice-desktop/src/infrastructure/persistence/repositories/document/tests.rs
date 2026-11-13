use super::implementation::SqliteDocumentRepository as DocumentRepository;
use lattice_core::shared::error::AppError;

use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::application::ports::RepositoryPort;
use lattice_core::shared::persistence::{Database, Page};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::time::Duration;

async fn repository() -> (DocumentRepository, SqlitePool) {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    (DocumentRepository::new(db), pool)
}

/// Insert a row directly: these queries read one column and must not
/// depend on chunks existing.
async fn insert(pool: &SqlitePool, id: &str, path: &str, checksum: &str) {
    sqlx::query(
        "INSERT INTO documents \
             (id, file_path, file_name, file_type, mime_type, size_bytes, \
              modified_at, indexed_at, checksum, status) \
             VALUES (?1, ?2, ?3, 'txt', 'text/plain', 4, '2026-09-25T00:00:00Z', \
                     '2026-09-25T00:00:00Z', ?4, 'indexed')",
    )
    .bind(id)
    .bind(path)
    .bind(id)
    .bind(checksum)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn count_by_checksum_counts_every_row_that_shares_the_content() {
    let (repo, pool) = repository().await;
    let shared = "a".repeat(64);
    let lonely = "b".repeat(64);

    insert(&pool, "doc-1", "/lib/a/one.txt", &shared).await;
    insert(&pool, "doc-2", "/lib/a/two.txt", &shared).await;
    insert(&pool, "doc-3", "/lib/b/three.txt", &lonely).await;

    assert_eq!(repo.count_by_checksum(&shared).await.unwrap(), 2);
    assert_eq!(repo.count_by_checksum(&lonely).await.unwrap(), 1);
    assert_eq!(repo.count_by_checksum(&"c".repeat(64)).await.unwrap(), 0);
}

#[tokio::test]
async fn find_metadata_by_ids_reads_only_the_asked_rows() {
    let (repo, pool) = repository().await;
    insert(&pool, "doc-1", "/lib/a/one.txt", &"a".repeat(64)).await;
    insert(&pool, "doc-2", "/lib/b/two.txt", &"b".repeat(64)).await;
    insert(&pool, "doc-3", "/lib/c/three.txt", &"c".repeat(64)).await;

    let mut found: Vec<String> = repo
        .find_metadata_by_ids(&["doc-3".into(), "doc-1".into(), "missing".into()])
        .await
        .unwrap()
        .into_iter()
        .map(|doc| doc.id().as_str().to_owned())
        .collect();
    found.sort();

    assert_eq!(found, vec!["doc-1", "doc-3"]);
    assert!(repo.find_metadata_by_ids(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_library_listing_without_a_limit_returns_every_row_but_a_chats_attachments() {
    let (repo, pool) = repository().await;
    for n in 0..5 {
        insert(
            &pool,
            &format!("doc-{n}"),
            &format!("/lib/{n}.txt"),
            &n.to_string().repeat(64),
        )
        .await;
    }
    insert(&pool, "attached", "/chat/a.txt", &"f".repeat(64)).await;
    sqlx::query("UPDATE documents SET owner_conversation_id = 'chat-1' WHERE id = 'attached'")
        .execute(&pool)
        .await
        .unwrap();

    let every = repo.find_all_paginated(Page::default()).await.unwrap();
    let first_two = repo
        .find_all_paginated(Page::new(Some(2), None).unwrap())
        .await
        .unwrap();
    let last_one = repo
        .find_all_paginated(Page::new(Some(2), Some(4)).unwrap())
        .await
        .unwrap();

    assert_eq!(every.len(), 5);
    assert!(every.iter().all(|doc| doc.id().as_str() != "attached"));
    assert_eq!(first_two.len(), 2);
    assert_eq!(last_one.len(), 1);
}

#[tokio::test]
async fn count_by_checksum_of_an_empty_table_is_zero() {
    let (repo, _) = repository().await;

    assert_eq!(repo.count_by_checksum(&"a".repeat(64)).await.unwrap(), 0);
}

#[tokio::test]
async fn list_checksums_is_the_distinct_referenced_set() {
    let (repo, pool) = repository().await;
    let shared = "a".repeat(64);
    let lonely = "b".repeat(64);

    insert(&pool, "doc-1", "/lib/a/one.txt", &shared).await;
    insert(&pool, "doc-2", "/lib/a/two.txt", &shared).await;
    insert(&pool, "doc-3", "/lib/b/three.txt", &lonely).await;
    // A row with no checksum must not put an empty string in the set:
    // the sweep would then treat "" as a referenced blob name.
    insert(&pool, "doc-4", "/lib/c/four.txt", "").await;

    let mut checksums = repo.list_checksums().await.unwrap();
    checksums.sort();

    assert_eq!(checksums, vec![shared, lonely]);
}

#[tokio::test]
async fn list_checksums_of_an_empty_table_is_empty() {
    let (repo, _) = repository().await;

    assert!(repo.list_checksums().await.unwrap().is_empty());
}

#[tokio::test]
async fn find_checksum_by_id_reads_one_column() {
    let (repo, pool) = repository().await;
    let checksum = "d".repeat(64);
    insert(&pool, "doc-1", "/lib/d/one.txt", &checksum).await;

    assert_eq!(
        repo.find_checksum_by_id("doc-1").await.unwrap(),
        checksum,
        "a document with no chunks still yields its checksum"
    );
    assert!(matches!(
        repo.find_checksum_by_id("missing").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn deleting_a_document_drops_it_from_the_referenced_set() {
    let (repo, pool) = repository().await;
    let checksum = "e".repeat(64);
    insert(&pool, "doc-1", "/lib/e/one.txt", &checksum).await;
    insert(&pool, "doc-2", "/lib/e/two.txt", &checksum).await;

    DocumentRepositoryPort::delete(&repo, "doc-1")
        .await
        .unwrap();
    assert_eq!(repo.count_by_checksum(&checksum).await.unwrap(), 1);
    assert_eq!(repo.list_checksums().await.unwrap(), vec![checksum.clone()]);

    DocumentRepositoryPort::delete(&repo, "doc-2")
        .await
        .unwrap();
    assert_eq!(repo.count_by_checksum(&checksum).await.unwrap(), 0);
    assert!(repo.list_checksums().await.unwrap().is_empty());
}

async fn aggregate(db: &Database, name: &str) -> lattice_core::domain::entities::Document {
    use lattice_core::domain::{
        entities::{chunk::Chunk, Document},
        value_objects::{
            source_context::{SourceContext, SourceGroup, StructureMode},
            Checksum,
        },
    };
    let metadata = Document::new(
        lattice_core::shared::types::ValidatedFilePath::new(std::path::PathBuf::from(format!(
            "/{name}.txt"
        )))
        .unwrap(),
        name.into(),
        "text/plain".into(),
        12,
        Checksum::new("a".repeat(64)).unwrap(),
    );
    let chunk = Chunk::new(metadata.id().clone(), "durable chunk".into(), 0);
    let tag = crate::features::tags::repository::TagRepository::new(db.clone())
        .get_or_create("contract", None)
        .await
        .unwrap();
    let mut document = metadata
        .from_parts(vec![chunk], vec![tag.id().clone()])
        .unwrap();
    document.set_source_context(Some(SourceContext {
        group: SourceGroup {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Sources".into(),
            edition: None,
            description: None,
            ordered: true,
            structure: StructureMode::Sections,
        },
        position: 2,
    }));
    document
}

async fn assert_contract(
    repository: &dyn DocumentRepositoryPort,
    document: &lattice_core::domain::entities::Document,
) {
    repository
        .save_batch(std::slice::from_ref(document))
        .await
        .unwrap();
    let loaded = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.chunks().len(), 1);
    assert_eq!(loaded.chunks()[0].content(), "durable chunk");
    assert_eq!(loaded.tags(), document.tags());
    assert_eq!(loaded.source_context(), document.source_context());
    repository.save(&loaded).await.unwrap();
    repository
        .rename(document.id().as_str(), "Renamed")
        .await
        .unwrap();
    assert_eq!(
        repository
            .find_by_id(document.id().as_str())
            .await
            .unwrap()
            .unwrap()
            .chunks()
            .len(),
        1
    );
    assert!(repository.list_metadata().await.unwrap()[0]
        .chunks()
        .is_empty());
}

#[tokio::test]
async fn standalone_and_in_unit_writes_share_the_aggregate_contract() {
    let (repository, _) = repository().await;
    let document = aggregate(&repository.db, "standalone").await;
    assert_contract(&repository, &document).await;
    let mut tx = repository.db.begin_write().await.unwrap();
    super::ops::save_batch(&mut tx, std::slice::from_ref(&document))
        .await
        .unwrap();
    let loaded = super::ops::find_by_id(&mut tx, document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.chunks().len(), 1);
    assert_eq!(loaded.chunks()[0].content(), "durable chunk");
    assert_eq!(loaded.tags(), document.tags());
    assert_eq!(loaded.source_context(), document.source_context());
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn failed_aggregate_save_preserves_existing_children() {
    let (repository, pool) = repository().await;
    let document = aggregate(&repository.db, "rollback").await;
    repository.save(&document).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_chunk BEFORE INSERT ON text_chunks BEGIN SELECT RAISE(ABORT, 'injected child failure'); END").execute(&pool).await.unwrap();
    assert!(repository.save(&document).await.is_err());
    let saved = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.chunks().len(), 1);
    assert_eq!(saved.tags(), document.tags());
}

#[tokio::test]
async fn aggregate_reads_fail_instead_of_silently_dropping_corrupt_chunks_or_tags() {
    let (repository, pool) = repository().await;
    let good = aggregate(&repository.db, "good-for-batch").await;
    repository.save(&good).await.unwrap();
    insert(&pool, "corrupt", "/corrupt.txt", &"f".repeat(64)).await;
    sqlx::query(
        "INSERT INTO text_chunks(id,document_id,content,chunk_index) VALUES('','corrupt','body',0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(repository.find_by_id("corrupt").await.is_err());
    assert!(repository.find_all().await.is_err());

    sqlx::query("DELETE FROM text_chunks WHERE document_id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tags(id,name) VALUES('not-a-uuid','corrupt-tag')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO document_tags(document_id,tag_id) VALUES('corrupt','not-a-uuid')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_id("corrupt").await.is_err());
    assert!(repository.find_all().await.is_err());
}

#[tokio::test]
async fn failed_document_error_message_survives_aggregate_save_and_load() {
    use lattice_core::domain::entities::document::DocumentStatus;

    let (repository, _) = repository().await;
    let mut document = aggregate(&repository.db, "failed-with-diagnostic").await;
    document.mark_failed();
    document.set_error_message(Some("unable to extract text".into()));

    repository.save(&document).await.unwrap();
    let loaded = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.status(), DocumentStatus::Failed);
    assert_eq!(loaded.error_message(), Some("unable to extract text"));

    repository.save(&loaded).await.unwrap();
    let saved_again = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved_again.error_message(), Some("unable to extract text"));
}

#[tokio::test]
async fn metadata_reads_reject_unrepresentable_counts_and_malformed_timestamps() {
    let (repository, pool) = repository().await;
    insert(&pool, "corrupt", "/corrupt.txt", &"f".repeat(64)).await;
    insert(&pool, "valid", "/valid.txt", &"e".repeat(64)).await;

    sqlx::query("UPDATE documents SET access_count=2147483648 WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_id("corrupt").await.is_err());
    assert!(repository.find_by_path_pattern("%").await.is_err());

    sqlx::query("UPDATE documents SET access_count=-1 WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_path("/corrupt.txt").await.is_err());

    sqlx::query("UPDATE documents SET access_count=0, modified_at='2025-12-07T08:16:30.12345é789+00:00' WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_id("corrupt").await.is_err());

    sqlx::query("UPDATE documents SET modified_at='2025-12-07T08:16:30.123456789+00:00', word_count=2147483648 WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_id("corrupt").await.is_err());

    sqlx::query("UPDATE documents SET word_count=-1 WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_path("/corrupt.txt").await.is_err());

    sqlx::query("UPDATE documents SET word_count=0 WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    let loaded = repository
        .find_by_path("/corrupt.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.modified_at().timestamp_subsec_nanos(), 123_456_789);

    sqlx::query("UPDATE documents SET last_accessed_at='not-a-timestamp' WHERE id='corrupt'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository.find_by_path("/corrupt.txt").await.is_err());
}

#[tokio::test]
async fn aggregate_lists_batch_children_and_preserve_each_documents_parts() {
    use crate::tests::common::query_counts::count_reads;
    let (repo, pool) = repository().await;
    let mut expected = std::collections::HashMap::new();
    // Cross the hydration batch boundary.
    for index in 0..501 {
        let document = aggregate(&repo.db, &format!("batch-{index}")).await;
        repo.save(&document).await.unwrap();
        expected.insert(document.id().as_str().to_owned(), document);
    }
    let (loaded, reads) = count_reads(repo.find_all()).await;
    let loaded = loaded.unwrap();
    assert_eq!(loaded.len(), 501);
    assert_eq!(
        reads, 5,
        "one metadata query and two child queries per batch"
    );
    for document in loaded {
        let original = &expected[document.id().as_str()];
        assert_eq!(document.chunks().len(), 1);
        assert_eq!(document.chunks()[0].id(), original.chunks()[0].id());
        assert_eq!(document.tags(), original.tags());
        assert_eq!(document.source_context(), original.source_context());
    }
    // Aggregate reads retain the domain rule requiring chunks; metadata
    // lookups remain available for a pending document.
    insert(&pool, "empty", "/empty.txt", &"b".repeat(64)).await;
    assert!(repo.find_all().await.is_err());
    assert_eq!(
        repo.find_metadata_by_ids(&["empty".into()])
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn access_batches_preserve_embeddings_and_increment_each_document_once() {
    use crate::tests::common::query_counts::count_reads;
    let (repo, pool) = repository().await;
    let document = aggregate(&repo.db, "search-hit").await;
    repo.save(&document).await.unwrap();
    insert(&pool, "pending", "/pending.txt", &"b".repeat(64)).await;
    insert(&pool, "unrelated", "/unrelated.txt", &"c".repeat(64)).await;
    sqlx::query("INSERT INTO text_embeddings(id,chunk_id,embedding,model_name,dimension) VALUES ('kept',?,x'0000803f','test',1)")
        .bind(document.chunks()[0].id().as_str()).execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_chunk_rewrite BEFORE DELETE ON text_chunks BEGIN SELECT RAISE(ABORT, 'access tracking rewrote chunks'); END")
        .execute(&pool).await.unwrap();
    let mut ids = vec![document.id().as_str().to_owned(); 2];
    ids.push("pending".into());
    ids.extend((0..33_000).map(|i| format!("missing-{i}")));
    let (tracked, reads) = count_reads(repo.record_accesses(&ids)).await;
    tracked.unwrap();
    assert_eq!(reads, 0, "access tracking does not read document bodies");
    let (first, second) = tokio::join!(repo.record_accesses(&ids), repo.record_accesses(&ids));
    first.unwrap();
    second.unwrap();
    let loaded = repo
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.access_count(), 3);
    assert!(loaded.last_accessed_at().is_some());
    assert_eq!(loaded.chunks(), document.chunks());
    assert_eq!(loaded.tags(), document.tags());
    assert_eq!(loaded.source_context(), document.source_context());
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id,access_count FROM documents WHERE id IN ('pending','unrelated') ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(counts, [("pending".into(), 3), ("unrelated".into(), 0)]);
    let embedding: Vec<u8> =
        sqlx::query_scalar("SELECT embedding FROM text_embeddings WHERE id='kept'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(embedding, [0, 0, 128, 63]);
    sqlx::query("UPDATE documents SET access_count=2147483647 WHERE id='pending'")
        .execute(&pool)
        .await
        .unwrap();
    repo.record_accesses(&["pending".into()]).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT access_count FROM documents WHERE id='pending'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, i32::MAX as i64);
    repo.db.close().await;
    repo.record_accesses(&[]).await.unwrap();
}

#[tokio::test]
async fn access_metadata_participates_in_the_callers_transaction() {
    let (repo, pool) = repository().await;
    insert(&pool, "hit", "/hit.txt", &"d".repeat(64)).await;
    let mut tx = repo.db.begin_write().await.unwrap();
    super::ops::record_accesses(&mut tx, &["hit".into()])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT access_count FROM documents WHERE id='hit'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn a_delete_waiting_behind_a_held_write_lock_outlasts_the_busy_timeout() {
    const BUSY_TIMEOUT: Duration = Duration::from_millis(100);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("documents.db");
    lattice_core::shared::testing::sqlite::write_migrated(&path)
        .await
        .unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(BUSY_TIMEOUT),
        )
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let repo = DocumentRepository::new(db.clone());
    insert(&pool, "doc-1", "/lib/one.txt", &"a".repeat(64)).await;

    let holder = db.begin_write().await.unwrap();
    let deleting = {
        let repo = repo.clone();
        tokio::spawn(async move { DocumentRepositoryPort::delete(&repo, "doc-1").await })
    };
    tokio::time::sleep(BUSY_TIMEOUT * 3).await;
    holder.commit().await.unwrap();

    deleting.await.unwrap().unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = 'doc-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
