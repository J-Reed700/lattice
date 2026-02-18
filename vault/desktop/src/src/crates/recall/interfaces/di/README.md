# Dependency Injection Architecture

This module implements a comprehensive dependency injection (DI) system for the Recall application, following the "bricks and studs" philosophy from the zen-architect specification.

## Overview

The DI architecture provides:
- **Trait-based abstractions** for repositories and services
- **Production implementations** backed by SQLite and ONNX models
- **Mock implementations** for fast, isolated unit testing
- **Centralized container** managing dependency lifecycle

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application Layer                    │
│              (Commands, Event Handlers)                 │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│                   DI Container                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │  AppContainer (Production)                       │  │
│  │  - Real SQLite repositories                      │  │
│  │  - ONNX embedding service                        │  │
│  │  - Vector search service                         │  │
│  └──────────────────────────────────────────────────┘  │
│                                                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │  MockAppContainer (Testing)                      │  │
│  │  - In-memory mock repositories                   │  │
│  │  - Deterministic mock embedding service          │  │
│  │  - In-memory mock search service                 │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│                 Trait Interfaces                         │
│  - DocumentRepositoryTrait                               │
│  - ChunkRepositoryTrait                                  │
│  - EmbeddingRepositoryTrait                              │
│  - TagRepositoryTrait                                    │
│  - MentionRepositoryTrait                                │
│  - EmbeddingServiceTrait                                 │
│  - SearchServiceTrait                                    │
└────────────────────┬────────────────────────────────────┘
                     │
        ┌────────────┴────────────┐
        ▼                         ▼
┌──────────────────┐    ┌──────────────────┐
│   Production     │    │      Mocks       │
│ Implementations  │    │  Implementations │
│                  │    │                  │
│ - SQLite-backed  │    │ - HashMap-backed │
│ - ONNX models    │    │ - Deterministic  │
│ - Real search    │    │ - No I/O         │
└──────────────────┘    └──────────────────┘
```

## Components

### 1. Repository Traits (`repositories/traits.rs`)

Defines the contract for data access:

```rust
#[async_trait]
pub trait DocumentRepositoryTrait: Send + Sync {
    async fn create(&self, ...) -> Result<Document>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>>;
    async fn list_all(&self) -> Result<Vec<Document>>;
    // ... more methods
}
```

**Available Traits:**
- `DocumentRepositoryTrait` - Document storage
- `ChunkRepositoryTrait` - Text chunk management
- `EmbeddingRepositoryTrait` - Vector embedding storage
- `TagRepositoryTrait` - Tag and tagging operations
- `MentionRepositoryTrait` - Mention extraction and linking

### 2. Mock Implementations (`repositories/mocks.rs`)

In-memory implementations for testing:

```rust
pub struct MockDocumentRepository {
    documents: Arc<Mutex<HashMap<String, Document>>>,
    path_index: Arc<Mutex<HashMap<String, String>>>,
}
```

**Features:**
- Thread-safe with `Arc<Mutex>`
- Deterministic behavior
- Fast (no I/O)
- Clearable for test isolation

**Available Mocks:**
- `MockDocumentRepository`
- `MockChunkRepository`
- `MockEmbeddingRepository`
- `MockTagRepository`
- `MockMentionRepository`

### 3. Service Traits (`services/traits.rs`)

Defines the contract for business logic:

```rust
#[async_trait]
pub trait EmbeddingServiceTrait: Send + Sync {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

#[async_trait]
pub trait SearchServiceTrait: Send + Sync {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<SearchResult>;
    async fn search_with_metadata(&self, ...) -> Result<Vec<SearchResult>>;
}
```

**Mock Services:**
- `MockEmbeddingService` - Deterministic hash-based embeddings
- `MockSearchService` - In-memory brute-force search

### 4. DI Container (`di/mod.rs`)

Central dependency management:

```rust
// Production container
pub struct AppContainer {
    document_repo: Arc<DocumentRepository>,
    chunk_repo: Arc<ChunkRepository>,
    // ... more dependencies
    embedding_service: Option<Arc<EmbeddingService>>,
    search_service: Option<Arc<dyn SearchServiceTrait>>,
}

// Mock container
pub struct MockAppContainer {
    document_repo: Arc<MockDocumentRepository>,
    chunk_repo: Arc<MockChunkRepository>,
    // ... more mocks
    embedding_service: Arc<MockEmbeddingService>,
    search_service: Arc<MockSearchService>,
}
```

## Usage Examples

### Production Usage

```rust
use crate::interfaces::di::AppContainer;

async fn index_document(pool: SqlitePool, file_path: &str) -> Result<()> {
    // Create container with production implementations
    let container = AppContainer::new(pool).await?;

    // Use repositories
    let doc = container.documents()
        .create(file_path, "file.txt", "text/plain", 1024, "2024-01-01", "hash")
        .await?;

    // Create chunks
    let chunk = container.chunks()
        .create(&doc.id, "content", None, None, 0, None, None)
        .await?;

    Ok(())
}
```

### Testing Usage

```rust
use crate::interfaces::di::MockAppContainer;

#[tokio::test]
async fn test_document_indexing() {
    // Create container with mock implementations
    let container = MockAppContainer::new();

    // Create document (no database required!)
    let doc = container.documents()
        .create("/test.txt", "test.txt", "text/plain", 100, "2024-01-01", "hash")
        .await
        .unwrap();

    // Verify
    assert_eq!(doc.file_name, "test.txt");
    assert_eq!(doc.status, "indexed");

    // Find it back
    let found = container.documents()
        .find_by_id(&doc.id)
        .await
        .unwrap();
    assert!(found.is_some());
}
```

### Polymorphic Functions

Write functions that work with any implementation:

```rust
async fn count_documents(repo: &dyn DocumentRepositoryTrait) -> i64 {
    repo.count().await.unwrap_or(0)
}

// Works with production
let prod_count = count_documents(prod_container.documents().as_ref()).await;

// Works with mocks
let test_count = count_documents(mock_container.documents().as_ref()).await;
```

## Benefits

### 1. Testability

**Without DI:**
```rust
#[tokio::test]
async fn test_indexing() {
    // Need to set up:
    // - SQLite database
    // - Run migrations
    // - Download ONNX model
    // - Initialize embedding service
    // ... slow and complex
}
```

**With DI:**
```rust
#[tokio::test]
async fn test_indexing() {
    let container = MockAppContainer::new();
    // Fast, isolated, no setup needed
}
```

### 2. Flexibility

Swap implementations without changing code:

```rust
// Development: Use mocks
let container = MockAppContainer::new();

// Production: Use real database
let container = AppContainer::new(pool).await?;

// Testing: Use different mock behavior
use vault_desktop::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
let container = MockAppContainer::with_dimension(DEFAULT_EMBEDDING_DIM);
```

### 3. Decoupling

Depend on traits, not concrete types:

```rust
// Good: Depends on trait
async fn process(repo: &dyn DocumentRepositoryTrait) { ... }

// Bad: Depends on concrete type
async fn process(repo: &DocumentRepository) { ... }
```

### 4. Maintainability

Clear separation of concerns:
- **Repositories** = Data access
- **Services** = Business logic
- **Container** = Wiring and lifecycle
- **Commands** = User-facing operations

## Test Examples

Comprehensive tests are available in `di/tests.rs`:

1. **Complete Indexing Workflow** - Document → Chunks → Embeddings
2. **Tag Management** - Creating, associating, querying tags
3. **Mention Extraction** - Parsing, storing, linking mentions
4. **Repository Polymorphism** - Generic functions with trait objects
5. **Service Polymorphism** - Swappable service implementations
6. **Full RAG Pipeline** - Complete retrieval-augmented generation
7. **Batch Operations** - Efficient bulk processing
8. **Test Isolation** - Cleanup and reset between tests

Run tests:
```bash
cargo test --package vault-desktop-tauri di::tests
```

## Design Patterns

### Bricks and Studs Philosophy

- **Bricks** = Self-contained modules (repositories, services)
- **Studs** = Public interfaces (traits)
- **Regeneratable** = Can rebuild implementations without breaking dependents

### Repository Pattern

Data access abstraction:
```
Commands → Services → Repositories → Database
```

### Service Layer Pattern

Business logic separation:
```
API/Commands → Services → Multiple Repositories
```

### Dependency Inversion Principle

High-level modules depend on abstractions (traits), not concrete implementations:
```
Commands → Traits ← (Implementations | Mocks)
```

## Adding New Dependencies

### 1. Define Trait

```rust
// repositories/traits.rs
#[async_trait]
pub trait MyRepositoryTrait: Send + Sync {
    async fn my_method(&self, param: &str) -> Result<MyData>;
}
```

### 2. Implement Production Version

```rust
// repositories/my_repository.rs
pub struct MyRepository {
    pool: SqlitePool,
}

#[async_trait]
impl MyRepositoryTrait for MyRepository {
    async fn my_method(&self, param: &str) -> Result<MyData> {
        // SQLite implementation
    }
}
```

### 3. Implement Mock Version

```rust
// repositories/mocks.rs
pub struct MockMyRepository {
    data: Arc<Mutex<HashMap<String, MyData>>>,
}

#[async_trait]
impl MyRepositoryTrait for MockMyRepository {
    async fn my_method(&self, param: &str) -> Result<MyData> {
        // In-memory implementation
    }
}
```

### 4. Add to Container

```rust
// di/mod.rs
pub struct AppContainer {
    // ... existing fields
    my_repo: Arc<MyRepository>,
}

impl AppContainer {
    pub fn my_repository(&self) -> Arc<MyRepository> {
        Arc::clone(&self.my_repo)
    }
}

pub struct MockAppContainer {
    // ... existing fields
    my_repo: Arc<MockMyRepository>,
}

impl MockAppContainer {
    pub fn my_repository(&self) -> Arc<dyn MyRepositoryTrait> {
        Arc::clone(&self.my_repo) as Arc<dyn MyRepositoryTrait>
    }
}
```

## Performance Considerations

### Mock Performance

Mocks are **significantly faster** than real implementations:

| Operation | Real (SQLite) | Mock (HashMap) | Speedup |
|-----------|--------------|----------------|---------|
| Create document | ~1-5ms | ~10µs | 100-500x |
| Find by ID | ~0.5-2ms | ~1µs | 500-2000x |
| Batch insert | ~50-100ms | ~100µs | 500-1000x |
| Embedding | ~20-50ms | ~1µs | 20000-50000x |

### Production Optimization

For production, the container supports:
- Connection pooling (SQLite)
- Lazy initialization (services)
- Arc for zero-cost cloning
- Async throughout for concurrency

## Error Handling

All operations return `Result<T, AppError>`:

```rust
use crate::shared::error::{Result, AppError};

async fn example(container: &MockAppContainer) -> Result<()> {
    let doc = container.documents()
        .create(...)
        .await?; // Propagates errors

    Ok(())
}
```

Error types:
- `AppError::Database` - Database operations
- `AppError::EmbeddingFailed` - Embedding generation
- `AppError::NotFound` - Resource not found

## Best Practices

### 1. Always Use Traits in Public APIs

```rust
// Good
pub async fn process(repo: &dyn DocumentRepositoryTrait) { ... }

// Bad
pub async fn process(repo: &MockDocumentRepository) { ... }
```

### 2. Clear Test Data Between Tests

```rust
#[tokio::test]
async fn my_test() {
    let container = MockAppContainer::new();

    // ... test logic ...

    container.clear_all(); // Clean up
}
```

### 3. Use Batch Operations

```rust
// Good: Single batch operation
let ids = repo.create_batch(&doc_id, chunks).await?;

// Bad: Multiple individual operations
for chunk in chunks {
    repo.create(&doc_id, &chunk).await?;
}
```

### 4. Prefer Constructor Injection

```rust
// Good: Dependencies injected
pub struct MyService {
    repo: Arc<dyn DocumentRepositoryTrait>,
}

// Bad: Creates own dependencies
pub struct MyService {
    repo: DocumentRepository,
}
```

## Migration Guide

### Migrating Existing Code

**Before:**
```rust
async fn index_file(pool: SqlitePool, path: &str) -> Result<()> {
    let doc_repo = DocumentRepository::new(pool.clone());
    let chunk_repo = ChunkRepository::new(pool.clone());
    let embedding_service = EmbeddingService::new(model_path).await?;

    // ... use repositories
}
```

**After:**
```rust
async fn index_file(container: &AppContainer, path: &str) -> Result<()> {
    let doc_repo = container.documents();
    let chunk_repo = container.chunks();
    let embedding_service = container.embedding_service();

    // ... use repositories (same code!)
}
```

## Future Enhancements

Potential additions to the DI system:

1. **Scoped Lifetimes** - Request-scoped vs application-scoped
2. **Configuration Injection** - Inject config objects
3. **Factory Pattern** - Factories for creating instances
4. **Auto-wiring** - Automatic dependency resolution
5. **Conditional Binding** - Feature-flag based implementations

## References

- **Zen-Architect Spec**: Architecture guidelines
- **Repository Pattern**: Martin Fowler's PoEAA
- **Dependency Injection**: Dependency Inversion Principle (SOLID)
- **Rust Async**: Tokio and async-trait documentation

## Contributing

When adding new features:

1. Define trait in `repositories/traits.rs` or `services/traits.rs`
2. Implement production version
3. Implement mock version in `mocks.rs`
4. Add to both containers in `di/mod.rs`
5. Write comprehensive tests in `di/tests.rs`
6. Update this README

---

**Last Updated**: 2025-11-15
**Version**: 1.0.0
**Maintainer**: Recall Development Team
