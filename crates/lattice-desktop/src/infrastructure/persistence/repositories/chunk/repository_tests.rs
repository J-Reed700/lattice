//! The pool-backed chunk repository and the chunk operations inside a unit of
//! work, over the real migrated schema.
use super::ops::CHUNK_ROWS_PER_INSERT;
use super::{ops, ChunkRepository};
use lattice_core::application::ports::{ChunkRepositoryPort, RepositoryPort};
use lattice_core::domain::entities::chunk::Chunk;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;
use lattice_core::shared::types::DocumentId;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("foreign keys on");
    pool
}

async fn document(pool: &SqlitePool, status: &str) -> DocumentId {
    let id = DocumentId::new();
    sqlx::query("INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum, status) VALUES (?, ?, 'herbarium.md', 1, '2026-01-01T00:00:00Z', 'checksum', ?)")
        .bind(id.as_str())
        .bind(format!("/vault/{}.md", id.as_str()))
        .bind(status)
        .execute(pool)
        .await
        .expect("document row");
    id
}

fn chunk(document: &DocumentId, index: usize) -> Chunk {
    Chunk::new(
        document.clone(),
        format!("Pressed fern specimen, sheet {index}."),
        index,
    )
}

fn ids(chunks: &[Chunk]) -> Vec<String> {
    chunks.iter().map(|c| c.id().as_str().to_owned()).collect()
}

#[tokio::test]
async fn pool_repository_reads_in_index_order_and_by_document() {
    let pool = pool().await;
    let db = Database::new(pool.clone());
    let repo = ChunkRepository::new(db.clone());
    let ferns = document(&pool, "indexed").await;
    let mosses = document(&pool, "pending").await;
    let fern_chunks = vec![chunk(&ferns, 2), chunk(&ferns, 0), chunk(&ferns, 1)];
    for c in &fern_chunks {
        repo.save(c).await.unwrap();
    }
    let moss = chunk(&mosses, 0);
    repo.save(&moss).await.unwrap();

    let ordered = repo.find_by_document(ferns.as_str()).await.unwrap();
    assert_eq!(
        ordered.iter().map(Chunk::index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(ordered[0].content(), "Pressed fern specimen, sheet 0.");
    let first_two = repo
        .find_first_by_document(ferns.as_str(), 2)
        .await
        .unwrap();
    assert_eq!(
        first_two.iter().map(Chunk::index).collect::<Vec<_>>(),
        vec![0, 1]
    );

    assert_eq!(RepositoryPort::count(&repo).await.unwrap(), 4);
    assert_eq!(
        repo.count_by_document(ferns.as_str()).await.unwrap(),
        3usize
    );
    assert_eq!(
        ChunkRepositoryPort::count_by_document(&repo, ferns.as_str())
            .await
            .unwrap(),
        3i64
    );
    assert_eq!(repo.count_all().await.unwrap(), 4);
    assert_eq!(repo.find_all().await.unwrap().len(), 4);

    let moss_id = moss.id().as_str().to_owned();
    assert!(repo.exists(&moss_id).await.unwrap());
    assert!(!repo.exists("missing").await.unwrap());
    assert!(repo.find_by_id("missing").await.unwrap().is_none());
    assert_eq!(
        repo.find_by_id(&moss_id).await.unwrap().map(|c| c.index()),
        Some(0)
    );
    assert!(repo.find_by_ids(&[]).await.unwrap().is_empty());
    let picked = repo
        .find_by_ids(&[
            moss_id.clone(),
            ids(&fern_chunks)[0].clone(),
            "missing".into(),
        ])
        .await
        .unwrap();
    assert_eq!(picked.len(), 2);

    assert_eq!(
        repo.find_by_document(mosses.as_str()).await.unwrap().len(),
        1
    );
    let limited = repo
        .find_first_by_document(ferns.as_str(), 1)
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].index(), 0);
    assert!(repo.find_by_document("missing").await.unwrap().is_empty());
}

#[tokio::test]
async fn pool_repository_reports_corrupt_chunk_rows_instead_of_wrapping_them() {
    let pool = pool().await;
    let db = Database::new(pool.clone());
    let repo = ChunkRepository::new(db.clone());
    let document = document(&pool, "indexed").await;
    let chunk = chunk(&document, 0);
    repo.save(&chunk).await.unwrap();
    sqlx::query("UPDATE text_chunks SET word_count = -1 WHERE id = ?")
        .bind(chunk.id().as_str())
        .execute(&pool)
        .await
        .unwrap();

    let result = repo.find_by_id(chunk.id().as_str()).await;
    assert!(
        matches!(result, Err(AppError::InvalidData(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn pool_batches_are_atomic_and_deletes_reach_only_their_targets() {
    let pool = pool().await;
    let db = Database::new(pool.clone());
    let repo = ChunkRepository::new(db.clone());
    let herbs = document(&pool, "indexed").await;
    let lichens = document(&pool, "indexed").await;

    repo.save_batch(&[]).await.unwrap();
    let orphan = chunk(&DocumentId::new(), 0);
    let failed = repo.save_batch(&[chunk(&herbs, 0), orphan]).await;
    assert!(
        failed.is_err(),
        "a chunk without its document must fail the batch"
    );
    assert_eq!(repo.count_all().await.unwrap(), 0, "the batch rolled back");

    let created = repo
        .create(
            herbs.as_str(),
            "Basil keeps best in water.",
            None,
            None,
            5,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(created.index(), 5);
    assert!(repo.exists(created.id().as_str()).await.unwrap());
    let batch = repo
        .create_batch(vec![chunk(&herbs, 6), chunk(&herbs, 7), chunk(&lichens, 0)])
        .await
        .unwrap();
    assert_eq!(repo.count_all().await.unwrap(), 4);

    repo.delete_batch(&[]).await.unwrap();
    let batch_ids = ids(&batch);
    repo.delete_batch(&[batch_ids[0].as_str(), batch_ids[1].as_str()])
        .await
        .unwrap();
    assert_eq!(repo.count_by_document(herbs.as_str()).await.unwrap(), 1);
    repo.delete(created.id().as_str()).await.unwrap();
    repo.delete("missing").await.unwrap();
    assert_eq!(repo.count_by_document(herbs.as_str()).await.unwrap(), 0);

    repo.delete_by_document(lichens.as_str()).await.unwrap();
    assert_eq!(repo.count_all().await.unwrap(), 0);
}

#[tokio::test]
async fn chunk_batch_writes_are_bounded_and_late_batch_failure_rolls_back() {
    use crate::tests::common::query_counts::count_writes;

    let pool = pool().await;
    let db = Database::new(pool.clone());
    let repo = ChunkRepository::new(db.clone());
    let owner = document(&pool, "indexed").await;
    let chunks: Vec<_> = (0..2 * CHUNK_ROWS_PER_INSERT + 1)
        .map(|index| chunk(&owner, index))
        .collect();
    let (saved, writes) = count_writes(repo.save_batch(&chunks)).await;
    saved.unwrap();
    assert_eq!(
        writes, 3,
        "two full INSERTs and one more chunk should use three INSERTs"
    );
    assert_eq!(
        repo.count_by_document(owner.as_str()).await.unwrap(),
        2 * CHUNK_ROWS_PER_INSERT + 1
    );

    let other = document(&pool, "indexed").await;
    let mut failing: Vec<_> = (0..CHUNK_ROWS_PER_INSERT)
        .map(|index| chunk(&other, index))
        .collect();
    failing.push(chunk(&DocumentId::new(), CHUNK_ROWS_PER_INSERT));
    assert!(repo.save_batch(&failing).await.is_err());
    assert_eq!(
        repo.count_by_document(other.as_str()).await.unwrap(),
        0,
        "a foreign-key failure in the second INSERT must roll back the first"
    );

    let mut tx = db.begin_write().await.unwrap();
    let (saved, writes) = count_writes(ops::save_batch_optimized(&mut tx, &chunks)).await;
    saved.unwrap();
    assert_eq!(writes, 3, "a unit of work should use shared bulk writes");
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn chunk_operations_follow_their_unit_of_work() {
    let pool = pool().await;
    let db = Database::new(pool.clone());
    let seeds = document(&pool, "indexed").await;

    let mut tx = db.begin_write().await.unwrap();
    ops::save_batch_optimized(&mut tx, &[]).await.unwrap();
    let saved = vec![chunk(&seeds, 1), chunk(&seeds, 0)];
    ops::save_batch_optimized(&mut tx, &saved).await.unwrap();
    assert_eq!(
        ops::find_by_document(&mut tx, seeds.as_str())
            .await
            .unwrap()
            .iter()
            .map(Chunk::index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        ops::count_by_document(&mut tx, seeds.as_str())
            .await
            .unwrap(),
        2usize
    );
    let limited = ops::find_by_document_limit(&mut tx, seeds.as_str(), Some(1))
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].index(), 0);
    assert!(ops::exists(&mut tx, saved[0].id().as_str()).await.unwrap());
    tx.rollback().await.unwrap();

    let pool_repo = ChunkRepository::new(db.clone());
    assert_eq!(
        pool_repo.count_all().await.unwrap(),
        0,
        "rollback undid the writes"
    );

    let mut tx = db.begin_write().await.unwrap();
    let kept = chunk(&seeds, 3);
    ops::save(&mut tx, &kept).await.unwrap();
    ops::save_batch_optimized(&mut tx, &[chunk(&seeds, 4)])
        .await
        .unwrap();
    ops::delete(&mut tx, kept.id().as_str()).await.unwrap();
    assert_eq!(ops::count(&mut tx).await.unwrap(), 1);
    assert_eq!(ops::count_all(&mut tx).await.unwrap(), 1);
    tx.commit().await.unwrap();
    assert_eq!(
        pool_repo
            .find_by_document(seeds.as_str())
            .await
            .unwrap()
            .len(),
        1,
        "commit kept the writes"
    );
}

/// Migrated pool + a parent document row, ready for chunk persistence.
async fn create_test_repo() -> (ChunkRepository, DocumentId) {
    let pool = pool().await;
    let doc_id = document(&pool, "indexed").await;
    (ChunkRepository::new(Database::new(pool)), doc_id)
}

#[tokio::test]
async fn test_save_and_find_by_id() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = Chunk::new(doc_id, "Test chunk content".to_string(), 0);

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
        let chunk = Chunk::new(doc_id.clone(), format!("Chunk {index}"), index);
        repo.save(&chunk).await.unwrap();
    }

    let chunks = ChunkRepositoryPort::find_first_by_document(&repo, doc_id.as_str(), 2)
        .await
        .unwrap();

    let contents: Vec<_> = chunks.iter().map(|c| c.content().to_owned()).collect();
    assert_eq!(contents, vec!["Chunk 0", "Chunk 1"]);
}

#[tokio::test]
async fn test_find_by_document() {
    let (repo, doc_id) = create_test_repo().await;

    let chunk1 = Chunk::new(doc_id.clone(), "Chunk 1".to_string(), 0);
    let chunk2 = Chunk::new(doc_id.clone(), "Chunk 2".to_string(), 1);

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
        Chunk::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        Chunk::new(doc_id.clone(), "Chunk 2".to_string(), 1),
        Chunk::new(doc_id.clone(), "Chunk 3".to_string(), 2),
    ];

    repo.save_batch(&entities).await.unwrap();

    let count = repo.count().await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_delete() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = Chunk::new(doc_id, "Test chunk".to_string(), 0);

    repo.save(&entity).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 1);

    repo.delete(entity.id().as_str()).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_delete_by_document() {
    let (repo, doc_id) = create_test_repo().await;

    let chunks = vec![
        Chunk::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        Chunk::new(doc_id.clone(), "Chunk 2".to_string(), 1),
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
        Chunk::new(doc_id.clone(), "Chunk 1".to_string(), 0),
        Chunk::new(doc_id.clone(), "Chunk 2".to_string(), 1),
        Chunk::new(doc_id.clone(), "Chunk 3".to_string(), 2),
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
    let pool = pool().await;
    let doc_id = document(&pool, "indexed").await;
    let db = Database::new(pool.clone());
    let repo = ChunkRepository::new(db.clone());
    let mut scanned = Chunk::new(doc_id.clone(), "Recognized scan".to_string(), 0);
    scanned.set_provenance(None, Some(0), Some(15), Some(2));
    scanned.set_from_ocr(true);
    let typed = Chunk::new(doc_id.clone(), "Text layer".to_string(), 1);
    let chunks = [scanned.clone(), typed.clone()];
    let mark = |found: &[Chunk], chunk: &Chunk| {
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

    let mut tx = db.begin_write().await.unwrap();
    save_chunks(&mut tx, doc_id.as_str(), &chunks)
        .await
        .unwrap();
    let reloaded = fetch_chunks_for_document(&mut tx, doc_id.as_str())
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(mark(&reloaded, &scanned), Some((true, Some(2))));
    assert_eq!(mark(&reloaded, &typed), Some((false, None)));
}

#[tokio::test]
async fn test_exists() {
    let (repo, doc_id) = create_test_repo().await;

    let entity = Chunk::new(doc_id, "Test chunk".to_string(), 0);

    assert!(!repo.exists(entity.id().as_str()).await.unwrap());

    repo.save(&entity).await.unwrap();

    assert!(repo.exists(entity.id().as_str()).await.unwrap());
}
