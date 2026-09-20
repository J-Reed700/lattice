//! Opt-in real-model lifecycle check. All state lives in a temporary directory.
use super::*;
use crate::application::factories::ChecksumFactory;
use crate::application::ports::{EmbeddingPort, VectorSearchPort};
use crate::domain::entities::document::Document;
use crate::domain::value_objects::{ChunkingStrategy, FileMetadata};
use crate::features::embedding::{candle_service::CandleEmbeddingService, encoding, generation};
use crate::features::indexing::use_cases::embedding_input::{
    embed_prepared_chunks, prepare_structured_with_spans,
};
use crate::features::search::engine::vector_search::{VectorIndexCompression, VectorQuantization};
use crate::shared::domain_types::ValidatedFilePath;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[tokio::test]
#[ignore = "requires LATTICE_TEST_EMBEDDING_MODEL pointing at downloaded model artifacts"]
async fn real_model_import_search_and_disk_restart_preserve_results() {
    let model_dir = std::env::var_os("LATTICE_TEST_EMBEDDING_MODEL")
        .expect("set LATTICE_TEST_EMBEDDING_MODEL to a local model directory");
    let model = CandleEmbeddingService::open_unregistered(Path::new(&model_dir)).unwrap();
    let dimension = model.dimension();
    let identity = model.model_identity();
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("library.db"))
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // Actual repository prose, rather than seeded vectors or generated facts.
    // This checks component integration, not held-out retrieval accuracy.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../evals/retrieval/EVALUATION_PROTOCOL.md")
        .canonicalize()
        .unwrap();
    let text = std::fs::read_to_string(&source).unwrap();
    let metadata = FileMetadata::new(
        "EVALUATION_PROTOCOL.md".into(),
        "text/markdown".into(),
        text.len() as i64,
        chrono::Utc::now(),
    )
    .unwrap();
    let checksum = ChecksumFactory::from_bytes(text.as_bytes()).unwrap();
    let doc = Document::from_file(
        ValidatedFilePath::new(source.clone()).unwrap(),
        metadata,
        checksum,
        text,
        ChunkingStrategy::FixedSize { size: 512 },
    )
    .unwrap();
    let (doc, spans) = prepare_structured_with_spans(doc, &model, &[]).unwrap();
    let embeddings = embed_prepared_chunks(&doc, &model, &spans).await.unwrap();
    assert_eq!(doc.chunks().len(), embeddings.len());
    assert!(embeddings.len() > 1, "exercise multiple real chunks");
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) VALUES (?, ?, 'EVALUATION_PROTOCOL.md', 0, CURRENT_TIMESTAMP, 'smoke')")
        .bind(doc.id().as_str()).bind(source.to_str().unwrap())
        .execute(&mut *tx).await.unwrap();
    for (chunk, vector) in doc.chunks().iter().zip(&embeddings) {
        sqlx::query("INSERT INTO text_chunks (id, document_id, content, contextualized_content, chunk_index) VALUES (?, ?, ?, ?, ?)")
            .bind(chunk.id().as_str()).bind(doc.id().as_str()).bind(chunk.content())
            .bind(chunk.embedding_text()).bind(chunk.chunk_index() as i64)
            .execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) VALUES (?, ?, ?, ?, ?)")
            .bind(encoding::vector_key(chunk.id().as_str())).bind(chunk.id().as_str())
            .bind(encoding::encode_embedding(vector)).bind(&identity).bind(dimension as i64)
            .execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let lexical: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'review'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        lexical > 0,
        "fresh migration must populate the lexical index"
    );
    let query = model
        .embed_query("evidence-bound human review and release gates")
        .await
        .unwrap();
    drop(model);

    for (name, compression) in [
        ("full", VectorIndexCompression::None),
        (
            "compressed",
            VectorIndexCompression::truncated(dimension.min(256), VectorQuantization::I8),
        ),
    ] {
        let path = dir.path().join(format!("{name}.usearch"));
        let index = Arc::new(
            USearchVectorIndex::open_or_create_with_compression(
                dimension,
                path.clone(),
                compression.clone(),
            )
            .unwrap()
            .with_coalesced_saves(),
        );
        let outcome = open_or_rebuild(
            &index,
            &pool,
            &path,
            &identity,
            &identity,
            dimension,
            || generation::restore(&pool, &identity, dimension),
        )
        .await
        .unwrap();
        assert!(outcome.rebuilt());
        let before = VectorSearchPort::search(index.as_ref(), &query, 5, 0.0).unwrap();
        assert!(!before.is_empty());
        drop(index);

        // Reopen both disk-backed resources, with no encoder involved.
        let reopened_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        let index = Arc::new(
            USearchVectorIndex::open_or_create_with_compression(
                dimension,
                path.clone(),
                compression,
            )
            .unwrap()
            .with_coalesced_saves(),
        );
        let outcome = open_or_rebuild(
            &index,
            &reopened_pool,
            &path,
            &identity,
            &identity,
            dimension,
            || async { Err(AppError::Other("unexpected rebuild on restart".into())) },
        )
        .await
        .unwrap();
        assert!(!outcome.rebuilt());
        let after = VectorSearchPort::search(index.as_ref(), &query, 5, 0.0).unwrap();
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.chunk_id, b.chunk_id);
            assert_eq!(a.content, b.content, "text must be hydrated after restart");
            assert!((a.score - b.score).abs() < 1e-5);
        }
        eprintln!(
            "{name}: {} real chunks; lexical search and {} ranked hits survive disk restart",
            embeddings.len(),
            after.len()
        );
        reopened_pool.close().await;
    }
    pool.close().await;
}
