# HyDE Query Processing - Phase 1: Query Classifier

## Overview

The QueryClassifier is the first component of the Hypothetical Document Embeddings (HyDE) system. It analyzes user queries and classifies them into one of three types:

- **Greeting**: Simple greetings like "hi", "hello", "good morning"
- **Question**: Natural language questions or informational queries
- **Command**: Action-oriented commands like "search", "find", "index"

## Implementation

### Module Structure

```
hyde/
├── mod.rs                 # Module exports
├── query_classifier.rs    # Core implementation
└── README.md             # This file
```

### Query Classification Logic

1. **Greeting Detection** (highest priority)
   - Matches patterns: `hi`, `hello`, `hey`, `greetings`, `good morning/afternoon/evening`
   - Must be exact match (no additional text)
   - Case-insensitive

2. **Command Detection** (medium priority)
   - Starts with command verbs: `search`, `find`, `show`, `list`, `index`, `delete`, `remove`, `open`, `create`, `update`
   - Case-insensitive

3. **Question Detection** (low priority)
   - Contains question words: `what`, `how`, `why`, `when`, `where`, `who`, `which`
   - Or question phrases: `can you`, `could you`, `explain`, `tell me`
   - Case-insensitive

4. **Default**: If no pattern matches, defaults to `Question`

## Usage

```rust
use vault_desktop::features::qa::hyde::{QueryClassifier, QueryType};

let classifier = QueryClassifier::new();

// Classify queries
assert_eq!(classifier.classify("hello"), QueryType::Greeting);
assert_eq!(classifier.classify("what is rust?"), QueryType::Question);
assert_eq!(classifier.classify("search for documents"), QueryType::Command);
```

## Testing

Comprehensive tests cover:
- ✅ Greeting detection (exact matches)
- ✅ Question detection (question words)
- ✅ Command detection (command verbs)
- ✅ Default behavior (fallback to Question)
- ✅ Case insensitivity
- ✅ Whitespace handling

Run tests:
```bash
# Standalone integration test
cargo test --test query_classifier_test

# Inline unit tests (requires fixing other compilation errors)
cargo test --lib features::qa::hyde
```

## Test Results

```
running 6 tests
test test_command_detection ... ok
test test_case_insensitivity ... ok
test test_greeting_detection ... ok
test test_whitespace_handling ... ok
test test_default_to_question ... ok
test test_question_detection ... ok

test result: ok. 6 passed; 0 failed; 0 ignored
```

## Dependencies

- `regex` - Pattern matching for query classification
- `once_cell` - Lazy static initialization of regex patterns
- `serde` - Serialization for `QueryType` enum

All dependencies are already in `Cargo.toml`.

## Architecture Compliance

✅ **Self-Contained Module**: All code in `hyde/` directory
✅ **Clear Public Interface**: Only `QueryClassifier` and `QueryType` exported
✅ **No External Dependencies**: Uses only standard patterns from existing services
✅ **Comprehensive Tests**: 6 tests covering all scenarios
✅ **Documentation**: Inline docs and README

## Next Steps (Future Phases)

- Phase 2: Hypothetical document generator
- Phase 3: Query expansion
- Phase 4: Integration with search pipeline
- Phase 5: Performance optimization

## Files Created

1. `src-tauri/src/infrastructure/services/hyde/mod.rs` - Module declaration
2. `src-tauri/src/infrastructure/services/hyde/query_classifier.rs` - Implementation
3. `src-tauri/src/infrastructure/services/hyde/README.md` - Documentation
4. `src-tauri/tests/query_classifier_test.rs` - Integration tests

## Integration

The module is registered in `src-tauri/src/infrastructure/services/mod.rs`:

```rust
// HyDE query processing
pub mod hyde;
```

And can be imported anywhere in the codebase:

```rust
use crate::features::qa::hyde::{QueryClassifier, QueryType};
```
