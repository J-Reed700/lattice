# Snapshot Testing Guide

This directory contains snapshot tests for the Recall Desktop Tauri application. Snapshot tests capture the output of complex data structures and compare them against stored "snapshots" to detect unintended changes.

## Overview

Snapshot testing is particularly useful for:
- **Search Results**: Ensuring consistent formatting of search result structures
- **Indexing Output**: Validating chunking and embedding storage formats
- **Audit Events**: Verifying audit log serialization and structure

We use the [`insta`](https://docs.rs/insta/) crate for snapshot testing.

## Test Organization

```
tests/snapshots/
├── README.md                        # This file
├── search_results_test.rs          # Search result format tests
├── indexing_output_test.rs         # Indexing and chunking tests
├── audit_events_test.rs            # Audit event serialization tests
└── snapshots/                      # Generated snapshot files
    ├── search_results_test__*.snap
    ├── indexing_output_test__*.snap
    └── audit_events_test__*.snap
```

## Running Snapshot Tests

### Run All Snapshot Tests

```bash
cd vault/desktop/src-tauri
cargo test --test search_results_test
cargo test --test indexing_output_test
cargo test --test audit_events_test
```

Or run all tests together:

```bash
cargo test snapshots
```

### Run Specific Snapshot Test

```bash
cargo test test_hybrid_search_results_format
```

## Reviewing Snapshots

When snapshot tests fail, use `cargo-insta` for interactive review:

### Install cargo-insta

```bash
cargo install cargo-insta
```

### Review Changes Interactively

```bash
# Review all pending snapshots
cargo insta review

# Review snapshots for a specific test file
cargo insta review --test search_results_test
```

The interactive review tool will show:
- **Old snapshot** (expected)
- **New snapshot** (actual)
- **Diff** highlighting changes

You can then:
- **Accept** changes (if intentional)
- **Reject** changes (if bugs)
- **Skip** for manual review later

### Accept All Changes

```bash
cargo insta accept
```

### Reject All Changes

```bash
cargo insta reject
```

## When to Update Snapshots

Update snapshots when you **intentionally** change:

1. **Data Structure Changes**
   - Adding/removing fields to search results
   - Changing serialization format
   - Modifying event structures

2. **Format Improvements**
   - Better JSON formatting
   - Enhanced metadata fields
   - Improved field naming

3. **Feature Additions**
   - New search modes
   - Additional indexing events
   - Extra audit actions

**⚠️ DO NOT** accept snapshot changes if:
- Tests are failing due to bugs
- Changes are unintentional
- You don't understand why the snapshot changed

## Creating New Snapshot Tests

### Basic Pattern

```rust
use insta::assert_json_snapshot;

#[test]
fn test_my_feature() {
    let data = MyStruct {
        field: "value",
    };

    assert_json_snapshot!("snapshot_name", data);
}
```

### With Redactions

Use redactions to ignore non-deterministic fields like IDs and timestamps:

```rust
assert_json_snapshot!("snapshot_name", data, {
    ".id" => "[event_id]",
    ".timestamp" => "[timestamp]",
    ".**.uuid" => insta::dynamic_redaction(|value, _path| {
        value.as_str().unwrap().to_string()
    })
});
```

### Common Redaction Patterns

```rust
// Static redaction (replace with fixed string)
".id" => "[id]"

// Dynamic redaction (transform value)
".**.timestamp" => insta::dynamic_redaction(|value, _| {
    "[timestamp]"
})

// Sorted redaction (for unordered collections)
".items" => insta::sorted_redaction()
```

## Test Categories

### 1. Search Results Tests (`search_results_test.rs`)

Tests snapshot formats for:
- Hybrid search results (vector + BM25)
- Pure vector search results
- Pure BM25 search results
- Search results with metadata
- Empty/edge case results

**Key snapshots:**
- `hybrid_search_basic`
- `hybrid_search_with_reranking`
- `bm25_search_results`
- `vector_search_results`

### 2. Indexing Output Tests (`indexing_output_test.rs`)

Tests snapshot formats for:
- Text chunking output
- Embedding storage format
- Indexing events (started, progress, completed, error)
- Batch processing results

**Key snapshots:**
- `text_chunks_basic`
- `embedding_storage`
- `indexing_event_sequence`
- `indexing_file_completed`

### 3. Audit Events Tests (`audit_events_test.rs`)

Tests snapshot formats for:
- File operation events
- Search/query events
- Credential access events
- Configuration changes
- System events

**Key snapshots:**
- `audit_file_indexed_success`
- `audit_search_performed`
- `audit_credential_accessed`
- `audit_log_sequence`

## Best Practices

### 1. Use Descriptive Snapshot Names

```rust
// Good
assert_json_snapshot!("hybrid_search_with_reranking", results);

// Bad
assert_json_snapshot!("test1", results);
```

### 2. Redact Non-Deterministic Data

Always redact:
- UUIDs
- Timestamps
- File paths (when testing structure, not specific paths)
- Generated IDs

```rust
assert_json_snapshot!("my_snapshot", data, {
    ".id" => "[id]",
    ".timestamp" => "[timestamp]",
    ".path" => "[path]"
});
```

### 3. Test Edge Cases

Include snapshots for:
- Empty collections
- Single items
- Maximum sizes
- Error conditions

### 4. Keep Snapshots Focused

Each test should focus on one specific aspect:

```rust
// Good - focused tests
#[test]
fn test_search_results_with_metadata() { ... }

#[test]
fn test_search_results_empty() { ... }

// Bad - testing too much at once
#[test]
fn test_all_search_scenarios() { ... }
```

### 5. Document Complex Snapshots

Add comments explaining what the test validates:

```rust
#[test]
fn test_hybrid_search_with_reranking() {
    // Tests that reranking scores are properly included
    // and override the original fusion scores
    let results = vec![...];
    assert_json_snapshot!("hybrid_search_with_reranking", results);
}
```

## Snapshot File Format

Snapshots are stored as `.snap` files in YAML format:

```yaml
---
source: tests/snapshots/search_results_test.rs
expression: results
---
[
  {
    "id": "doc1",
    "score": 0.95,
    "vector_score": 0.92,
    ...
  }
]
```

## Integration with CI/CD

Snapshot tests should be part of your CI pipeline:

```bash
# In CI, fail if snapshots don't match
cargo test --test search_results_test -- --nocapture

# Check for pending snapshots
cargo insta test --check
```

If CI fails due to snapshot mismatches:
1. Review the diff locally using `cargo insta review`
2. If changes are intentional, accept and commit updated snapshots
3. If changes are bugs, fix the code and re-run tests

## Troubleshooting

### Snapshot Test Fails

1. **Review the diff**:
   ```bash
   cargo insta review
   ```

2. **Check if change is intentional**:
   - If yes: accept the snapshot
   - If no: fix your code

### Snapshot Not Generated

Ensure you're using the correct assertion macro:
```rust
use insta::assert_json_snapshot;  // For JSON
use insta::assert_snapshot;       // For plain text
```

### Redactions Not Working

Check redaction syntax:
```rust
// Correct
".id" => "[id]"

// Incorrect
"id" => "[id]"  // Missing dot prefix
```

## Resources

- **insta Documentation**: https://docs.rs/insta/
- **insta Guide**: https://insta.rs/docs/
- **Redaction Patterns**: https://insta.rs/docs/redactions/
- **cargo-insta CLI**: https://insta.rs/docs/cli/

## Contributing

When adding new snapshot tests:

1. Create a focused test for a specific scenario
2. Use descriptive snapshot names
3. Add appropriate redactions
4. Document complex tests with comments
5. Review generated snapshots before committing
6. Update this README if adding new test categories

## Examples

### Example: Testing New Search Mode

```rust
#[test]
fn test_semantic_fusion_search() {
    let results = vec![
        HybridSearchResult {
            id: "doc1".to_string(),
            score: 0.95,
            // ... other fields
        },
    ];

    assert_json_snapshot!("semantic_fusion_results", results, {
        ".**.id" => insta::sorted_redaction()
    });
}
```

Run the test:
```bash
cargo test test_semantic_fusion_search
```

Review the snapshot:
```bash
cargo insta review --test search_results_test
```

Accept if correct:
```bash
cargo insta accept
```

---

**Last Updated**: 2024-01-15
**Maintained By**: Recall Desktop Team
