# Integration Test Guide

Complete guide to the Tauri backend integration test infrastructure.

## Quick Start

```bash
# Run all integration tests
cd vault/desktop/src-tauri
cargo test --test test_end_to_end_indexing
cargo test --test test_search_integration
cargo test --test test_tag_integration

# Run specific test
cargo test test_single_document_indexing

# Run with output
cargo test -- --nocapture

# Run in parallel
cargo test -- --test-threads=8
```

## Architecture Overview

```
tests/
├── helpers/                    # Reusable test infrastructure
│   ├── mod.rs                 # TestContext + main helpers
│   ├── factories.rs           # Test data builders
│   ├── mocks.rs              # Mock implementations
│   ├── assertions.rs         # Custom assertions
│   └── README.md             # Detailed helper docs
│
├── integration/               # Integration test suites
│   ├── test_end_to_end_indexing.rs   # Document indexing tests
│   ├── test_search_integration.rs    # Search functionality tests
│   ├── test_tag_integration.rs       # Tag/mention tests
│   └── README.md                      # Integration test docs
│
├── fixtures/                  # Test data fixtures
├── mocks/                     # Additional mocks
└── INTEGRATION_TEST_GUIDE.md # This file
```

## Test Infrastructure

### TestContext - Your Test Foundation

`TestContext` is the core of the test infrastructure. It provides:

```rust
use helpers::TestContext;

#[tokio::test]
async fn example_test() -> Result<()> {
    // 1. Create isolated test environment
    let ctx = TestContext::new().await?;

    // 2. Access all repositories
    let doc_repo = ctx.doc_repo();
    let tag_repo = ctx.tag_repo();
    let chunk_repo = ctx.chunk_repo();
    let embedding_repo = ctx.embedding_repo();
    let mention_repo = ctx.mention_repo();

    // 3. Create test data easily
    let doc = ctx.create_test_document("test.md", "content").await?;
    let chunks = ctx.create_test_chunks(&doc.id, 5).await?;
    let tags = ctx.create_test_tags(&["important", "urgent"]).await?;

    // 4. Access mock embedder
    let embedding = ctx.embedder.embed_text("test").await?;

    // 5. Get stats
    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 1);

    // 6. Cleanup happens automatically when ctx drops
    Ok(())
}
```

**Key Features:**
- ✅ In-memory SQLite (fast, isolated, no cleanup needed)
- ✅ Mock embedder (deterministic, no model loading)
- ✅ All repository instances ready to use
- ✅ Automatic resource cleanup
- ✅ Database statistics
- ✅ Temp file management

### Test Data Factories

Build test data with sensible defaults and easy customization:

```rust
use helpers::{DocumentFactory, ChunkFactory, EmbeddingFactory};

// Documents
let doc = DocumentFactory::new()
    .file_name("custom.md")
    .file_type("markdown")
    .content("Custom content")
    .build();

doc.insert_into_db(&doc_repo).await?;

// Chunks
let chunk = ChunkFactory::new()
    .document_id(&doc.id)
    .content("Chunk content")
    .chunk_index(0)
    .build();

chunk.insert_into_db(&chunk_repo).await?;

// Embeddings
let embedding = EmbeddingFactory::new()
    .chunk_id(&chunk.id)
    .random_embedding(384)
    .build();

embedding.insert_into_db(&embedding_repo).await?;

// Batch creation
let docs = create_test_document_batch(10);
let chunks = create_test_chunk_batch("doc-id", 5);
```

### Mock Services

Fast, deterministic mocks for external dependencies:

```rust
use helpers::{MockEmbedder, MockLLMClient, MockSearchIndex};

// Mock embedder (deterministic)
let embedder = MockEmbedder::new(384);
let emb1 = embedder.embed_text("hello").await?;
let emb2 = embedder.embed_text("hello").await?;
assert_eq!(emb1, emb2); // Always equal

// Mock LLM
let llm = MockLLMClient::new();
llm.set_response("prompt", "response").await;
let response = llm.generate("prompt").await?;

// Mock search index
let index = MockSearchIndex::new();
index.add_document("doc1", "content", embedding).await;
let results = index.search(&query_embedding, 10).await;
```

### Custom Assertions

Domain-specific assertions for clearer tests:

```rust
use helpers::*;

// Documents
assert_document_exists(&doc_repo, &doc.id).await?;
assert_chunk_count(&chunk_repo, &doc.id, 5).await?;

// Tags
assert_tag_exists(&tag_repo, "important").await?;
assert_document_has_tags(&tag_repo, &doc.id, &["tag1", "tag2"]).await?;

// Mentions
assert_mention_exists(&mention_repo, "alice").await?;
assert_mention_type(&mention_repo, "alice", "person").await?;

// Embeddings
assert_embeddings_similar(&emb1, &emb2, 0.9);
assert_embedding_dimensions(&embedding, 384);
assert_embedding_normalized(&embedding);

// Search
assert_search_contains(&results, &["doc1", "doc3"]);
assert_min_results(&results, 5);
```

## Integration Test Suites

### 1. End-to-End Indexing Tests

**File:** `integration/test_end_to_end_indexing.rs`

Tests the complete document indexing pipeline:

**Coverage:**
- ✅ Single document indexing
- ✅ Document with chunks
- ✅ Document with embeddings
- ✅ Large document chunking
- ✅ Markdown/code chunking
- ✅ Batch indexing
- ✅ Concurrent indexing
- ✅ Edge cases (empty, very long, special chars)
- ✅ Performance benchmarks

**Example Test:**
```rust
#[tokio::test]
async fn test_document_with_chunks_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;
    let doc = ctx.create_test_document("test.md", "Test content").await?;
    let chunks = ctx.create_test_chunks(&doc.id, 3).await?;

    assert_chunk_count(&ctx.chunk_repo(), &doc.id, 3).await?;

    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i as i32);
        assert!(!chunk.content.is_empty());
    }

    Ok(())
}
```

### 2. Search Integration Tests

**File:** `integration/test_search_integration.rs`

Tests semantic and hybrid search functionality:

**Coverage:**
- ✅ Semantic search with embeddings
- ✅ Full-text search (FTS5)
- ✅ Hybrid search (semantic + keyword)
- ✅ Search ranking
- ✅ Query edge cases
- ✅ Unicode and special characters
- ✅ Case sensitivity
- ✅ Performance benchmarks

**Example Test:**
```rust
#[tokio::test]
async fn test_full_text_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "rust.md",
        "Rust programming language with memory safety"
    ).await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("Rust").await?;

    assert!(!results.is_empty());
    assert!(results.contains(&doc.id));

    Ok(())
}
```

### 3. Tag Integration Tests

**File:** `integration/test_tag_integration.rs`

Tests tag management and document-tag relationships:

**Coverage:**
- ✅ Tag creation and retrieval
- ✅ Document-tag associations
- ✅ Tag search and filtering
- ✅ Person mentions (`@alice`)
- ✅ Wikilink mentions (`[[topic]]`)
- ✅ Backlinks and relationships
- ✅ Concurrent operations
- ✅ Tag autocomplete

**Example Test:**
```rust
#[tokio::test]
async fn test_extract_person_mentions() -> Result<()> {
    let ctx = TestContext::new().await?;
    let doc = ctx.create_test_document("mentions.md", "Test").await?;

    let content = "Meeting with @alice-smith and @bob-jones.";

    let mention_repo = ctx.mention_repo();
    let mentions = mention_repo
        .extract_and_store_mentions(&doc.id, content)
        .await?;

    let person_mentions: Vec<_> = mentions
        .iter()
        .filter(|m| m.mention.mention_type == "person")
        .collect();

    assert_eq!(person_mentions.len(), 2);
    assert_mention_exists(&mention_repo, "alice-smith").await?;

    Ok(())
}
```

## Common Patterns

### Pattern 1: Testing Document Creation

```rust
#[tokio::test]
async fn test_document_creation() -> Result<()> {
    // Arrange
    let ctx = TestContext::new().await?;

    // Act
    let doc = ctx.create_test_document("test.md", "content").await?;

    // Assert
    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;

    Ok(())
}
```

### Pattern 2: Testing Search

```rust
#[tokio::test]
async fn test_search_functionality() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create searchable documents
    let doc1 = ctx.create_test_document("ml.md", "machine learning").await?;
    let doc2 = ctx.create_test_document("ai.md", "artificial intelligence").await?;

    // Search
    let results = ctx.doc_repo().search_documents("machine").await?;

    // Verify
    assert_search_contains(&results, &[&doc1.id]);

    Ok(())
}
```

### Pattern 3: Testing with Embeddings

```rust
#[tokio::test]
async fn test_semantic_similarity() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Generate embeddings
    let emb1 = ctx.embedder.embed_text("machine learning").await?;
    let emb2 = ctx.embedder.embed_text("machine learning").await?;
    let emb3 = ctx.embedder.embed_text("cooking recipes").await?;

    // Verify similarity
    assert_embeddings_similar(&emb1, &emb2, 0.99);
    assert_embeddings_different(&emb1, &emb3, 0.5);

    Ok(())
}
```

### Pattern 4: Testing Concurrent Operations

```rust
#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Spawn concurrent tasks
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

    // Wait for completion
    let results = futures::future::join_all(handles).await;

    // Verify all succeeded
    for result in results {
        assert!(result.is_ok());
    }

    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 10);

    Ok(())
}
```

### Pattern 5: Testing Tag Relationships

```rust
#[tokio::test]
async fn test_tag_relationships() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("doc1.md", "content").await?;
    let doc2 = ctx.create_test_document("doc2.md", "content").await?;

    let tags = ctx.create_test_tags(&["shared-tag"]).await?;
    let tag_repo = ctx.tag_repo();

    // Create relationships
    tag_repo.add_tag_to_document(&doc1.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc2.id, &tags[0].id).await?;

    // Verify bidirectional relationship
    assert_document_has_tags(&tag_repo, &doc1.id, &["shared-tag"]).await?;
    assert_tag_on_documents(&tag_repo, "shared-tag", &[&doc1.id, &doc2.id]).await?;

    Ok(())
}
```

## Best Practices

### ✅ DO

1. **Use TestContext for all tests**
   ```rust
   let ctx = TestContext::new().await?;
   ```

2. **Use factories for test data**
   ```rust
   let doc = DocumentFactory::new().file_name("test.md").build();
   ```

3. **Use domain assertions**
   ```rust
   assert_document_exists(&repo, &doc.id).await?;
   ```

4. **Keep tests focused and isolated**
   ```rust
   // One test = one thing
   ```

5. **Use descriptive test names**
   ```rust
   test_concurrent_document_indexing_handles_race_conditions
   ```

6. **Document complex test scenarios**
   ```rust
   /// Tests that concurrent tag associations don't cause duplicates
   #[tokio::test]
   async fn test_concurrent_tag_associations() { ... }
   ```

### ❌ DON'T

1. **Don't use real databases or services**
   ```rust
   // ❌ Bad
   let pool = connect_to_prod_db().await?;
   ```

2. **Don't create test data manually**
   ```rust
   // ❌ Bad
   let doc = Document { id: "123", ... }; // Too verbose
   ```

3. **Don't test multiple things in one test**
   ```rust
   // ❌ Bad
   test_everything_at_once() // Hard to debug
   ```

4. **Don't rely on test execution order**
   ```rust
   // ❌ Bad - tests must be independent
   ```

5. **Don't ignore cleanup**
   ```rust
   // ❌ Bad - TestContext handles this
   // Manual cleanup is unnecessary
   ```

## Troubleshooting

### "models module not found"

**Solution:** The models.rs stub should exist. If not:
```bash
echo 'pub use crate::domain_types::*;' > src/models.rs
```

### "Database locked" errors

**Cause:** SQLite doesn't handle concurrent writes well.

**Solution:** Use separate TestContext per test, or serialize writes.

### Tests are slow

**Checklist:**
- ✅ Using in-memory database?
- ✅ Using mock embedder (not real model)?
- ✅ Running tests in parallel?
- ✅ Not loading external resources?

### Flaky tests

**Common causes:**
- Race conditions in concurrent tests
- Non-deterministic mocks (use `MockEmbedder::new()`, not `new_random()`)
- Shared state between tests
- Timing dependencies

**Solution:** Ensure each test is fully isolated.

### Import errors in tests

**Solution:** Add to test file:
```rust
#[cfg(test)]
mod helpers {
    pub use crate::helpers::*;
}
```

## Performance Guidelines

| Operation | Target | Notes |
|-----------|--------|-------|
| Single doc indexing | < 100ms | In-memory DB |
| 100 docs indexing | < 5s | Batch operations |
| Search (100 docs) | < 1s | With embeddings |
| Tag association | < 50ms | Single operation |
| Concurrent ops | No deadlocks | Test with 10+ threads |

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Run integration tests
  run: |
    cd vault/desktop/src-tauri
    cargo test --test test_end_to_end_indexing -- --test-threads=8
    cargo test --test test_search_integration -- --test-threads=8
    cargo test --test test_tag_integration -- --test-threads=8
```

### Coverage Report

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --test test_end_to_end_indexing --out Html
```

## Extending the Tests

### Adding a New Integration Test File

1. Create file: `tests/integration/test_new_feature.rs`

2. Add helper import:
```rust
mod helpers {
    pub use crate::helpers::*;
}

use helpers::*;
```

3. Write tests using TestContext:
```rust
#[tokio::test]
async fn test_new_feature() -> Result<()> {
    let ctx = TestContext::new().await?;
    // Test implementation
    Ok(())
}
```

### Adding a New Factory

See `helpers/README.md` for detailed instructions on extending factories, mocks, and assertions.

## Resources

- **Helpers Documentation:** `tests/helpers/README.md`
- **Integration Tests:** `tests/integration/README.md`
- **Rust Testing Book:** https://doc.rust-lang.org/book/ch11-00-testing.html
- **Tokio Testing:** https://tokio.rs/tokio/topics/testing
- **SQLx Testing:** https://github.com/launchbadge/sqlx#testing

## Summary

The integration test infrastructure provides:

✅ **Fast**: In-memory database, mock services
✅ **Isolated**: Each test gets fresh environment
✅ **Comprehensive**: Covers indexing, search, tags, mentions
✅ **Easy to use**: Simple API, clear patterns
✅ **Well documented**: This guide + inline docs
✅ **Maintainable**: Reusable helpers, DRY principles

**Get started now:**

```rust
#[tokio::test]
async fn my_first_test() -> Result<()> {
    let ctx = TestContext::new().await?;
    let doc = ctx.create_test_document("test.md", "Hello, testing!").await?;
    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;
    Ok(())
}
```

Happy testing! 🚀
