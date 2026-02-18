# Performance Benchmarks Guide

This document explains how to run, interpret, and use the performance benchmarks for critical paths in the Recall desktop application.

## Table of Contents

- [Overview](#overview)
- [Running Benchmarks](#running-benchmarks)
- [Benchmark Suites](#benchmark-suites)
- [Interpreting Results](#interpreting-results)
- [Performance Targets](#performance-targets)
- [Continuous Performance Monitoring](#continuous-performance-monitoring)
- [Optimization Workflow](#optimization-workflow)

---

## Overview

The Recall desktop application includes comprehensive performance benchmarks using [Criterion.rs](https://github.com/bheisler/criterion.rs) to measure and track the performance of critical code paths.

### Why Benchmark?

1. **Measure First, Optimize Later** - Avoid premature optimization by identifying actual bottlenecks
2. **Prevent Performance Regressions** - Catch performance degradations before they reach production
3. **Validate Optimizations** - Ensure optimizations actually improve performance
4. **Document Performance Characteristics** - Understand the cost of different operations

### Benchmark Philosophy

We follow these principles:

- **80/20 Rule** - Focus on the 20% of code causing 80% of performance issues
- **Real-World Scenarios** - Benchmark realistic use cases, not micro-optimizations
- **Trade-off Analysis** - Balance performance gains against code complexity
- **Zero-Cost Abstractions** - Verify that type safety has minimal runtime overhead

---

## Running Benchmarks

### Prerequisites

```bash
cd /home/user/Recall/vault/desktop/src-tauri
```

### Run All Benchmarks

```bash
cargo bench
```

This will run all benchmarks and generate HTML reports in `target/criterion/`.

### Run Specific Benchmark Suite

```bash
# Error handling benchmarks
cargo bench --bench error_handling_performance

# Domain types benchmarks
cargo bench --bench domain_types_performance

# Type safety benchmarks
cargo bench --bench type_safety_performance

# Vector search benchmarks
cargo bench --bench vector_search

# HNSW persistence benchmarks
cargo bench --bench hnsw_persistence
```

### Run Specific Benchmark Within a Suite

```bash
cargo bench --bench error_handling_performance -- error_creation
```

### Save Baseline for Comparison

```bash
# Save current performance as baseline
cargo bench -- --save-baseline master

# Compare against baseline
cargo bench -- --baseline master
```

### Generate Detailed Reports

```bash
# Run benchmarks and open HTML report
cargo bench
open target/criterion/report/index.html  # macOS
xdg-open target/criterion/report/index.html  # Linux
start target/criterion/report/index.html  # Windows
```

---

## Benchmark Suites

### 1. Error Handling Performance (`error_handling_performance.rs`)

**Purpose**: Measures the overhead of error handling patterns using `AppError` and `ResultExt`.

**Key Benchmarks**:

- **Error Creation**
  - Simple error construction
  - Complex error with fields
  - Conversion from `io::Error`

- **Error Context**
  - Static context addition
  - Dynamic context (closures)
  - Nested context chains
  - Option context

- **Error Conversion**
  - `to_string()`
  - `to_user_friendly_message()`
  - `Into<String>`

- **Error Checks**
  - `is_recoverable()`
  - `is_user_fixable()`
  - `is_fatal()`

- **Error Propagation**
  - Single-level propagation
  - Multi-level error chains

- **Realistic Usage**
  - File read error chains
  - Database error handling
  - Validation errors

**Expected Performance**:
- Error creation: < 100ns
- Context addition: < 200ns
- Error conversion: < 50ns
- User-friendly message: < 500ns

**Example Output**:
```
error_creation/simple_error
                        time:   [42.3 ns 43.1 ns 43.9 ns]
error_context/static_context
                        time:   [156 ns 159 ns 162 ns]
```

---

### 2. Domain Types Performance (`domain_types_performance.rs`)

**Purpose**: Measures the overhead of type-safe domain types using the newtype pattern.

**Key Benchmarks**:

- **ID Creation**
  - `DocumentId::new()` (UUID generation)
  - `DocumentId::from_string()`
  - Multiple ID types (Tag, Chunk, Mention)

- **ID String Operations**
  - `as_str()`
  - `to_string()`
  - `from_str()` (parsing)

- **Validation**
  - Valid tag name
  - Invalid tag name (empty, too long)
  - Validated file paths

- **Serialization**
  - JSON serialization
  - JSON deserialization
  - Roundtrip performance

- **Newtype vs Raw**
  - Newtype wrapping overhead
  - Raw string operations
  - Performance comparison

- **Hash and Equality**
  - Equality checks
  - Hash computation
  - HashMap lookups

- **Batch Operations**
  - Create 10/100/1000 IDs
  - Validate multiple tag names
  - Throughput testing

**Expected Performance**:
- ID creation (UUID): < 500ns
- ID creation from string: < 50ns
- Validation: < 100ns
- Serialization: < 1µs
- Deserialization: < 1µs

**Example Output**:
```
id_creation/document_id_new
                        time:   [387 ns 393 ns 401 ns]
validation/valid_tag_name
                        time:   [78.2 ns 79.5 ns 80.9 ns]
```

---

### 3. Type Safety Performance (`type_safety_performance.rs`)

**Purpose**: Compares type-safe approaches against unsafe alternatives to verify zero-cost abstractions.

**Key Benchmarks**:

- **Type-Safe IDs vs Strings**
  - Type-safe ID operations
  - String-based operations
  - Performance comparison

- **Validated vs Unchecked**
  - Validated tag names
  - Unchecked strings
  - Error handling overhead

- **Path Safety**
  - Validated paths
  - Raw PathBuf
  - Operations on validated paths

- **Newtype Wrapping Overhead**
  - Raw UUID operations
  - Wrapped UUID (newtype)
  - Multiple newtypes

- **Collection Operations**
  - Type-safe HashMap
  - String-based HashMap
  - Lookup performance

- **Inline Performance**
  - Inlined methods (`as_str`)
  - Direct access
  - Compiler optimization effectiveness

- **Realistic Scenarios**
  - Document lookup (1000 entries)
  - Tag filtering
  - Batch validation

- **Cache Performance**
  - Sequential access
  - Random lookup
  - Cache locality

- **Compile-Time vs Runtime**
  - Type system enforcement (zero cost)
  - Runtime type checking

**Expected Performance**:
- Newtype overhead: ~0% (zero-cost abstraction)
- Validation overhead: < 100ns
- Type conversions: < 50ns

**Key Insight**: Type safety should have **near-zero overhead** due to:
- Zero-cost abstraction (same memory layout)
- Method inlining
- Compile-time enforcement

**Example Output**:
```
type_safe_ids_vs_strings/type_safe_id_operations
                        time:   [412 ns 418 ns 425 ns]
type_safe_ids_vs_strings/string_based_id_operations
                        time:   [408 ns 414 ns 421 ns]
                        change: [-1.5% -0.8% +0.1%] (no significant difference)
```

---

## Interpreting Results

### Understanding Criterion Output

```
benchmark_name          time:   [lower_bound estimate upper_bound]
                        change: [lower% estimate% upper%] (p < 0.05)
```

**Fields**:
- **lower_bound**: 95% confidence interval lower bound
- **estimate**: Best estimate of the mean time
- **upper_bound**: 95% confidence interval upper bound
- **change**: Percentage change from previous run (if baseline exists)

### Performance Change Interpretation

| Change | Interpretation |
|--------|----------------|
| < -5% | **Significant improvement** ✅ |
| -5% to -2% | Minor improvement |
| -2% to +2% | **No significant change** (noise) |
| +2% to +5% | Minor regression ⚠️ |
| > +5% | **Significant regression** ❌ Investigate! |

### Statistical Significance

Criterion uses statistical analysis to determine if changes are significant:

- **p < 0.05**: Change is statistically significant
- **No change detected**: Variation is within noise threshold

### HTML Reports

The HTML reports provide:

1. **Graphs**: Visual representation of performance over time
2. **Violin Plots**: Distribution of measurements
3. **Comparison**: Side-by-side comparison with baseline
4. **Regression**: Trend analysis

---

## Performance Targets

### Critical Path Operations

| Operation | Target | Threshold | Impact |
|-----------|--------|-----------|--------|
| Error creation | < 100ns | 200ns | High-frequency operation |
| Error context | < 200ns | 500ns | Error path (less critical) |
| ID creation | < 500ns | 1µs | High-frequency operation |
| ID validation | < 50ns | 100ns | Very high-frequency |
| Tag validation | < 100ns | 200ns | User-facing operation |
| Serialization | < 1µs | 5µs | I/O bound (less critical) |
| HashMap lookup | < 50ns | 100ns | Critical path |

### Performance Budget

**Per-Request Budget**: 100µs for core operations

Breakdown:
- Error handling: 5µs (5%)
- Type validation: 10µs (10%)
- Domain logic: 85µs (85%)

### Optimization Priority

1. **P0 - Critical**: Operations in hot paths (> 1000/s)
2. **P1 - High**: User-facing operations (> 100/s)
3. **P2 - Medium**: Background operations (< 100/s)
4. **P3 - Low**: One-time setup operations

---

## Continuous Performance Monitoring

### CI/CD Integration

Add to your CI pipeline:

```yaml
# .github/workflows/benchmarks.yml
name: Performance Benchmarks

on:
  pull_request:
  push:
    branches: [main]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run benchmarks
        run: |
          cd vault/desktop/src-tauri
          cargo bench --no-fail-fast
      - name: Upload results
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-results
          path: vault/desktop/src-tauri/target/criterion
```

### Performance Regression Detection

```bash
# Before making changes
cargo bench -- --save-baseline before

# After making changes
cargo bench -- --baseline before

# Review changes
open target/criterion/report/index.html
```

If you see significant regressions:
1. **Investigate** the cause
2. **Profile** the code to find bottlenecks
3. **Optimize** only if necessary
4. **Re-benchmark** to verify improvement
5. **Document** the trade-offs

---

## Optimization Workflow

### Step 1: Measure (Establish Baseline)

```bash
# Run benchmarks to establish baseline
cargo bench -- --save-baseline main

# Identify bottlenecks
open target/criterion/report/index.html
```

**Questions to ask**:
- Which operations are slowest?
- Which operations happen most frequently?
- What is the user impact?

### Step 2: Profile (Root Cause Analysis)

Use profiling tools to understand *why* code is slow:

```bash
# CPU profiling
cargo flamegraph --bench domain_types_performance

# Memory profiling
valgrind --tool=massif target/release/bench-binary
```

### Step 3: Optimize (Implement Changes)

Apply optimization patterns:

1. **Algorithmic improvements** (biggest wins)
2. **Caching** (for expensive computations)
3. **Batch processing** (reduce overhead)
4. **Better data structures** (e.g., HashMap vs Vec)
5. **Avoiding allocations** (use references)

### Step 4: Validate (Measure Again)

```bash
# Benchmark optimized code
cargo bench -- --baseline main

# Look for improvements
# Expected: -5% or more for significant changes
```

### Step 5: Document (Record Trade-offs)

Document:
- **Performance gain**: X% improvement
- **Code complexity**: Increased/decreased/same
- **Maintenance burden**: Trade-offs made
- **Test coverage**: Ensure correctness

**Example**:
```rust
// PERFORMANCE: Using HashMap for O(1) lookups instead of Vec O(n)
// Trade-off: Slightly more memory usage, but 10x faster for lookups > 100 items
// Benchmark: document_lookup improved from 450ns to 45ns (-90%)
```

---

## Best Practices

### DO ✅

1. **Benchmark before optimizing** - Measure first, always
2. **Use realistic data** - Benchmark with real-world sizes
3. **Benchmark hot paths** - Focus on high-frequency operations
4. **Save baselines** - Track performance over time
5. **Document trade-offs** - Explain why optimizations were made
6. **Keep benchmarks fast** - Each benchmark should run in < 10s

### DON'T ❌

1. **Don't optimize without measuring** - Premature optimization is evil
2. **Don't benchmark in debug mode** - Always use `--release`
3. **Don't ignore variance** - Look at confidence intervals
4. **Don't micro-optimize** - Focus on algorithmic improvements
5. **Don't sacrifice readability** - Complexity has a cost
6. **Don't benchmark on noisy systems** - Close other applications

---

## Common Performance Patterns

### Pattern 1: Avoiding Allocations

```rust
// ❌ Slow: Allocates a new String
fn format_id_slow(id: &DocumentId) -> String {
    format!("doc_{}", id.as_str())
}

// ✅ Fast: Returns a reference (zero allocation)
fn get_id_fast(id: &DocumentId) -> &str {
    id.as_str()
}
```

### Pattern 2: Using References

```rust
// ❌ Slow: Clones the ID
fn process_slow(id: DocumentId) -> bool {
    id.as_str().len() > 0
}

// ✅ Fast: Uses a reference
fn process_fast(id: &DocumentId) -> bool {
    id.as_str().len() > 0
}
```

### Pattern 3: Batching Operations

```rust
// ❌ Slow: Multiple HashMap lookups
fn get_documents_slow(ids: &[DocumentId], map: &HashMap<DocumentId, Doc>) -> Vec<Doc> {
    ids.iter().filter_map(|id| map.get(id).cloned()).collect()
}

// ✅ Fast: Single pass with references
fn get_documents_fast<'a>(
    ids: &[DocumentId],
    map: &'a HashMap<DocumentId, Doc>
) -> Vec<&'a Doc> {
    ids.iter().filter_map(|id| map.get(id)).collect()
}
```

### Pattern 4: Lazy Evaluation

```rust
// ❌ Slow: Computes even if not needed
fn get_message_slow(err: &AppError) -> String {
    let user_msg = err.to_user_friendly_message();  // Always computed
    if is_debug_mode() {
        err.to_string()
    } else {
        user_msg
    }
}

// ✅ Fast: Lazy evaluation
fn get_message_fast(err: &AppError) -> String {
    if is_debug_mode() {
        err.to_string()
    } else {
        err.to_user_friendly_message()  // Only computed when needed
    }
}
```

---

## Troubleshooting

### Benchmarks Are Unstable

**Symptoms**: Large variance in results, inconsistent performance

**Solutions**:
1. Close background applications
2. Disable CPU frequency scaling:
   ```bash
   # Linux
   sudo cpupower frequency-set --governor performance
   ```
3. Run multiple iterations:
   ```bash
   cargo bench -- --sample-size 1000
   ```

### Benchmarks Take Too Long

**Symptoms**: Benchmarks running for > 5 minutes

**Solutions**:
1. Reduce sample size:
   ```bash
   cargo bench -- --sample-size 50
   ```
2. Run specific benchmarks:
   ```bash
   cargo bench --bench error_handling_performance
   ```
3. Use `--quick` flag:
   ```bash
   cargo bench -- --quick
   ```

### Unexpected Performance Regression

**Symptoms**: Sudden performance drop without code changes

**Possible Causes**:
1. **Compiler version change** - Different optimization levels
2. **System load** - Background processes interfering
3. **Thermal throttling** - CPU overheating
4. **Memory pressure** - System swapping to disk

**Investigation**:
1. Run flamegraph to identify hotspot:
   ```bash
   cargo flamegraph --bench domain_types_performance
   ```
2. Check system resources:
   ```bash
   htop  # Linux
   top   # macOS
   ```
3. Compare with baseline on another machine

---

## Additional Resources

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Flamegraph Guide](https://github.com/flamegraph-rs/flamegraph)
- [Rust Profiling Tools](https://github.com/rust-lang/rustc-guide/blob/master/src/profiling.md)

---

## Summary

**Key Takeaways**:

1. ⏱️ **Measure First** - Always benchmark before optimizing
2. 🎯 **Focus on Hot Paths** - Optimize the 20% causing 80% of issues
3. 📊 **Track Over Time** - Use baselines to detect regressions
4. 🔍 **Profile to Understand** - Use flamegraphs to find bottlenecks
5. ⚖️ **Balance Trade-offs** - Consider complexity vs performance gains
6. 📝 **Document Decisions** - Explain why optimizations were made

**Remember**: The goal is not to make everything fast, but to make the right things fast enough while maintaining code quality and readability.

---

**Last Updated**: 2025-11-15

For questions or issues with benchmarks, please consult the performance optimization team or file an issue in the repository.
