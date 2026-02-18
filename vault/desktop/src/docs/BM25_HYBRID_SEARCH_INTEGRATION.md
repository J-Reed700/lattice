# BM25 and Hybrid Search Integration

## Overview

BM25Search and HybridSearch services have been successfully integrated into the ServiceContainer for dependency injection.

## Changes Made

### 1. ServiceContainer Updates (`src/di/service_container.rs`)

#### Fields Added
```rust
/// BM25 search service (trait object for DIP)
///
/// Provides keyword-based search using BM25 algorithm
bm25_search: Arc<dyn BM25SearchTrait>,

/// Hybrid search service (trait object for DIP)
///
/// Combines vector and BM25 search with configurable weighting
hybrid_search: Arc<dyn HybridSearchTrait>,
```

#### Getter Methods
```rust
pub fn bm25_search(&self) -> Arc<dyn BM25SearchTrait> {
    Arc::clone(&self.bm25_search)
}

pub fn hybrid_search(&self) -> Arc<dyn HybridSearchTrait> {
    Arc::clone(&self.hybrid_search)
}
```

#### Constructor Updated
The `ServiceContainer::new()` constructor now accepts:
- `bm25_search: Arc<dyn BM25SearchTrait>`
- `hybrid_search: Arc<dyn HybridSearchTrait>`

#### Builder Pattern Updated
```rust
pub fn bm25_search(mut self, service: Arc<dyn BM25SearchTrait>) -> Self
pub fn hybrid_search(mut self, service: Arc<dyn HybridSearchTrait>) -> Self
```

### 2. Setup Integration (`src/setup/app.rs`)

Services are instantiated in `create_legacy_service_container()`:

```rust
// Create BM25 search service
let bm25_search: Arc<dyn BM25SearchTrait> = Arc::new(BM25Search::new(pool.clone()));

// Create hybrid search service (combines vector and BM25)
let hybrid_search: Arc<dyn HybridSearchTrait> = Arc::new(
    HybridSearchService::new(Arc::clone(&search_service), Arc::clone(&bm25_search))
);
```

### 3. Test Updates

All test functions updated to create and pass mock implementations:
```rust
let bm25_search = Arc::new(MockBM25Search::new()) as Arc<dyn BM25SearchTrait>;
let hybrid_search = Arc::new(MockHybridSearch::new()) as Arc<dyn HybridSearchTrait>;
```

## Usage in Commands

### BM25 Search Example

```rust
#[tauri::command]
async fn bm25_search(
    container: State<'_, ServiceContainer>,
    query: String,
    limit: usize,
) -> Result<Vec<SearchResult>, AppError> {
    // Rate limiting
    container.security_context()
        .rate_limiters
        .search
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Get BM25 search service
    let bm25 = container.bm25_search();

    // Perform search
    let results = bm25.search(&query, limit).await?;

    Ok(results)
}
```

### Hybrid Search Example

```rust
#[tauri::command]
async fn hybrid_search(
    container: State<'_, ServiceContainer>,
    query: String,
    limit: usize,
    vector_weight: f32, // 0.0 = pure BM25, 1.0 = pure vector
) -> Result<Vec<SearchResult>, AppError> {
    // Rate limiting
    container.security_context()
        .rate_limiters
        .search
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Get hybrid search service
    let hybrid = container.hybrid_search();

    // Perform hybrid search
    let results = hybrid.search(&query, limit, vector_weight).await?;

    Ok(results)
}
```

## Benefits

1. **Dependency Injection**: Services are injected through the container, following SOLID principles
2. **Testability**: Easy to inject mock implementations for testing
3. **Type Safety**: All dependencies are explicitly typed through traits
4. **Flexibility**: Can swap implementations without changing commands
5. **Consistency**: Follows existing patterns for other services in the container

## Testing

### Unit Tests
Mock implementations are available:
- `MockBM25Search` - Mock BM25 search service
- `MockHybridSearch` - Mock hybrid search service

### Integration Tests
Services can be tested with real database connections:
```rust
let pool = SqlitePool::connect(":memory:").await?;
let bm25 = Arc::new(BM25Search::new(pool.clone()));
let hybrid = Arc::new(HybridSearchService::new(vector_search, bm25.clone()));
```

## Next Steps

1. Create Tauri commands that use these services
2. Update frontend to call the new commands
3. Add performance benchmarks comparing BM25, vector, and hybrid search
4. Document recommended vector_weight values for different use cases
5. Add user preferences for default search strategy

## Related Files

- `/home/user/Recall/vault/desktop/src-tauri/src/di/service_container.rs`
- `/home/user/Recall/vault/desktop/src-tauri/src/setup/app.rs`
- `/home/user/Recall/vault/desktop/src-tauri/src/search/bm25.rs`
- `/home/user/Recall/vault/desktop/src-tauri/src/search/hybrid.rs`
- `/home/user/Recall/vault/desktop/src-tauri/src/services/traits.rs`
