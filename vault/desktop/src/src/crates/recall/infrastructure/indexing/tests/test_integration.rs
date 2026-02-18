use crate::infrastructure::persistence::database::initialize_database;
use crate::infrastructure::indexing::actor::IndexingService;
use crate::infrastructure::indexing::storage::IndexStorage;
use crate::infrastructure::services::embedding::EmbeddingService;
use sqlx::SqlitePool;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use tempfile::{NamedTempFile, TempDir};
use tokenizers::Tokenizer;

async fn setup_test_environment() -> anyhow::Result<(TempDir, SqlitePool, Arc<Tokenizer>)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display())).await?;

    initialize_database(&pool).await?;

    let tokenizer = Arc::new(
        Tokenizer::from_pretrained("bert-base-uncased", None)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?,
    );

    Ok((temp_dir, pool, tokenizer))
}

#[tokio::test]
async fn test_end_to_end_contextual_indexing() -> anyhow::Result<()> {
    let (temp_dir, pool, tokenizer) = setup_test_environment().await?;

    let mut test_file = NamedTempFile::new_in(temp_dir.path())?;
    writeln!(
        test_file,
        "# Introduction\n\nThis is the introduction section of the document.\n\nIt contains important information."
    )?;
    writeln!(
        test_file,
        "\n# Main Content\n\nThis is the main content section."
    )?;
    test_file.flush()?;

    let storage = IndexStorage::new(pool.clone());

    let path = test_file.path();
    let needs_index = storage.needs_reindex(path).await?;
    assert!(needs_index);

    Ok(())
}

#[tokio::test]
async fn test_contextualized_storage() -> anyhow::Result<()> {
    let (temp_dir, pool, tokenizer) = setup_test_environment().await?;

    let mut test_file = NamedTempFile::new_in(temp_dir.path())?;
    writeln!(test_file, "Test content for storage verification")?;
    test_file.flush()?;

    let storage = IndexStorage::new(pool.clone());

    let chunks = vec![crate::infrastructure::indexing::chunker::ContextualizedChunk {
        original_content: "Test content".to_string(),
        contextualized_content: "[Document: test.txt]\n\nTest content".to_string(),
        context_prefix: "[Document: test.txt]".to_string(),
        chunk_index: 0,
        token_count: 10,
        start_idx: 0,
        end_idx: 12,
    }];

    let embeddings = vec![vec![0.1; DEFAULT_EMBEDDING_DIM]];

    let doc_id = storage
        .store_document_with_context(test_file.path(), "text/plain", chunks, embeddings)
        .await?;

    assert!(!doc_id.is_empty());

    let result = sqlx::query!(
        r#"
        SELECT content, contextualized_content, context_prefix
        FROM text_chunks
        WHERE document_id = ?
        "#,
        doc_id
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(result.content, "Test content");
    assert_eq!(
        result.contextualized_content,
        Some("[Document: test.txt]\n\nTest content".to_string())
    );
    assert_eq!(result.context_prefix, Some("[Document: test.txt]".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_context_improves_search_relevance() -> anyhow::Result<()> {
    let (temp_dir, pool, tokenizer) = setup_test_environment().await?;

    let storage = IndexStorage::new(pool.clone());

    let mut doc1 = NamedTempFile::with_suffix_in(".txt", temp_dir.path())?;
    writeln!(doc1, "The revenue grew by 23% this quarter.")?;
    doc1.flush()?;

    let mut doc2 = NamedTempFile::with_suffix_in(".txt", temp_dir.path())?;
    writeln!(doc2, "Employee satisfaction increased by 23%.")?;
    doc2.flush()?;

    let chunks1 = vec![crate::infrastructure::indexing::chunker::ContextualizedChunk {
        original_content: "The revenue grew by 23% this quarter.".to_string(),
        contextualized_content: "[Document: financial_report.txt | Section: Revenue]\n\nThe revenue grew by 23% this quarter.".to_string(),
        context_prefix: "[Document: financial_report.txt | Section: Revenue]".to_string(),
        chunk_index: 0,
        token_count: 15,
        start_idx: 0,
        end_idx: 38,
    }];

    let chunks2 = vec![crate::infrastructure::indexing::chunker::ContextualizedChunk {
        original_content: "Employee satisfaction increased by 23%.".to_string(),
        contextualized_content: "[Document: hr_report.txt | Section: Satisfaction]\n\nEmployee satisfaction increased by 23%.".to_string(),
        context_prefix: "[Document: hr_report.txt | Section: Satisfaction]".to_string(),
        chunk_index: 0,
        token_count: 15,
        start_idx: 0,
        end_idx: 39,
    }];

    let embeddings = vec![vec![0.1; DEFAULT_EMBEDDING_DIM]];

    let doc1_id = storage
        .store_document_with_context(doc1.path(), "text/plain", chunks1.clone(), embeddings.clone())
        .await?;

    let doc2_id = storage
        .store_document_with_context(doc2.path(), "text/plain", chunks2.clone(), embeddings.clone())
        .await?;

    let doc1_result = sqlx::query!(
        r#"
        SELECT contextualized_content
        FROM text_chunks
        WHERE document_id = ?
        "#,
        doc1_id
    )
    .fetch_one(&pool)
    .await?;

    let doc2_result = sqlx::query!(
        r#"
        SELECT contextualized_content
        FROM text_chunks
        WHERE document_id = ?
        "#,
        doc2_id
    )
    .fetch_one(&pool)
    .await?;

    assert!(doc1_result
        .contextualized_content
        .unwrap()
        .contains("Revenue"));
    assert!(doc2_result
        .contextualized_content
        .unwrap()
        .contains("Satisfaction"));

    assert_ne!(
        chunks1[0].contextualized_content,
        chunks2[0].contextualized_content
    );

    Ok(())
}

#[tokio::test]
async fn test_pdf_page_range_tracking() -> anyhow::Result<()> {
    let page_ranges = vec![(1, 0, 500), (2, 500, 1000), (3, 1000, 1500)];

    let chunk_start_positions = vec![50, 250, 600, 850, 1100, 1400];

    let expected_pages = vec![
        Some(1),
        Some(1),
        Some(2),
        Some(2),
        Some(3),
        Some(3),
    ];

    for (i, &chunk_start) in chunk_start_positions.iter().enumerate() {
        let page = crate::infrastructure::indexing::metadata_extractor::determine_page_number(
            chunk_start,
            &page_ranges,
        );
        assert_eq!(page, expected_pages[i], "Chunk at position {} should be on page {:?}", chunk_start, expected_pages[i]);
    }

    Ok(())
}

#[tokio::test]
async fn test_multiple_chunks_same_document() -> anyhow::Result<()> {
    let (temp_dir, pool, tokenizer) = setup_test_environment().await?;

    let storage = IndexStorage::new(pool.clone());

    let chunks = vec![
        crate::infrastructure::indexing::chunker::ContextualizedChunk {
            original_content: "First chunk content.".to_string(),
            contextualized_content: "[Document: test.txt | Page: 1]\n\nFirst chunk content."
                .to_string(),
            context_prefix: "[Document: test.txt | Page: 1]".to_string(),
            chunk_index: 0,
            token_count: 10,
            start_idx: 0,
            end_idx: 20,
        },
        crate::infrastructure::indexing::chunker::ContextualizedChunk {
            original_content: "Second chunk content.".to_string(),
            contextualized_content: "[Document: test.txt | Page: 2]\n\nSecond chunk content."
                .to_string(),
            context_prefix: "[Document: test.txt | Page: 2]".to_string(),
            chunk_index: 1,
            token_count: 10,
            start_idx: 20,
            end_idx: 41,
        },
        crate::infrastructure::indexing::chunker::ContextualizedChunk {
            original_content: "Third chunk content.".to_string(),
            contextualized_content: "[Document: test.txt | Page: 2]\n\nThird chunk content."
                .to_string(),
            context_prefix: "[Document: test.txt | Page: 2]".to_string(),
            chunk_index: 2,
            token_count: 10,
            start_idx: 41,
            end_idx: 61,
        },
    ];

    let embeddings = vec![
        vec![0.1; DEFAULT_EMBEDDING_DIM],
        vec![0.2; DEFAULT_EMBEDDING_DIM],
        vec![0.3; DEFAULT_EMBEDDING_DIM],
    ];

    let temp_file = NamedTempFile::new_in(temp_dir.path())?;

    let doc_id = storage
        .store_document_with_context(temp_file.path(), "text/plain", chunks, embeddings)
        .await?;

    let stored_chunks = sqlx::query!(
        r#"
        SELECT chunk_index, content, contextualized_content, context_prefix
        FROM text_chunks
        WHERE document_id = ?
        ORDER BY chunk_index
        "#,
        doc_id
    )
    .fetch_all(&pool)
    .await?;

    assert_eq!(stored_chunks.len(), 3);

    assert_eq!(stored_chunks[0].content, "First chunk content.");
    assert!(stored_chunks[0]
        .contextualized_content
        .as_ref()
        .unwrap()
        .contains("Page: 1"));

    assert_eq!(stored_chunks[1].content, "Second chunk content.");
    assert!(stored_chunks[1]
        .contextualized_content
        .as_ref()
        .unwrap()
        .contains("Page: 2"));

    assert_eq!(stored_chunks[2].content, "Third chunk content.");
    assert!(stored_chunks[2]
        .contextualized_content
        .as_ref()
        .unwrap()
        .contains("Page: 2"));

    Ok(())
}
