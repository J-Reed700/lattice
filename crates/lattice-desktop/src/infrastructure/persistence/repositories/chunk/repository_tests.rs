//! The pool-backed and transaction-backed chunk repositories over the real
//! migrated schema.
use super::{ChunkFilter, SqliteChunkRepository, SqliteChunkRepositoryTx};
use lattice_core::application::ports::{ChunkRepositoryPort, Filter, RepositoryPort};
use lattice_core::domain::entities::chunk::Chunk;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::DocumentId;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

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

struct ForeignFilter;
impl Filter for ForeignFilter {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn by_document(document: &DocumentId, limit: Option<usize>) -> ChunkFilter {
    ChunkFilter {
        document_id: Some(document.as_str().to_owned()),
        limit,
    }
}

#[tokio::test]
async fn pool_repository_reads_in_index_order_and_filters_by_document() {
    let pool = pool().await;
    let repo = SqliteChunkRepository::new(pool.clone());
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
    assert_eq!(repo.count_indexed_documents().await.unwrap(), 1);
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
        repo.find_by_filter(&by_document(&mosses, None))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.find_by_filter(&by_document(&ferns, Some(10_000)))
            .await
            .unwrap()
            .len(),
        3
    );
    let no_document = ChunkFilter {
        document_id: None,
        limit: None,
    };
    assert!(repo.find_by_filter(&no_document).await.unwrap().is_empty());
    assert!(repo
        .find_by_filter(&ForeignFilter)
        .await
        .unwrap()
        .is_empty());
    for limit in [0, 10_001] {
        let refused = repo.find_by_filter(&by_document(&ferns, Some(limit))).await;
        assert!(
            matches!(refused, Err(AppError::InvalidInput(_))),
            "{refused:?}"
        );
    }
}

#[tokio::test]
async fn pool_batches_are_atomic_and_deletes_reach_only_their_targets() {
    let pool = pool().await;
    let repo = SqliteChunkRepository::new(pool.clone());
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
    assert_eq!(repo.count_indexed_documents().await.unwrap(), 2);

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
async fn transaction_repository_follows_its_transaction() {
    let pool = pool().await;
    let seeds = document(&pool, "indexed").await;

    let tx = Arc::new(Mutex::new(pool.begin().await.unwrap()));
    let repo = SqliteChunkRepositoryTx::new(tx.clone());
    repo.save_batch(&[]).await.unwrap();
    let saved = vec![chunk(&seeds, 1), chunk(&seeds, 0)];
    repo.save_batch(&saved).await.unwrap();
    assert_eq!(
        repo.find_by_document(seeds.as_str())
            .await
            .unwrap()
            .iter()
            .map(Chunk::index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        repo.count_by_document(seeds.as_str()).await.unwrap(),
        2usize
    );
    assert_eq!(
        repo.find_by_filter(&by_document(&seeds, None))
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(repo
        .find_by_filter(&ForeignFilter)
        .await
        .unwrap()
        .is_empty());
    let refused = repo.find_by_filter(&by_document(&seeds, Some(0))).await;
    assert!(
        matches!(refused, Err(AppError::InvalidInput(_))),
        "{refused:?}"
    );
    assert!(repo.exists(saved[0].id().as_str()).await.unwrap());

    let rolled_back = Arc::try_unwrap(tx)
        .ok()
        .expect("only the test owns the transaction");
    rolled_back.into_inner().rollback().await.unwrap();
    let dropped = repo.find_all().await;
    assert!(
        matches!(dropped, Err(AppError::InternalError(_))),
        "{dropped:?}"
    );
    let pool_repo = SqliteChunkRepository::new(pool.clone());
    assert_eq!(
        pool_repo.count_all().await.unwrap(),
        0,
        "rollback undid the writes"
    );

    let tx = Arc::new(Mutex::new(pool.begin().await.unwrap()));
    let repo = SqliteChunkRepositoryTx::new(tx.clone());
    let kept = repo
        .create(
            seeds.as_str(),
            "Acorns need a cold winter.",
            None,
            None,
            3,
            None,
            None,
        )
        .await
        .unwrap();
    repo.create_batch(vec![chunk(&seeds, 4)]).await.unwrap();
    repo.delete(kept.id().as_str()).await.unwrap();
    assert_eq!(RepositoryPort::count(&repo).await.unwrap(), 1);
    assert_eq!(repo.count_all().await.unwrap(), 1);
    let committed = Arc::try_unwrap(tx)
        .ok()
        .expect("only the test owns the transaction");
    committed.into_inner().commit().await.unwrap();
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
