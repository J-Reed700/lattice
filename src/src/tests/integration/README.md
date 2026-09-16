# Integration Tests

Comprehensive integration tests for the Tauri Rust backend.

## Test Files

### test_end_to_end_indexing.rs

End-to-end tests for the complete document indexing workflow:

- Document ingestion and storage
- Content extraction
- Chunk generation
- Embedding creation
- Batch and concurrent indexing
- Performance benchmarks
- Edge cases and error handling

**Key Tests:**
- `test_single_document_indexing` - Basic document creation
- `test_large_document_chunking` - Handling large documents
- `test_concurrent_document_indexing` - Parallel indexing
- `test_indexing_performance` - Performance validation

### test_search_integration.rs

Comprehensive search functionality tests:

- Semantic search with embeddings
- Full-text search (FTS5)
- Hybrid search (semantic + keyword)
- Search ranking and relevance
- Query handling edge cases
- Performance benchmarks

**Key Tests:**
- `test_semantic_search_basic` - Basic semantic search
- `test_full_text_search` - FTS5 keyword search
- `test_hybrid_search_combination` - Combined search strategies
- `test_search_performance_many_documents` - Search scalability

### test_tag_integration.rs

Tag management and document-tag relationship tests:

- Tag creation and retrieval
- Document-tag associations
- Tag search and filtering
- Mention extraction and linking
- Backlinks and relationships
- Concurrent operations

**Key Tests:**
- `test_add_multiple_tags_to_document` - Tag associations
- `test_extract_person_mentions` - Mention parsing
- `test_mention_backlinks` - Relationship tracking
- `test_concurrent_tag_creation` - Parallel tag operations

## Test Helpers

All tests use the comprehensive test helper framework located in `tests/helpers/`:

### TestContext

Main test environment providing:
- In-memory SQLite database
- Mock embedding service
- All repository instances
- Automatic cleanup

**Usage:**
```rust
let ctx = TestContext::new().await?;
let doc = ctx.create_test_document("test.md", "content").await?;
```

### Factories

Builder-pattern factories for creating test data:
- `DocumentFactory` - Test documents
- `ChunkFactory` - Document chunks
- `EmbeddingFactory` - Embedding vectors
- `TagFactory` - Tags (via TestContext)

**Usage:**
```rust
let doc = DocumentFactory::new()
    .file_name("custom.md")
    .content("Custom content")
    .build();
```

### Mocks

Mock implementations for external dependencies:
- `MockEmbedder` - Deterministic embeddings
- `MockLLMClient` - Predefined LLM responses
- `MockSearchIndex` - In-memory search

**Usage:**
```rust
let embedder = MockEmbedder::new(384);
let embedding = embedder.embed_text("test").await?;
```

### Assertions

Domain-specific assertions for clearer tests:
- `assert_document_exists` - Document validation
- `assert_chunk_count` - Chunk verification
- `assert_tag_exists` - Tag validation
- `assert_embeddings_similar` - Similarity checking
- `assert_search_contains` - Search result validation

**Usage:**
```rust
assert_document_exists(&ctx.doc_repo(), &doc.id).await?;
assert_embeddings_similar(&emb1, &emb2, 0.9);
```

## Running Tests

Run all integration tests:
```bash
cargo test --test test_end_to_end_indexing
cargo test --test test_search_integration
cargo test --test test_tag_integration
```

Run specific test:
```bash
cargo test test_single_document_indexing
```

Run with output:
```bash
cargo test -- --nocapture
```

## Test Design Principles

1. **Isolated**: Each test has its own database
2. **Fast**: In-memory databases, mock services
3. **Deterministic**: Same inputs = same outputs
4. **Comprehensive**: Cover happy paths, edge cases, errors
5. **Realistic**: Simulate real-world scenarios

## Test Coverage Goals

- **Documents**: Creation, retrieval, updates, deletion
- **Chunks**: Generation, storage, retrieval
- **Embeddings**: Creation, similarity, search
- **Tags**: CRUD, associations, search
- **Mentions**: Extraction, types, backlinks
- **Search**: Semantic, keyword, hybrid, ranking
- **Performance**: Batch operations, concurrency
- **Edge Cases**: Empty inputs, special chars, errors

## Adding New Tests

1. Choose appropriate test file based on feature area
2. Use `TestContext` for setup
3. Use factories for test data
4. Use assertions for validation
5. Add documentation explaining what's being tested

**Example:**
```rust
#[tokio::test]
async fn test_new_feature() -> Result<()> {
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

## Performance Targets

- Single document indexing: < 100ms
- 100 documents indexing: < 5s
- Search (100 docs): < 1s
- Tag association: < 50ms
- Concurrent operations: No deadlocks

## Maintenance

- Keep tests fast and focused
- Update mocks when APIs change
- Document complex test scenarios
- Remove obsolete tests
- Maintain test coverage > 60%
