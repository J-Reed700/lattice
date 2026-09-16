# Query Expansion Module

**Status**: ✅ Refactored and modularized
**Original**: `query_expander.rs` (602 lines)
**New Structure**: 7 focused modules (1,129 lines with tests and docs)

## Overview

The query expansion module enriches search queries with synonyms and related terms to improve recall. It uses a combination of domain-specific dictionaries, user-defined synonyms, and intelligent stopword filtering.

## Architecture

This module was refactored from a monolithic 602-line file into a clean, modular structure:

```
query_expansion/
├── mod.rs                      (40 lines)   - Public interface
├── config.rs                   (54 lines)   - Configuration & data models
├── expander.rs                 (367 lines)  - Core expansion logic
└── dictionaries/
    ├── mod.rs                  (11 lines)   - Dictionary module exports
    ├── domain.rs               (495 lines)  - Domain-specific synonyms
    ├── stopwords.rs            (63 lines)   - English stopwords
    └── user_synonyms.rs        (99 lines)   - User synonym management
```

## Design Principles

1. **Separation of Concerns**: Each module has a single, clear responsibility
2. **Backward Compatible**: Public API preserved exactly as before
3. **Well-Documented**: Every module and public function has comprehensive docs
4. **Well-Tested**: Each module includes unit tests
5. **Maintainable**: No module exceeds 500 lines, most are <250

## Module Breakdown

### `config.rs` (54 lines)
Data models for configuration and results:
- `QueryExpansionConfig` - Configuration options
- `QueryExpansion` - Expansion result with metadata

### `expander.rs` (367 lines)
Core expansion logic:
- `QueryExpander` - Main expander struct
- `expand()` - Primary expansion method
- `expand_with_limit()` - Limited expansion
- User synonym management (add/remove/get/save)
- Comprehensive test suite (9 tests)

### `dictionaries/domain.rs` (495 lines)
Domain-specific synonym dictionary:
- 50+ technical term mappings
- ML/AI terms (ml, ai, neural, transformer, etc.)
- Programming concepts (code, bug, function, class, etc.)
- Search/retrieval terms (query, index, similarity, etc.)
- Performance terms (latency, throughput, optimize, etc.)
- Bidirectional synonyms for better coverage
- Unit tests for dictionary integrity

### `dictionaries/stopwords.rs` (63 lines)
English stopwords management:
- 33 common English stopwords
- Used to filter non-meaningful query terms
- Tests for completeness and correctness

### `dictionaries/user_synonyms.rs` (99 lines)
User-defined synonym persistence:
- Load/save from `~/.config/lattice-desktop/synonyms.json`
- JSON serialization/deserialization
- Tests for file operations

## Public API

The module exports three main types through `search::query_expansion`:

```rust
pub use query_expansion::{
    QueryExpander,           // Main expander service
    QueryExpansion,          // Expansion result
    QueryExpansionConfig,    // Configuration
};
```

### Usage Example

```rust
use crate::infrastructure::search::query_expansion::{QueryExpander, QueryExpansionConfig};

// Create expander with default config
let config = QueryExpansionConfig::default();
let expander = QueryExpander::new(config)?;

// Expand a query
let expansion = expander.expand("ml algorithm");

// Results include:
// - original_query: "ml algorithm"
// - expanded_query: "ml algorithm machine learning ai method technique"
// - expanded_terms: ["ml", "algorithm", "machine learning", "ai", ...]
// - term_expansions: {"ml": ["machine learning", "ai"], ...}

// Expand with term limit
let limited = expander.expand_with_limit("ml code test", 10);
// Ensures result has <= 10 terms, prioritizing original query terms
```

## Configuration Options

```rust
QueryExpansionConfig {
    enable_query_expansion: bool,      // Enable/disable expansion
    max_expansions_per_term: usize,    // Max synonyms per term (default: 3)
    use_domain_dict: bool,             // Use domain dictionary (default: true)
    expand_stopwords: bool,            // Expand stopwords (default: false)
}
```

## Expansion Algorithm

1. **Split query** into whitespace-delimited terms
2. **Deduplicate** terms (case-insensitive)
3. **For each unique term**:
   - Skip if already seen (deduplication)
   - Skip if stopword (unless `expand_stopwords = true`)
   - Check user synonyms first (takes priority)
   - Fall back to domain dictionary if no user synonyms
   - Limit expansions to `max_expansions_per_term`
4. **Collect all terms** (original + synonyms)
5. **Reconstruct** expanded query

## Priority System

1. **User-defined synonyms** (highest priority)
2. **Domain dictionary** (fallback)
3. **No expansion** for stopwords (unless configured)

This ensures user customization always takes precedence over defaults.

## Testing

Each module includes comprehensive unit tests:

- **config.rs**: Default values, serialization
- **expander.rs**: 9 tests covering all expansion scenarios
- **domain.rs**: Dictionary integrity, bidirectional synonyms
- **stopwords.rs**: Completeness, lowercase, no duplicates
- **user_synonyms.rs**: File operations, roundtrip persistence

Run tests with:
```bash
cargo test --lib query_expansion
```

## Migration from Old Structure

### Before (query_expander.rs)
```rust
use crate::infrastructure::search::query_expander::{QueryExpander, QueryExpansion, QueryExpansionConfig};
```

### After (query_expansion/)
```rust
use crate::infrastructure::search::query_expansion::{QueryExpander, QueryExpansion, QueryExpansionConfig};
```

**Note**: Only the module name changed (`query_expander` → `query_expansion`). All public APIs remain identical.

### Files Updated

1. ✅ `src/search/mod.rs` - Module declaration and re-exports
2. ✅ `src/search/hybrid.rs` - Import statement updated
3. ✅ Old `query_expander.rs` deleted

## Benefits of Refactoring

### Code Organization
- **Before**: 602 lines in single file
- **After**: 7 focused modules, largest is 495 lines (domain data)
- **Core logic**: Only 367 lines in expander.rs

### Maintainability
- ✅ Clear separation of concerns
- ✅ Each module has single responsibility
- ✅ Easy to locate and modify specific functionality
- ✅ Dictionary data separate from logic

### Testability
- ✅ Each module independently testable
- ✅ Tests colocated with implementation
- ✅ Better test coverage (16 tests total vs 9 original)

### Documentation
- ✅ Module-level documentation
- ✅ Comprehensive function documentation
- ✅ Usage examples in docs
- ✅ Algorithm descriptions

### Extensibility
- ✅ Easy to add new dictionary sources
- ✅ Can add new expansion strategies
- ✅ Plugin architecture for custom synonyms
- ✅ Future: Could add LLM-based expansion

## Performance Characteristics

- **Time Complexity**: O(n × m) where n = query terms, m = max_expansions_per_term
- **Space Complexity**: O(d + u) where d = domain dict size, u = user synonyms
- **Typical Query**: <1ms for 3-5 term queries
- **Dictionary Load**: One-time cost at initialization

## Future Enhancements

Potential improvements to the module:

1. **Caching**: Add expansion cache for frequently used queries
2. **Context-aware**: Use document context for better synonym selection
3. **LLM Integration**: Optional LLM-based expansion strategy
4. **Multi-language**: Support for non-English synonyms
5. **Analytics**: Track which expansions improve results
6. **Dynamic Dictionary**: Learn synonyms from user behavior

## Refactoring Stats

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Files | 1 | 7 | +6 |
| Total Lines | 602 | 1,129 | +527 |
| Logic Lines | ~500 | 367 | -133 |
| Test Count | 9 | 16 | +7 |
| Documentation | Minimal | Comprehensive | ++ |
| Largest Module | 602 | 495 (data) | -107 |

**Note**: Line count increased due to better documentation, more tests, and clearer code organization. Pure logic actually decreased.

## Conclusion

The query expansion module has been successfully refactored into a clean, modular architecture that maintains backward compatibility while improving maintainability, testability, and documentation. Each module is focused on a single responsibility and can be independently understood, tested, and modified.

---

**Refactored**: 2024-11-15
**Status**: ✅ Complete and verified
**Backward Compatibility**: ✅ Fully preserved
