use super::*;
use lattice_core::shared::types::DocumentId;

/// In-memory pool with the real migrations applied, so the tests exercise the
/// production `text_chunks` schema (`language`, `token_count`, `word_count`,
/// `has_code`, `section`, `page_number`) that this repository reads and writes.
async fn create_test_pool() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// `text_chunks.document_id` is a foreign key onto `documents(id)` and sqlx
/// enables `PRAGMA foreign_keys` by default, so every chunk needs a parent row.
async fn insert_document(pool: &SqlitePool, document_id: &DocumentId) {
    sqlx::query(
        r#"
            INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum)
            VALUES (?, ?, 'chunk-fixture.md', 1, '2026-01-01T00:00:00Z', 'checksum')
            "#,
    )
    .bind(document_id.as_str())
    .bind(format!("/vault/{}.md", document_id.as_str()))
    .execute(pool)
    .await
    .unwrap();
}

/// Migrated pool + a parent document row, ready for chunk persistence.
async fn create_test_repo() -> (ChunkRepository, DocumentId) {
    let pool = create_test_pool().await;
    let doc_id = DocumentId::new();
    insert_document(&pool, &doc_id).await;
    (ChunkRepository::new(pool), doc_id)
}

#[tokio::test]
async fn test_save_and_find_by_id() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = ChunkEntity::new(doc_id, "Test chunk content".to_string(), 0);

    repo.save(&entity).await.unwrap();

    let found = repo.find_by_id(entity.id().as_str()).await.unwrap();
    assert!(found.is_some());

    let found_entity = found.unwrap();
    assert_eq!(found_entity.id().as_str(), entity.id().as_str());
    assert_eq!(found_entity.content(), "Test chunk content");
    assert_eq!(found_entity.index(), 0);
}

#[tokio::test]
async fn find_first_by_document_reads_only_the_opening_chunks() {
    let (repo, doc_id) = create_test_repo().await;
    for index in (0..5).rev() {
        let chunk = ChunkEntity::new(doc_id.clone(), format!("Chunk {index}"), index);
        repo.save(&chunk).await.unwrap();
    }

    let chunks = ChunkRepositoryPort::find_first_by_document(&repo, doc_id.as_str(), 2)
        .await
        .unwrap();

    let contents: Vec<_> = chunks.iter().map(|c| c.content().to_owned()).collect();
    assert_eq!(contents, vec!["Chunk 0", "Chunk 1"]);

    let filtered = repo
        .find_by_filter(&ChunkFilter {
            document_id: Some(doc_id.as_str().to_owned()),
            limit: Some(1),
        })
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].index(), 0);
}

#[tokio::test]
async fn test_find_by_document() {
    let (repo, doc_id) = create_test_repo().await;

    let chunk1 = ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0);
    let chunk2 = ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1);

    repo.save(&chunk1).await.unwrap();
    repo.save(&chunk2).await.unwrap();

    let chunks = repo.find_by_document(doc_id.as_str()).await.unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].content(), "Chunk 1");
    assert_eq!(chunks[1].content(), "Chunk 2");
}

#[tokio::test]
async fn test_save_batch() {
    let (repo, doc_id) = create_test_repo().await;

    let entities = vec![
        ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
        ChunkEntity::new(doc_id.clone(), "Chunk 3".to_string(), 2),
    ];

    repo.save_batch(&entities).await.unwrap();

    let count = repo.count().await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_delete() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = ChunkEntity::new(doc_id, "Test chunk".to_string(), 0);

    repo.save(&entity).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 1);

    repo.delete(entity.id().as_str()).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_delete_by_document() {
    let (repo, doc_id) = create_test_repo().await;

    let chunks = vec![
        ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
    ];

    repo.save_batch(&chunks).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 2);

    repo.delete_by_document(doc_id.as_str()).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_count_by_document() {
    let (repo, doc_id) = create_test_repo().await;

    let chunks = vec![
        ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
        ChunkEntity::new(doc_id.clone(), "Chunk 3".to_string(), 2),
    ];

    repo.save_batch(&chunks).await.unwrap();

    let count = repo.count_by_document(doc_id.as_str()).await.unwrap();
    assert_eq!(count, 3);
}

/// The OCR mark survives the repository's upsert and the document
/// aggregate's replace, and comes back from both reads, so a citation built
/// from a stored chunk still knows the text was read off a scan.
#[tokio::test]
async fn an_ocr_chunk_keeps_its_mark_through_the_database() {
    use crate::infrastructure::persistence::repositories::document::ops::{
        fetch_chunks_for_document, save_chunks,
    };
    let (repo, doc_id) = create_test_repo().await;
    let mut scanned = ChunkEntity::new(doc_id.clone(), "Recognized scan".to_string(), 0);
    scanned.set_provenance(None, Some(0), Some(15), Some(2));
    scanned.set_from_ocr(true);
    let typed = ChunkEntity::new(doc_id.clone(), "Text layer".to_string(), 1);
    let chunks = [scanned.clone(), typed.clone()];
    let mark = |found: &[ChunkEntity], chunk: &ChunkEntity| {
        found
            .iter()
            .find(|c| c.id() == chunk.id())
            .map(|c| (c.from_ocr(), c.page_number()))
    };

    repo.save_batch(&chunks).await.unwrap();
    let ids: Vec<String> = chunks.iter().map(|c| c.as_str().to_string()).collect();
    let found = ChunkRepositoryPort::find_by_ids(&repo, &ids).await.unwrap();

    assert_eq!(mark(&found, &scanned), Some((true, Some(2))));
    assert_eq!(mark(&found, &typed), Some((false, None)));

    let mut conn = repo.pool.acquire().await.unwrap();
    save_chunks(&mut conn, doc_id.as_str(), &chunks)
        .await
        .unwrap();
    let reloaded = fetch_chunks_for_document(&mut conn, doc_id.as_str())
        .await
        .unwrap();

    assert_eq!(mark(&reloaded, &scanned), Some((true, Some(2))));
    assert_eq!(mark(&reloaded, &typed), Some((false, None)));
}

#[tokio::test]
async fn test_exists() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = ChunkEntity::new(doc_id, "Test chunk".to_string(), 0);

    assert!(!repo.exists(entity.id().as_str()).await.unwrap());

    repo.save(&entity).await.unwrap();

    assert!(repo.exists(entity.id().as_str()).await.unwrap());
}
