use crate::features::embedding::repository::EmbeddingRepository;
use crate::infrastructure::persistence::repositories::embedding::SqliteEmbeddingRepositoryTx;
use crate::tests::common::query_counts::count_reads;
use lattice_core::application::ports::EmbeddingRepositoryPort;
use lattice_core::shared::persistence::embedding_encoding::encode_embedding;
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
         CREATE TABLE text_embeddings (chunk_id TEXT PRIMARY KEY, embedding BLOB);
         CREATE INDEX chunk_order ON text_chunks(document_id,chunk_index);
         INSERT INTO text_chunks VALUES ('a2','a',7),('a1','a',0),('b1','b',0),('private','private',0),('no-vector','a',9);",
    ).execute(&pool).await.unwrap();
    for (chunk, vector) in [
        ("a2", vec![2.0, 3.0]),
        ("a1", vec![1.0, 2.0]),
        ("b1", vec![4.0, 5.0]),
        ("private", vec![9.0, 9.0]),
    ] {
        sqlx::query("INSERT INTO text_embeddings VALUES (?,?)")
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
