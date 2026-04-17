//! Comprehensive tests for Dependency Injection pattern
//!
//! This module demonstrates the DI pattern working with mocks, showing:
//! - Repository polymorphism with trait objects
//! - Service injection and mocking
//! - Complete workflows using MockAppContainer
//! - Testing without database or ML models

#[cfg(test)]
mod di_integration_tests {
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
    use crate::infrastructure::persistence::repositories::traits::*;
    use crate::infrastructure::services::traits::*;
    use crate::interfaces::di::MockAppContainer;

    /// Test complete document indexing workflow using mocks
    ///
    /// Demonstrates: Document creation -> Chunking -> Embedding -> Search
    #[tokio::test]
    async fn test_complete_indexing_workflow_with_mocks() -> Result<(), Box<dyn std::error::Error>>
    {
        let container = MockAppContainer::new();

        // Step 1: Create document
        let doc = container
            .documents()
            .create(
                "/docs/example.txt",
                "example.txt",
                "text/plain",
                2048,
                "2024-01-01T12:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .expect("Failed to create document");

        assert_eq!(doc.file_name(), "example.txt");
        use crate::domain::entities::document::DocumentStatus;
        assert_eq!(doc.status(), DocumentStatus::Indexed);

        // Step 2: Create chunks for the document
        use crate::domain::entities::chunk::Chunk;
        let chunks_data = vec![
            Chunk::new(doc.id().clone(), "First paragraph content".to_string(), 0),
            Chunk::new(doc.id().clone(), "Second paragraph content".to_string(), 1),
            Chunk::new(doc.id().clone(), "Third paragraph content".to_string(), 2),
        ];

        let chunk_ids = container
            .chunks()
            .create_batch(chunks_data)
            .await
            .expect("Failed to create chunks");

        assert_eq!(chunk_ids.len(), 3);

        // Step 3: Generate embeddings for chunks
        let chunk_texts = vec![
            "First paragraph content".to_string(),
            "Second paragraph content".to_string(),
            "Third paragraph content".to_string(),
        ];

        let embeddings = container
            .embedding_service()
            .embed_batch(&chunk_texts)
            .await?;

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), DEFAULT_EMBEDDING_DIM);

        // Step 4: Store embeddings
        use crate::features::embedding::entity::Embedding as EmbeddingEntity;
        let chunk_embeddings: Vec<(EmbeddingEntity, Vec<f32>)> = chunk_ids
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, emb)| {
                let embedding_metadata =
                    EmbeddingEntity::new(chunk.id().clone(), "mock-model".to_string(), emb.len());
                (embedding_metadata, emb.clone())
            })
            .collect();

        let embedding_ids = container
            .embeddings()
            .create_batch(chunk_embeddings)
            .await
            .expect("Failed to store embeddings");

        assert_eq!(embedding_ids.len(), 3);

        // Step 5: Verify retrieval
        let stored_chunks = container
            .chunks()
            .find_by_document(doc.id().as_str())
            .await
            .expect("Failed to retrieve chunks");

        assert_eq!(stored_chunks.len(), 3);
        assert_eq!(stored_chunks[0].content(), "First paragraph content");

        // Verify embeddings were stored
        for chunk in &chunk_ids {
            let stored_emb = container
                .embeddings()
                .find_by_chunk(chunk.id().as_str())
                .await
                .expect("Failed to find embedding");

            assert!(stored_emb.is_some());
            assert_eq!(stored_emb.unwrap().model_name(), "mock-model");
        }
        Ok(())
    }

    /// Test semantic search workflow with mocks
    ///
    /// Demonstrates: Adding to search index -> Querying -> Ranking results
    #[tokio::test]
    async fn test_search_workflow_with_mocks() -> Result<(), Box<dyn std::error::Error>> {
        let container = MockAppContainer::new();

        // Since MockSearchService is wrapped in Arc, we need to test differently
        // This demonstrates the limitation and workaround

        // Generate some test embeddings
        let texts = vec![
            "machine learning algorithms".to_string(),
            "deep neural networks".to_string(),
            "natural language processing".to_string(),
            "computer vision systems".to_string(),
        ];

        let embeddings = container.embedding_service().embed_batch(&texts).await?;

        // In a real scenario, these would be added to the search index
        // For now, we verify the embeddings are deterministic
        let embedding1_a = container.embedding_service().embed_single("test").await?;
        let embedding1_b = container.embedding_service().embed_single("test").await?;
        assert_eq!(
            embedding1_a, embedding1_b,
            "Embeddings should be deterministic"
        );

        let embedding2 = container
            .embedding_service()
            .embed_single("different")
            .await?;
        assert_ne!(
            embedding1_a, embedding2,
            "Different texts should have different embeddings"
        );
        Ok(())
    }

    /// Test tag management with dependency injection
    ///
    /// Demonstrates: Creating tags -> Associating with documents -> Querying
    #[tokio::test]
    async fn test_tag_workflow_with_di() {
        let container = MockAppContainer::new();

        // Create multiple documents
        let doc1 = container
            .documents()
            .create(
                "/docs/ml.txt",
                "ml.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"1".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        let doc2 = container
            .documents()
            .create(
                "/docs/ai.txt",
                "ai.txt",
                "text/plain",
                2048,
                "2024-01-02T00:00:00Z",
                &"2".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        let doc3 = container
            .documents()
            .create(
                "/docs/nlp.txt",
                "nlp.txt",
                "text/plain",
                1536,
                "2024-01-03T00:00:00Z",
                &"3".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        // Create tags
        let tag_ml = container
            .tags()
            .as_ref()
            .create_tag("machine-learning", Some("#3b82f6"))
            .await
            .unwrap();
        let tag_ai = container
            .tags()
            .as_ref()
            .create_tag("artificial-intelligence", Some("#8b5cf6"))
            .await
            .unwrap();
        let tag_nlp = container
            .tags()
            .as_ref()
            .create_tag("nlp", Some("#10b981"))
            .await
            .unwrap();

        // Associate tags with documents
        container
            .tags()
            .add_tag_to_document(doc1.id().as_str(), tag_ml.id().as_str())
            .await
            .unwrap();
        container
            .tags()
            .add_tag_to_document(doc1.id().as_str(), tag_ai.id().as_str())
            .await
            .unwrap();

        container
            .tags()
            .add_tag_to_document(doc2.id().as_str(), tag_ai.id().as_str())
            .await
            .unwrap();

        container
            .tags()
            .add_tag_to_document(doc3.id().as_str(), tag_ml.id().as_str())
            .await
            .unwrap();
        container
            .tags()
            .add_tag_to_document(doc3.id().as_str(), tag_nlp.id().as_str())
            .await
            .unwrap();

        // Query: Find all tags for doc1
        let doc1_tags = container
            .tags()
            .get_tags_for_document(doc1.id().as_str())
            .await
            .unwrap();
        assert_eq!(doc1_tags.len(), 2);
        assert!(doc1_tags
            .iter()
            .any(|t| t.name().as_str() == "machine-learning"));
        assert!(doc1_tags
            .iter()
            .any(|t| t.name().as_str() == "artificial-intelligence"));

        // Query: Find all documents with "machine-learning" tag
        let ml_docs = container
            .tags()
            .find_documents_by_tag(tag_ml.id().as_str())
            .await
            .unwrap();
        assert_eq!(ml_docs.len(), 2);
        assert!(ml_docs.contains(&doc1.id().to_string()));
        assert!(ml_docs.contains(&doc3.id().to_string()));

        // Query: Get all tags with counts
        let tags_with_counts = container.tags().get_all_with_counts().await.unwrap();
        assert_eq!(tags_with_counts.len(), 3);

        let ml_count = tags_with_counts
            .iter()
            .find(|t| t.0.name().as_str() == "machine-learning")
            .unwrap();
        assert_eq!(ml_count.1, 2);

        let ai_count = tags_with_counts
            .iter()
            .find(|t| t.0.name().as_str() == "artificial-intelligence")
            .unwrap();
        assert_eq!(ai_count.1, 2);

        let nlp_count = tags_with_counts
            .iter()
            .find(|t| t.0.name().as_str() == "nlp")
            .unwrap();
        assert_eq!(nlp_count.1, 1);
    }

    /// Test mention extraction and linking
    ///
    /// Demonstrates: Extracting mentions from text -> Storing -> Querying
    ///
    /// Fixed: Chunker now has minimum progress guarantee, iteration limit, and O(1) char lookup
    /// to prevent infinite loops on pathological inputs.
    #[tokio::test]
    async fn test_mention_workflow_with_di() {
        let container = MockAppContainer::new();

        // Create documents
        let doc1 = container
            .documents()
            .create(
                "/notes/meeting1.md",
                "meeting1.md",
                "text/markdown",
                512,
                "2024-01-15T10:00:00Z",
                &"1".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        let doc2 = container
            .documents()
            .create(
                "/notes/meeting2.md",
                "meeting2.md",
                "text/markdown",
                DEFAULT_EMBEDDING_DIM as i64,
                "2024-01-16T14:00:00Z",
                &"2".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        // Extract and store mentions
        let text1 = "Meeting with @[Alice Johnson] to discuss [[Project Alpha]] and [[Q1 Goals]].";
        let mentions1 = container
            .mentions()
            .extract_and_store_mentions(doc1.id().as_str(), text1)
            .await
            .unwrap();

        assert_eq!(mentions1.len(), 3);
        assert!(mentions1
            .iter()
            .any(|m| m.mention.name == "Alice Johnson" && m.mention.mention_type == "person"));
        assert!(mentions1
            .iter()
            .any(|m| m.mention.name == "Project Alpha" && m.mention.mention_type == "wikilink"));

        let text2 = "Follow-up with @[Alice Johnson] and @[Bob Smith] about [[Project Alpha]].";
        let mentions2 = container
            .mentions()
            .extract_and_store_mentions(doc2.id().as_str(), text2)
            .await
            .unwrap();

        assert_eq!(mentions2.len(), 3);

        // Query: Get all mentions in doc1
        let doc1_mentions = container
            .mentions()
            .get_mentions_for_document(doc1.id().as_str())
            .await
            .unwrap();
        assert_eq!(doc1_mentions.len(), 3);

        // Verify context was captured
        for mention in &doc1_mentions {
            assert!(mention.context.is_some());
            assert!(mention.position.is_some());
        }

        // Query: Find all documents mentioning "Alice Johnson"
        let alice_mention = container
            .mentions()
            .find_mention_by_name("Alice Johnson")
            .await
            .unwrap()
            .unwrap();
        let docs_with_alice = container
            .mentions()
            .get_documents_with_mention(alice_mention.id())
            .await
            .unwrap();

        assert_eq!(docs_with_alice.len(), 2);
        assert!(docs_with_alice.contains(&doc1.id().to_string()));
        assert!(docs_with_alice.contains(&doc2.id().to_string()));

        // Query: Search mentions
        let search_results = container
            .mentions()
            .search_mentions("Project", 10)
            .await
            .unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].name, "Project Alpha");
    }

    /// Test repository polymorphism with trait objects
    ///
    /// Demonstrates: Writing generic functions that accept any implementation
    // TODO: Rewrite for DDD ports - legacy test needs Document entity creation
    // Disabled due to trait incompatibility between DocumentRepositoryTrait and DocumentRepositoryPort
    // #[ignore]
    // #[tokio::test]
    // async fn test_repository_polymorphism() {
    //     use crate::application::ports::{RepositoryPort, DocumentRepositoryPort};
    //     use crate::domain::entities::Document;
    //
    //     // Generic function that works with any RepositoryPort<Document> implementation
    //     async fn count_documents<R: RepositoryPort<Document> + DocumentRepositoryPort>(repo: &R) -> usize {
    //         repo.count().await.unwrap_or(0)
    //     }
    //
    //     async fn create_test_doc<R: RepositoryPort<Document> + DocumentRepositoryPort>(repo: &R) -> String {
    //         let doc = repo
    //             .create(
    //                 "/test.txt",
    //                 "test.txt",
    //                 "text/plain",
    //                 100,
    //                 "2024-01-01T00:00:00Z",
    //                 "hash",
    //             )
    //             .await
    //             .expect("Failed to create document");
    //         doc.id()
    //     }
    //
    //     let container = MockAppContainer::new();
    //
    //     // Use generic functions with mock repository
    //     let initial_count = count_documents(container.documents().as_ref()).await;
    //     assert_eq!(initial_count, 0);
    //
    //     let doc_id = create_test_doc(container.documents().as_ref()).await;
    //     assert!(!doc_id.is_empty());
    //
    //     let final_count = count_documents(container.documents().as_ref()).await;
    //     assert_eq!(final_count, 1);
    //
    //     // Verify document was created
    //     let doc = container.documents().find_by_id(&doc_id).await.unwrap();
    //     assert!(doc.is_some());
    //     assert_eq!(doc.unwrap().file_name(), "test.txt");
    // }
    /// Test service polymorphism with trait objects
    ///
    /// Demonstrates: Generic embedding pipeline that works with any service
    #[tokio::test]
    async fn test_service_polymorphism() -> Result<(), Box<dyn std::error::Error>> {
        async fn embed_documents(
            service: &dyn EmbeddingServiceTrait,
            documents: &[String],
        ) -> Vec<Vec<f32>> {
            service
                .embed_batch(documents)
                .await
                .expect("Embedding failed")
        }

        let container = MockAppContainer::new();

        let docs = vec![
            "Document about machine learning".to_string(),
            "Document about deep learning".to_string(),
            "Document about neural networks".to_string(),
        ];

        let embeddings = embed_documents(container.embedding_service().as_ref(), &docs).await;

        assert_eq!(embeddings.len(), 3);
        for embedding in &embeddings {
            assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);
        }

        // Verify determinism
        let embeddings2 = embed_documents(container.embedding_service().as_ref(), &docs).await;
        assert_eq!(
            embeddings, embeddings2,
            "Embeddings should be deterministic"
        );
        Ok(())
    }

    /// Test full RAG pipeline using dependency injection
    ///
    /// Demonstrates: Complete Retrieval-Augmented Generation workflow
    #[tokio::test]
    async fn test_full_rag_pipeline_with_di() -> Result<(), Box<dyn std::error::Error>> {
        let container = MockAppContainer::new();

        // Setup: Create knowledge base
        let knowledge_base = [("What is machine learning?", "Machine learning is a subset of AI that enables systems to learn from data."),
            ("What is deep learning?", "Deep learning uses neural networks with multiple layers to learn complex patterns."),
            ("What is NLP?", "Natural Language Processing enables computers to understand human language.")];

        // Index documents
        let mut doc_ids = Vec::new();
        for (i, (title, content)) in knowledge_base.iter().enumerate() {
            let doc = container
                .documents()
                .create(
                    &format!("/kb/doc{}.txt", i),
                    &format!("doc{}.txt", i),
                    "text/plain",
                    content.len() as i64,
                    "2024-01-01T00:00:00Z",
                    &format!("{:0>64}", i), // Valid SHA-256 checksum (64 chars, zero-padded)
                )
                .await
                .unwrap();

            doc_ids.push(doc.id().clone());

            // Create chunk with content
            let chunk = container
                .chunks()
                .create(doc.id().as_str(), content, None, None, 0, None, None)
                .await
                .unwrap();

            // Generate and store embedding
            let embedding = container.embedding_service().embed_single(content).await?;

            container
                .embeddings()
                .create(chunk.id().as_str(), &embedding, "mock-model")
                .await?;
        }

        // RAG Query: "What is machine learning?"
        let query = "What is machine learning?";
        let query_embedding = container.embedding_service().embed_single(query).await?;

        // In a full implementation, we would:
        // 1. Search the index with query_embedding
        // 2. Retrieve top-k chunks
        // 3. Format context from chunks
        // 4. Generate answer using LLM

        // For this test, verify the pipeline components work
        assert_eq!(query_embedding.len(), DEFAULT_EMBEDDING_DIM);

        // Verify all documents were indexed
        let all_docs = container.documents().list_all().await.unwrap();
        assert_eq!(all_docs.len(), 3);

        // Verify all chunks were created
        let doc0_chunks = container
            .chunks()
            .find_by_document(doc_ids[0].as_str())
            .await
            .unwrap();
        assert_eq!(doc0_chunks.len(), 1);
        assert!(doc0_chunks[0].content().contains("Machine learning"));
        Ok(())
    }

    /// Test cleanup and isolation between tests
    ///
    /// Demonstrates: Mock container cleanup for test isolation
    #[tokio::test]
    async fn test_container_cleanup_and_isolation() {
        let container = MockAppContainer::new();

        // Add some data
        container
            .documents()
            .create(
                "/test1.txt",
                "test1.txt",
                "text/plain",
                100,
                "2024-01-01T00:00:00Z",
                &"1".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        container
            .tags()
            .as_ref()
            .create_tag("test-tag", None)
            .await
            .unwrap();

        let text = "Mention @[TestPerson] here";
        let doc = container
            .documents()
            .create(
                "/test2.txt",
                "test2.txt",
                "text/plain",
                100,
                "2024-01-01T00:00:00Z",
                &"2".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        container
            .mentions()
            .extract_and_store_mentions(doc.id().as_str(), text)
            .await
            .unwrap();

        // Verify data exists
        assert_eq!(container.documents().count().await.unwrap(), 2);
        assert_eq!(container.tags().get_all().await.unwrap().len(), 1);
        assert_eq!(container.embeddings().count().await.unwrap(), 0);

        // Clear all
        container.clear_all();

        // Verify data is cleared
        assert_eq!(container.documents().count().await.unwrap(), 0);
        assert_eq!(container.tags().get_all().await.unwrap().len(), 0);
        assert_eq!(container.embeddings().count().await.unwrap(), 0);
    }

    /// Test batch operations for performance
    ///
    /// Demonstrates: Efficient batch processing with DI
    #[tokio::test]
    async fn test_batch_operations_with_di() -> Result<(), Box<dyn std::error::Error>> {
        let container = MockAppContainer::new();

        // Create document
        let doc = container
            .documents()
            .create(
                "/large.txt",
                "large.txt",
                "text/plain",
                10000,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum
            )
            .await
            .unwrap();

        // Create 100 chunks in batch
        use crate::domain::entities::chunk::Chunk;
        let chunks_data: Vec<_> = (0..100)
            .map(|i| Chunk::new(doc.id().clone(), format!("Chunk {} content", i), i))
            .collect();

        let chunk_ids = container.chunks().create_batch(chunks_data).await.unwrap();

        assert_eq!(chunk_ids.len(), 100);

        // Generate embeddings in batch
        let texts: Vec<String> = (0..100).map(|i| format!("Chunk {} content", i)).collect();

        let embeddings = container.embedding_service().embed_batch(&texts).await?;

        assert_eq!(embeddings.len(), 100);

        // Store embeddings in batch
        use crate::features::embedding::entity::Embedding as EmbeddingEntity;
        let chunk_embeddings: Vec<(EmbeddingEntity, Vec<f32>)> = chunk_ids
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, emb)| {
                let embedding_metadata =
                    EmbeddingEntity::new(chunk.id().clone(), "mock-model".to_string(), emb.len());
                (embedding_metadata, emb.clone())
            })
            .collect();

        let embedding_ids = container
            .embeddings()
            .create_batch(chunk_embeddings)
            .await
            .unwrap();

        assert_eq!(embedding_ids.len(), 100);

        // Verify chunk count
        let chunk_count = container
            .chunks()
            .count_by_document(doc.id().as_str())
            .await
            .unwrap();
        assert_eq!(chunk_count, 100);
        Ok(())
    }
}
