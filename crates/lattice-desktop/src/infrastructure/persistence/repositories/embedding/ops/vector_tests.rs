use crate::features::embedding::repository::EmbeddingRepository;
use crate::infrastructure::persistence::repositories::embedding::SqliteEmbeddingRepositoryTx;
use crate::tests::common::query_counts::count_reads;
use lattice_core::application::ports::EmbeddingRepositoryPort;
use lattice_core::domain::entities::Embedding;
use lattice_core::shared::persistence::embedding_encoding::encode_embedding;
use lattice_core::shared::types::ChunkId;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn vector_batches_are_scoped_ordered_and_shared_by_both_adapters() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE text_chunks (id TEXT PRIMARY KEY, document_id TEXT, chunk_index INTEGER);
         CREATE TABLE text_embeddings (chunk_id TEXT PRIMARY KEY, embedding BLOB, dimension INTEGER);
         CREATE INDEX chunk_order ON text_chunks(document_id,chunk_index);
         INSERT INTO text_chunks VALUES ('a2','a',7),('a1','a',0),('b1','b',0),('private','private',0),('no-vector','a',9);",
    ).execute(&pool).await.unwrap();
    for (chunk, vector) in [
        ("a2", vec![2.0, 3.0]),
        ("a1", vec![1.0, 2.0]),
        ("b1", vec![4.0, 5.0]),
        ("private", vec![9.0, 9.0]),
    ] {
        sqlx::query("INSERT INTO text_embeddings VALUES (?,?,2)")
            .bind(chunk)
            .bind(encode_embedding(&vector))
            .execute(&pool)
            .await
            .unwrap();
    }
    let ids = ["b", "a", "a", "missing"].map(str::to_owned);
    let repo = EmbeddingRepository::new(pool.clone());
    let (vectors, reads) = count_reads(repo.vectors_by_document_ids(&ids)).await;
    let vectors = vectors.unwrap();
    assert_eq!(reads, 1);
    assert_eq!(vectors.len(), 2);
    assert_eq!(vectors["a"], [vec![1.0, 2.0], vec![2.0, 3.0]]);
    assert_eq!(vectors["b"], [vec![4.0, 5.0]]);
    let transaction = Arc::new(Mutex::new(pool.begin().await.unwrap()));
    let transactional = SqliteEmbeddingRepositoryTx::new(transaction.clone());
    let (loaded, reads) = count_reads(transactional.vectors_by_document_ids(&ids)).await;
    assert_eq!(loaded.unwrap(), vectors);
    assert_eq!(reads, 1);
    let (empty, reads) = count_reads(transactional.vectors_by_document_ids(&[])).await;
    assert!(empty.unwrap().is_empty());
    assert_eq!(reads, 0);
    drop(transactional);
    Arc::try_unwrap(transaction)
        .unwrap()
        .into_inner()
        .commit()
        .await
        .unwrap();
    sqlx::query("UPDATE text_embeddings SET embedding=x'010203' WHERE chunk_id='a1'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.vectors_by_document_ids(&ids).await.is_err());
}

#[tokio::test]
async fn save_batch_validates_every_vector_before_writing() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE text_embeddings (id TEXT PRIMARY KEY, chunk_id TEXT UNIQUE, embedding BLOB, model_name TEXT, dimension INTEGER, created_at TEXT)")
        .execute(&pool).await.unwrap();
    let mut entries = (0..150)
        .map(|index| {
            (
                Embedding::new(ChunkId::from(format!("valid-{index}")), "model".into(), 2),
                vec![1.0, 0.0],
            )
        })
        .collect::<Vec<_>>();
    entries.push((
        Embedding::new(
            ChunkId::from("wrong-dimension".to_string()),
            "model".into(),
            3,
        ),
        vec![0.0, 1.0],
    ));
    let mut conn = pool.acquire().await.unwrap();

    assert!(super::save_batch(&mut conn, entries).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_embeddings")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 0, "invalid input must be rejected before any write");

    let non_finite = Embedding::new(ChunkId::new(), "model".into(), 2);
    assert!(
        super::save_batch(&mut conn, vec![(non_finite, vec![f32::NAN, 0.0])])
            .await
            .is_err()
    );

    let valid_entries = (0..301)
        .map(|index| {
            (
                Embedding::new(ChunkId::from(format!("saved-{index}")), "model".into(), 2),
                vec![index as f32, 0.0],
            )
        })
        .collect();
    super::save_batch(&mut conn, valid_entries).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_embeddings")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 301);
    let (_, vector) = super::find_by_chunk_id(&mut conn, "saved-300")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(vector, [300.0, 0.0]);
}

#[tokio::test]
async fn repository_reads_reject_wrong_dimension_and_non_finite_vectors() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE text_chunks (id TEXT PRIMARY KEY, document_id TEXT, chunk_index INTEGER);
         CREATE TABLE text_embeddings (id TEXT PRIMARY KEY, chunk_id TEXT UNIQUE, embedding BLOB, model_name TEXT, dimension INTEGER, created_at TEXT);
         INSERT INTO text_chunks VALUES ('chunk', 'document', 0);",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO text_embeddings VALUES ('embedding','chunk',?,'model',1,'2026-01-01T00:00:00Z')")
        .bind(encode_embedding(&[1.0, 0.0]))
        .execute(&pool)
        .await
        .unwrap();
    let repo = EmbeddingRepository::new(pool.clone());

    assert!(repo.find_by_chunk_id("chunk").await.is_err());
    assert!(repo.find_by_document_id("document").await.is_err());
    assert!(repo
        .vectors_by_document_ids(&["document".to_string()])
        .await
        .is_err());

    sqlx::query("UPDATE text_embeddings SET embedding = ?, dimension = 2 WHERE chunk_id = 'chunk'")
        .bind(encode_embedding(&[f32::NAN, 0.0]))
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.find_by_chunk_id("chunk").await.is_err());
    assert!(repo.find_by_document_id("document").await.is_err());
    assert!(repo
        .vectors_by_document_ids(&["document".to_string()])
        .await
        .is_err());
}
