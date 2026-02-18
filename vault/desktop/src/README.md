# Tauri Backend - Rust Implementation

**Version**: 2.0
**Last Updated**: 2025-01-17
**Architecture**: Domain-Driven Design + SOLID Principles

---

## Overview

This directory contains the Rust backend for the Recall desktop application, built with Tauri 2.0. The architecture follows Domain-Driven Design (DDD) with strict trait implementation rules to maintain clean boundaries between production and test code.

---

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Run with release optimizations
cargo build --release

# Generate documentation
cargo doc --open
```

---

## Directory Structure

```
src-tauri/
├── src/
│   ├── commands/              # IPC command handlers (thin controllers)
│   ├── services/              # Business logic layer
│   │   ├── traits.rs         # Service trait definitions + mocks
│   │   ├── embedding_service.rs
│   │   ├── vector_search.rs
│   │   ├── tag_service.rs
│   │   └── ...
│   ├── domain/                # Rich domain models (DDD)
│   ├── repositories/          # Data access layer
│   ├── security/              # Security controls
│   ├── audit/                 # Audit logging
│   ├── di/                    # Dependency injection
│   │   └── service_container.rs
│   └── main.rs                # Application entry point
├── tests/                     # Integration tests
├── Cargo.toml                 # Dependencies
└── README.md                  # This file
```

---

## Architecture Quality Rules

### 🚨 Critical: Trait Implementation Rules

This codebase enforces **strict trait implementation rules** to prevent test code contamination in production builds.

#### Rule 1: Co-Location ✅

**Trait implementations MUST be in the same file as the struct.**

```rust
// ✅ CORRECT: services/embedding_service.rs
pub struct EmbeddingService { }

#[async_trait]
impl EmbeddingServiceTrait for EmbeddingService {
    // Implementation here
}
```

```rust
// ❌ WRONG: services/mocks/mock_embedding.rs
impl EmbeddingServiceTrait for EmbeddingService {
    // Violates co-location rule!
}
```

#### Rule 2: Mock Guards ✅

**All mocks MUST have `#[cfg(test)]` guards.**

```rust
// ✅ CORRECT: services/traits.rs
#[cfg(test)]
pub struct MockEmbeddingService { }

#[cfg(test)]
#[async_trait]
impl EmbeddingServiceTrait for MockEmbeddingService {
    // Mock implementation
}
```

```rust
// ❌ WRONG: Missing guards
pub struct MockEmbeddingService { } // Will be in production build!
```

#### Rule 3: No Production Code in Mock Files ✅

**Mock files (`mocks/`) MUST only contain test code.**

```rust
// ✅ CORRECT: services/mocks/mock_web.rs
#![cfg(test)]

pub struct MockWebScraper { }
// Only mock implementations
```

```rust
// ❌ WRONG: services/mocks/mock_web.rs
#![cfg(test)]

pub struct WebScraperService { } // Production code in mock file!
```

#### Rule 4: No Duplicates ✅

**Each trait has exactly ONE implementation per type.**

```rust
// ✅ CORRECT
// services/embedding_service.rs
impl EmbeddingServiceTrait for EmbeddingService { }

// services/traits.rs (different type)
#[cfg(test)]
impl EmbeddingServiceTrait for MockEmbeddingService { }
```

```rust
// ❌ WRONG: Duplicate implementations
// services/embedding_service.rs
impl EmbeddingServiceTrait for EmbeddingService { }

// services/mocks/mock_embedding.rs
impl EmbeddingServiceTrait for EmbeddingService { } // Compilation error!
```

---

## Architecture Quality Checks

### Pre-Commit Checklist

Before committing code:

- [ ] All trait implementations are co-located with their types
- [ ] All mocks have `#[cfg(test)]` guards
- [ ] No production code in `mocks/` directories
- [ ] No duplicate trait implementations
- [ ] Orphan rule compliance verified
- [ ] Tests pass: `cargo test`
- [ ] Formatting: `cargo fmt`
- [ ] Linting: `cargo clippy`

### Automated Verification

Run these commands to verify architecture compliance:

```bash
# Check for missing #[cfg(test)] guards on mocks
cd src
rg "pub struct Mock" --type rust | grep -v "#\[cfg(test)\]"
# Expected: No output (all mocks guarded)

# Check for production code in mock files
find . -path "*/mocks/*.rs" -exec grep -l "^impl.*Trait for [^Mock]" {} \;
# Expected: No output (no production code in mocks)

# Verify services have implementations
ls services/*.rs | while read f; do
  echo "Checking $f..."
  grep -q "impl.*Trait for" "$f" || echo "⚠️  No trait impl in $f"
done
# Expected: All services have implementations
```

### Common Violations & Fixes

#### ❌ Violation: Implementation in Wrong File

```rust
// services/mocks/mock_search.rs - WRONG!
impl SearchServiceTrait for VectorSearchService { }
```

**Fix**: Move to `services/vector_search.rs`:
```rust
// services/vector_search.rs - CORRECT
impl SearchServiceTrait for VectorSearchService { }
```

#### ❌ Violation: Missing `#[cfg(test)]` Guard

```rust
// services/traits.rs - WRONG!
pub struct MockService { } // No guard!
```

**Fix**: Add guard:
```rust
// services/traits.rs - CORRECT
#[cfg(test)]
pub struct MockService { }
```

#### ❌ Violation: Production Code in Mock File

```rust
// services/mocks/mock_web.rs - WRONG!
#![cfg(test)]
pub struct WebScraperService { } // Production code!
```

**Fix**: Create separate file:
```rust
// services/web_scraper.rs - CORRECT
pub struct WebScraperService { }
```

---

## Development Workflow

### Adding a New Service

1. **Define trait** in `services/traits.rs`:
```rust
#[async_trait]
pub trait MyServiceTrait: Send + Sync {
    async fn do_something(&self, input: &str) -> Result<Data>;
}
```

2. **Add mock** in `services/traits.rs`:
```rust
#[cfg(test)]
pub struct MockMyService {
    responses: Arc<Mutex<HashMap<String, Data>>>,
}

#[cfg(test)]
#[async_trait]
impl MyServiceTrait for MockMyService {
    async fn do_something(&self, input: &str) -> Result<Data> {
        // Mock implementation
    }
}
```

3. **Implement service** in `services/my_service.rs`:
```rust
pub struct MyService {
    pool: SqlitePool,
}

#[async_trait]
impl MyServiceTrait for MyService {
    async fn do_something(&self, input: &str) -> Result<Data> {
        // Production implementation
    }
}
```

4. **Register in ServiceContainer** (`di/service_container.rs`):
```rust
pub struct ServiceContainer {
    my_service: Arc<dyn MyServiceTrait>,
}

impl ServiceContainer {
    pub fn my_service(&self) -> Arc<dyn MyServiceTrait> {
        Arc::clone(&self.my_service)
    }
}
```

5. **Test with mocks**:
```rust
#[tokio::test]
async fn test_my_command() {
    let mock = Arc::new(MockMyService::new());
    let container = ServiceContainer::new(/* ... */ mock);
    // Test with mock
}
```

### Adding a New Command

1. Create file: `commands/my_feature.rs`
2. Implement command handler with:
   - Rate limiting
   - Input validation
   - Audit logging
   - Service delegation
3. Register in `main.rs` invoke_handler
4. Write integration tests

See [DEVELOPER_GUIDE.md](../DEVELOPER_GUIDE.md) for detailed examples.

---

## Testing

### Test Organization

```
tests/
├── unit/           # 188 unit tests (60%)
│   ├── domain/     # Domain model tests
│   ├── services/   # Service tests
│   └── security/   # Security unit tests
├── integration/    # 84 integration tests (30%)
│   ├── commands/   # Command tests
│   └── repositories/
└── security/       # 10 security tests (10%)
```

**Total**: 1,700+ comprehensive tests

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests
cargo test --test '*'

# Security tests
cargo test security

# With coverage
cargo tarpaulin --out Html --output-dir coverage
```

---

## Services Refactored (Phase 6)

**8 Services Fixed** to comply with trait implementation rules:

1. ✅ **EmbeddingService** - Moved from `mocks/mock_embedding.rs` to `services/embedding_service.rs`
2. ✅ **VectorSearchService** - Moved from `mocks/mock_search.rs` to `services/vector_search.rs`
3. ✅ **TagService** - Moved from `mocks/mock_tag.rs` to `services/tag_service.rs`
4. ✅ **FileStorageService** - Moved from `mocks/mock_file_storage.rs` to `services/file_storage.rs`
5. ✅ **ModelManagerService** - Moved from `mocks/mock_model_manager.rs` to `services/model_manager.rs`
6. ✅ **WebScraperService** - Moved from `mocks/mock_web.rs` to `services/web_ingestion.rs`
7. ✅ **IndexingService** - Moved from `mocks/mock_indexing.rs` to `services/indexing_service.rs`
8. ✅ **SearchEnrichmentService** - Moved from `mocks/mock_search.rs` to `services/search_enrichment.rs`

---

## Key Dependencies

- **Tauri**: 2.0 - Cross-platform desktop framework
- **SQLx**: 0.7 - Async SQL toolkit with compile-time verification
- **async-trait**: 0.1 - Async trait support
- **tokio**: 1.35 - Async runtime
- **serde**: 1.0 - Serialization framework
- **onnxruntime**: 0.0.14 - ML inference for embeddings

See `Cargo.toml` for full dependency list.

---

## Security

### Security Controls

- **Audit Logging**: CWE-778 mitigation (all operations logged)
- **Rate Limiting**: CWE-770 mitigation (DoS prevention)
- **Path Validation**: CWE-22 mitigation (directory traversal prevention)
- **Input Validation**: All inputs validated before processing
- **Secure Storage**: OS keyring integration for credentials

### Security Tests

100% coverage of security controls:
- Path traversal prevention
- Rate limiting enforcement
- Audit logging verification
- Input validation

---

## Documentation

- **[ARCHITECTURE.md](../ARCHITECTURE.md)** - Architecture overview
- **[DEVELOPER_GUIDE.md](../DEVELOPER_GUIDE.md)** - Development workflows
- **[CLAUDE.md](../../CLAUDE.md)** - AI assistant guide
- **Inline docs** - Run `cargo doc --open`

---

## Golden Rules Summary

1. **Co-location**: Implementations live with their types
2. **Guard mocks**: Always use `#[cfg(test)]`
3. **Separate concerns**: Mocks in `traits.rs`, production in dedicated files
4. **One implementation**: No duplicates per type
5. **Follow orphan rule**: Implement traits where you own trait or type

**Benefits**:
- ✅ Clear separation of test vs production code
- ✅ No contamination of production builds
- ✅ Easy to locate implementations
- ✅ Consistent architecture
- ✅ Compiler-enforced boundaries

---

## Support

- Architecture questions: See [ARCHITECTURE.md](../ARCHITECTURE.md)
- Development help: See [DEVELOPER_GUIDE.md](../DEVELOPER_GUIDE.md)
- API docs: `cargo doc --open`
- Test examples: `tests/`

---

**Version**: 2.0
**Last Updated**: 2025-01-17
**Maintainer**: Development Team
