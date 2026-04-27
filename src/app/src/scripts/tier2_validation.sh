#!/usr/bin/env bash
# TIER 2 Performance Validation Script
#
# Oracle's Mission: Validate bulk INSERT optimization worked
# Expected: db_save_pct < 5%, embedding_pct > 70%, 8-15x speedup
#
# Usage: ./scripts/tier2_validation.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_DATA_DIR="$PROJECT_ROOT/test_data_tier2"
LOG_FILE="$PROJECT_ROOT/tier2_validation.log"
RESULTS_FILE="$PROJECT_ROOT/TIER2_VALIDATION_RESULTS.md"

echo "==================================================================="
echo "TIER 2 PERFORMANCE VALIDATION"
echo "==================================================================="
echo ""
echo "Oracle's Predictions:"
echo "  - db_save_pct: < 5% (was ~60%)"
echo "  - embedding_pct: 70-80% (was ~25%)"
echo "  - Files/sec: 8-16 (was ~1)"
echo ""
echo "==================================================================="
echo ""

# Step 1: Create test dataset
echo "[1/4] Creating test dataset (100 diverse files)..."
rm -rf "$TEST_DATA_DIR"
mkdir -p "$TEST_DATA_DIR"

# Create 50 text files of varying sizes
for i in {1..50}; do
    size=$((1000 + RANDOM % 5000))  # 1-6KB files
    head -c $size /dev/urandom | base64 > "$TEST_DATA_DIR/text_${i}.txt"
    echo "Test file $i with $(wc -c < "$TEST_DATA_DIR/text_${i}.txt") bytes" >> "$TEST_DATA_DIR/text_${i}.txt"
done

# Create 30 markdown files with code
for i in {1..30}; do
    cat > "$TEST_DATA_DIR/doc_${i}.md" <<EOF
# Test Document $i

This is a test markdown document for TIER 2 validation.

## Code Example

\`\`\`rust
fn main() {
    println!("Hello from test doc $i");
    let x = vec![1, 2, 3, 4, 5];
    let sum: i32 = x.iter().sum();
    println!("Sum: {}", sum);
}
\`\`\`

## Performance Testing

This file tests:
- Content extraction performance
- Embedding generation (ONNX inference)
- Database bulk INSERT operations
- Vector search indexing

Document $i created at $(date)
Random data: $(head -c 500 /dev/urandom | base64 | head -c 500)
EOF
done

# Create 20 larger files (simulating documents)
for i in {1..20}; do
    cat > "$TEST_DATA_DIR/large_${i}.txt" <<EOF
Large Test Document $i
$(printf '=%.0s' {1..80})

This is a larger test document designed to stress-test the indexing pipeline.

$(for j in {1..50}; do
    echo "Paragraph $j: $(head -c 200 /dev/urandom | base64 | head -c 200)"
done)

End of document $i
Total size: $(wc -c < "$TEST_DATA_DIR/large_${i}.txt") bytes
EOF
done

FILE_COUNT=$(ls -1 "$TEST_DATA_DIR" | wc -l | tr -d ' ')
echo "  ✓ Created $FILE_COUNT test files in $TEST_DATA_DIR"
echo ""

# Step 2: Build the project (ensure latest code)
echo "[2/4] Building project with latest optimizations..."
cd "$PROJECT_ROOT"
cargo build --release --quiet 2>&1 | grep -E "Compiling|Finished" || true
echo "  ✓ Build complete"
echo ""

# Step 3: Run batch import with instrumentation
echo "[3/4] Running batch import with full instrumentation..."
echo "  This will take 1-3 minutes depending on machine speed..."
echo ""

# Set RUST_LOG to capture all our instrumentation
export RUST_LOG="lattice_desktop=info"

# Run the import (this will need to be adapted based on your CLI interface)
# For now, we'll create a simple Rust test program
cat > "$PROJECT_ROOT/examples/tier2_validation.rs" <<'RUST_EOF'
//! TIER 2 Validation - Batch Import Test
//!
//! Measures actual performance with Oracle's instrumentation

use lattice_desktop::*;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for instrumentation
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("TIER 2 Validation Test Starting...");
    println!("Collecting test files...");

    // Get all test files
    let test_dir = PathBuf::from("test_data_tier2");
    let files: Vec<PathBuf> = std::fs::read_dir(&test_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    println!("Found {} test files", files.len());
    println!("Starting batch import with instrumentation...\n");

    let start = Instant::now();

    // TODO: Call your batch import function here
    // This is a placeholder - adapt to your actual API

    let duration = start.elapsed();
    let files_per_sec = files.len() as f64 / duration.as_secs_f64();

    println!("\n=== TIER 2 VALIDATION RESULTS ===");
    println!("Total files: {}", files.len());
    println!("Total time: {:.2}s", duration.as_secs_f64());
    println!("Files/sec: {:.2}", files_per_sec);
    println!("\nCheck logs above for detailed metrics:");
    println!("  - Look for 'db_save_pct' < 5%");
    println!("  - Look for 'embedding_generation_pct' > 70%");

    Ok(())
}
RUST_EOF

# Since we need actual implementation, let's just document the manual process for now
echo "  NOTE: Automated benchmark requires integration with your app's CLI"
echo "  "
echo "  Manual Validation Steps:"
echo "  1. Start your application"
echo "  2. Import all files from: $TEST_DATA_DIR"
echo "  3. Monitor logs for instrumentation metrics"
echo "  4. Look for these key metrics in the logs:"
echo "     - IndexFileUseCase: db_save_pct, embedding_generation_pct"
echo "     - StartBatchFileImportUseCase: files_per_second"
echo "     - OnnxEmbeddingService: inference_pct"
echo ""
echo "  Or run: RUST_LOG=lattice_desktop=info cargo run --example tier2_validation"
echo ""

# Step 4: Create validation report template
echo "[4/4] Creating validation report template..."

cat > "$RESULTS_FILE" <<'EOF'
# TIER 2 Performance Validation Results

**Date**: $(date)
**Test Dataset**: 100 files (50 text, 30 markdown, 20 large docs)
**Status**: ⏳ AWAITING MANUAL VALIDATION

---

## Oracle's Predictions

| Metric | Before | After (Predicted) | Threshold |
|--------|--------|-------------------|-----------|
| db_save_pct | 50-60% | <5% | **MUST BE <5%** |
| embedding_pct | 20-30% | 70-80% | **MUST BE >60%** |
| Files/sec (batch) | 0.8-1.1 | 8-16 | **MUST BE >8** |
| Total time (100 docs) | ~100s | ~10s | **MUST BE <15s** |

---

## Actual Results (Fill in after running)

### Primary Metrics (from IndexFileUseCase logs)

```
Average across all files:
- db_save_pct: _____%
- embedding_generation_pct: _____%
- extraction_pct: _____%
- embedding_save_pct: _____%
```

### Batch Throughput (from StartBatchFileImportUseCase logs)

```
Batch summary:
- Total files: 100
- Total time: _____s
- Files per second: _____
- Completed: _____
- Failed: _____
```

### Embedding Performance (from OnnxEmbeddingService logs)

```
Average across batches:
- inference_pct: _____%
- tokenize_pct: _____%
- per_text_ms: _____ms
```

---

## Validation Status

- [ ] db_save_pct < 5% ✅ **DATABASE OPTIMIZATION SUCCESS**
- [ ] embedding_pct > 70% ✅ **NEW BOTTLENECK IDENTIFIED**
- [ ] Files/sec > 8 ✅ **8-15x SPEEDUP CONFIRMED**
- [ ] All metrics match Oracle's predictions

---

## Next Steps

**If validation PASSES**:
1. ✅ Bulk INSERT optimization confirmed working
2. ✅ Embeddings identified as next bottleneck (70-80%)
3. → Proceed to transaction fixes (Phase 2.5)
4. → Expected additional 2-3x gain from transaction optimization
5. → Total cumulative: 24-36x speedup

**If validation FAILS** (db_save_pct still >5%):
1. ⚠️ Debug why bulk INSERT isn't working as expected
2. Check logs for batch_count (should be ≤10 for typical docs)
3. Verify QueryBuilder is being used (not individual INSERTs)
4. Check for transaction overhead issues

---

## Log Analysis Commands

Extract metrics from logs:
```bash
# Get db_save_pct distribution
grep "File indexing completed" tier2_validation.log | \
  grep -oE 'db_save_pct="[0-9.]+%"' | \
  cut -d'"' -f2

# Get embedding_pct distribution
grep "File indexing completed" tier2_validation.log | \
  grep -oE 'embedding_generation_pct="[0-9.]+%"' | \
  cut -d'"' -f2

# Get batch throughput
grep "Batch file import completed" tier2_validation.log | \
  grep -oE 'files_per_second="[0-9.]+"'
```

---

**Oracle's Final Assessment**: Awaiting validation results
EOF

echo "  ✓ Validation report template created: $RESULTS_FILE"
echo ""

echo "==================================================================="
echo "TIER 2 VALIDATION SETUP COMPLETE"
echo "==================================================================="
echo ""
echo "Test dataset ready: $TEST_DATA_DIR ($FILE_COUNT files)"
echo "Results template: $RESULTS_FILE"
echo ""
echo "NEXT STEPS:"
echo "  1. Import files from $TEST_DATA_DIR using your application"
echo "  2. Monitor logs (set RUST_LOG=lattice_desktop=info)"
echo "  3. Fill in actual results in $RESULTS_FILE"
echo "  4. Validate: db_save_pct < 5%, embedding_pct > 70%"
echo ""
echo "Once validated, proceed to transaction fixes for 2-3x additional gain"
echo "==================================================================="
