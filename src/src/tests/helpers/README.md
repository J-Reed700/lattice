# Test Helpers

Comprehensive helper utilities for integration testing of the Tauri Rust backend.

## Overview

This module provides reusable test infrastructure for writing clean, maintainable integration tests.

### Components

```
helpers/
├── mod.rs           # Main module with TestContext
├── factories.rs     # Test data factories
├── mocks.rs        # Mock implementations
├── assertions.rs   # Custom assertions
└── README.md       # This file
```

## Core Components

### TestContext (mod.rs)

The main test environment providing everything needed for integration tests.

**Features:**
- In-memory SQLite database (fast, isolated)
- Mock embedding service (deterministic)
- All repository instances (doc, tag, chunk, etc.)
- Automatic cleanup on drop
- Database statistics
- Temp file management

**API:**
```rust
// Create context
let ctx = TestContext::new().await?;

// Custom embedding dimensions
let ctx = TestContext::with_embedding_dim(768).await?;

// Access repositories
let doc_repo = ctx.doc_repo();
let tag_repo = ctx.tag_repo();
let chunk_repo = ctx.chunk_repo();

// Create test data
let doc = ctx.create_test_document("test.md", "content").await?;
let chunks = ctx.create_test_chunks(&doc.id, 5).await?;
let tags = ctx.create_test_tags(&["tag1", "tag2"]).await?;

// Get stats
let stats = ctx.get_db_stats().await?;
assert_eq!(stats.documents, 1);

// Cleanup is automatic when ctx drops
```

### Factories (factories.rs)

Builder-pattern factories for creating test data with sensible defaults.

#### DocumentFactory

```rust
// With defaults
let doc = DocumentFactory::new().build();

// Customized
let doc = DocumentFactory::new()
    .file_name("custom.md")
    .file_type("markdown")
    .content("Custom content")
    .file_size(1024)
    .build();

// Insert into database
doc.insert_into_db(&doc_repo).await?;
```

#### ChunkFactory

```rust
// With defaults
let chunk = ChunkFactory::new().build();

// Customized
let chunk = ChunkFactory::new()
    .document_id("doc-123")
    .content("Chunk content")
    .chunk_index(0)
    .start_char(0)
    .end_char(100)
    .build();

// Insert into database
chunk.insert_into_db(&chunk_repo).await?;
```

#### EmbeddingFactory

```rust
// With defaults (384 dimensions)
let embedding = EmbeddingFactory::new().build();

// Custom dimensions and values
let embedding = EmbeddingFactory::new()
    .chunk_id("chunk-123")
    .embedding(vec![0.1, 0.2, 0.3])
    .build();

// Random embedding
let embedding = EmbeddingFactory::new()
    .random_embedding(768)
    .build();

// Insert into database
embedding.insert_into_db(&embedding_repo).await?;
```

#### Batch Factories

```rust
// Create multiple documents
let docs = create_test_document_batch(10);

// Create chunks for a document
let chunks = create_test_chunk_batch("doc-123", 5);

// Create embeddings for chunks
let embeddings = create_test_embedding_batch(&chunk_ids, 384);
```

#### Content Generators

```rust
// Markdown with mentions and tags
let content = generate_markdown_with_mentions(
    "alice",
    "project-x",
    &["important", "urgent"]
);

// Code snippets
let code = generate_code_snippet("rust", "simple");
let complex_code = generate_code_snippet("rust", "complex");
```

### Mocks (mocks.rs)

Mock implementations of external services for fast, deterministic testing.

#### MockEmbedder

```rust
// Deterministic embeddings (same input = same output)
let embedder = MockEmbedder::new(384);

let emb1 = embedder.embed_text("hello").await?;
let emb2 = embedder.embed_text("hello").await?;
assert_eq!(emb1, emb2); // Always equal

// Random embeddings
let embedder = MockEmbedder::new_random(384);

// Batch embedding
let texts = vec!["one", "two", "three"];
let embeddings = embedder.embed_batch(&texts).await?;

// Clear cache
embedder.clear_cache().await;
```

**How it works:**
- Uses deterministic hashing for consistent results
- No actual model loading (instant startup)
- Normalized unit vectors
- Thread-safe with internal caching

#### MockLLMClient

```rust
// Create client
let client = MockLLMClient::new();

// Set predefined responses
client.set_response("test prompt", "test response").await;

// Generate response
let response = client.generate("test prompt").await?;
assert_eq!(response, "test response");

// Unknown prompts return default
let default = client.generate("unknown").await?;
```

#### MockSearchIndex

```rust
// Create index
let index = MockSearchIndex::new();
let embedder = MockEmbedder::new(384);

// Add documents
let emb1 = embedder.embed_text("machine learning").await?;
index.add_document("doc1", "ML content", emb1).await;

// Search
let query_emb = embedder.embed_text("machine learning").await?;
let results = index.search(&query_emb, 10).await;

// Results are (doc_id, similarity_score) tuples, sorted by similarity
```

### Assertions (assertions.rs)

Domain-specific assertions with clear error messages.

#### Vector/Embedding Assertions

```rust
// Similarity check
assert_embeddings_similar(&emb1, &emb2, 0.9);

// Difference check
assert_embeddings_different(&emb1, &emb2, 0.5);

// Dimension check
assert_embedding_dimensions(&embedding, 384);

// Normalization check
assert_embedding_normalized(&embedding);
```

#### Document Assertions

```rust
// Existence checks
assert_document_exists(&repo, "doc-123").await?;
assert_document_not_exists(&repo, "doc-456").await?;

// Chunk count
assert_chunk_count(&repo, "doc-123", 5).await?;

// Batch checks
assert_documents_exist(&repo, &["doc1", "doc2"]).await?;
```

#### Tag Assertions

```rust
// Tag exists
assert_tag_exists(&repo, "important").await?;

// Document has tags
assert_document_has_tags(&repo, "doc-123", &["tag1", "tag2"]).await?;

// Tag on documents
assert_tag_on_documents(&repo, "important", &["doc1", "doc2"]).await?;

// Batch checks
assert_tags_exist(&repo, &["tag1", "tag2"]).await?;
```

#### Mention Assertions

```rust
// Mention exists
assert_mention_exists(&repo, "alice-smith").await?;

// Mention type
assert_mention_type(&repo, "alice-smith", "person").await?;

// Document has mentions
assert_document_has_mentions(
    &repo,
    "doc-123",
    &["alice", "project-x"]
).await?;

// Batch checks
assert_mentions_exist(&repo, &["alice", "bob"]).await?;
```

#### Search Result Assertions

```rust
// Contains specific IDs
assert_search_contains(&results, &["doc1", "doc3"]);

// Specific order
assert_search_order(&results, &["doc1", "doc2", "doc3"]);

// Result count bounds
assert_min_results(&results, 5);
assert_max_results(&results, 20);
```

## Usage Patterns

### Basic Test Structure

```rust
#[tokio::test]
async fn test_feature() -> Result<()> {
    // Arrange
    let ctx = TestContext::new().await?;
    let doc = ctx.create_test_document("test.md", "content").await?;

    // Act
    let result = perform_operation(&doc).await?;

    // Assert
    assert_eq!(result.status, "success");

    Ok(())
}
```

### Testing with Embeddings

```rust
#[tokio::test]
async fn test_semantic_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create document with embedding
    let doc = ctx.create_test_document("ml.md", "machine learning").await?;
    let chunks = ctx.create_test_chunks(&doc.id, 1).await?;

    let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
    let embeddings = ctx.create_test_embeddings(&chunk_ids).await?;

    // Query
    let query_emb = ctx.embedder.embed_text("machine learning").await?;

    // Verify similarity
    assert_embeddings_similar(&query_emb, &embeddings[0].embedding, 0.8);

    Ok(())
}
```

### Testing Tag Relationships

```rust
#[tokio::test]
async fn test_tag_associations() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("doc1.md", "content").await?;
    let doc2 = ctx.create_test_document("doc2.md", "content").await?;

    let tags = ctx.create_test_tags(&["shared-tag"]).await?;
    let tag_repo = ctx.tag_repo();

    // Associate tags
    tag_repo.add_tag_to_document(&doc1.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc2.id, &tags[0].id).await?;

    // Verify
    assert_tag_on_documents(&tag_repo, "shared-tag", &[&doc1.id, &doc2.id]).await?;

    Ok(())
}
```

### Testing Concurrent Operations

```rust
#[tokio::test]
async fn test_concurrent_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let doc_repo = ctx.doc_repo();
            tokio::spawn(async move {
                let doc = DocumentFactory::new()
                    .file_name(&format!("doc-{}.md", i))
                    .build();
                doc.insert_into_db(&doc_repo).await
            })
        })
        .collect();

    let results = futures::future::join_all(handles).await;

    for result in results {
        assert!(result.is_ok());
    }

    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 10);

    Ok(())
}
```

## Best Practices

### 1. Use TestContext for Setup

Always use `TestContext` instead of manual setup:

```rust
// ✅ Good
let ctx = TestContext::new().await?;
let doc = ctx.create_test_document("test.md", "content").await?;

// ❌ Avoid
let pool = setup_db().await?;
let repo = DocumentRepository::new(pool);
// Manual document creation...
```

### 2. Use Factories for Test Data

Use factories instead of manual construction:

```rust
// ✅ Good
let doc = DocumentFactory::new()
    .file_name("test.md")
    .content("Test content")
    .build();

// ❌ Avoid
let doc = TestDocument {
    id: Uuid::new_v4().to_string(),
    vault_id: "test-lattice".to_string(),
    // ... many more fields
};
```

### 3. Use Domain Assertions

Use domain-specific assertions for clarity:

```rust
// ✅ Good
assert_document_exists(&repo, &doc.id).await?;

// ❌ Less clear
let result = repo.get_by_id(&doc.id).await?;
assert!(result.is_some());
```

### 4. Keep Tests Focused

Each test should verify one thing:

```rust
// ✅ Good - focused test
#[tokio::test]
async fn test_document_creation() -> Result<()> {
    let ctx = TestContext::new().await?;
    let doc = ctx.create_test_document("test.md", "content").await?;
    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;
    Ok(())
}

// ❌ Avoid - testing too much
#[tokio::test]
async fn test_everything() -> Result<()> {
    // Creates docs, chunks, embeddings, tags, searches...
    // Hard to debug when it fails
}
```

### 5. Use Descriptive Test Names

```rust
// ✅ Good
test_empty_document_indexing
test_concurrent_tag_creation
test_semantic_search_with_similar_documents

// ❌ Avoid
test_1
test_docs
test_it_works
```

## Extending the Helpers

### Adding New Factories

```rust
// 1. Define test model
pub struct TestEntity {
    pub id: String,
    pub name: String,
}

// 2. Create factory
pub struct EntityFactory {
    id: String,
    name: String,
}

impl EntityFactory {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "default-name".to_string(),
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn build(self) -> TestEntity {
        TestEntity {
            id: self.id,
            name: self.name,
        }
    }
}

// 3. Add to TestContext if needed
impl TestContext {
    pub async fn create_test_entity(&self, name: &str) -> Result<TestEntity> {
        let entity = EntityFactory::new().name(name).build();
        // Insert into DB...
        Ok(entity)
    }
}
```

### Adding New Assertions

```rust
// In assertions.rs

/// Assert entity has expected property
pub async fn assert_entity_property(
    repo: &EntityRepository,
    entity_id: &str,
    expected_value: &str,
) -> Result<()> {
    let entity = repo.get_by_id(entity_id).await?;

    assert!(
        entity.is_some(),
        "Entity not found: {}",
        entity_id
    );

    let entity = entity.unwrap();
    assert_eq!(
        entity.property,
        expected_value,
        "Wrong property value: {} != {}",
        entity.property,
        expected_value
    );

    Ok(())
}
```

### Adding New Mocks

```rust
// In mocks.rs

/// Mock service for testing
pub struct MockService {
    responses: Arc<Mutex<HashMap<String, String>>>,
}

impl MockService {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn set_response(&self, key: &str, value: &str) {
        let mut responses = self.responses.lock().await;
        responses.insert(key.to_string(), value.to_string());
    }

    pub async fn get_response(&self, key: &str) -> Option<String> {
        let responses = self.responses.lock().await;
        responses.get(key).cloned()
    }
}
```

## Troubleshooting

### Tests are slow

- Ensure using in-memory database (`:memory:` or temp file)
- Check for blocking operations in async context
- Use mocks instead of real services
- Run tests in parallel: `cargo test -- --test-threads=4`

### Tests are flaky

- Check for race conditions in concurrent tests
- Ensure proper cleanup between tests
- Use deterministic mocks (not random)
- Verify test isolation

### Database errors

- Ensure schema is initialized: `initialize_database(&pool)`
- Check connection string format
- Verify temp directory permissions
- Look for foreign key violations

### Import errors

```rust
// In test file, add:
#[cfg(test)]
mod helpers {
    pub use crate::helpers::*;
}
```

## Performance Tips

1. **Reuse TestContext when possible** (within same test)
2. **Use batch operations** for creating many entities
3. **Mock expensive operations** (embedding generation, LLM calls)
4. **Limit database queries** in assertions
5. **Run tests in parallel** for faster CI/CD

## Further Reading

- [Rust Testing Best Practices](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [SQLx Testing Guide](https://github.com/launchbadge/sqlx#testing)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)
