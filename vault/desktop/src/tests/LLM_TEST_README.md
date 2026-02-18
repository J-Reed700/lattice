# LLM Integration Test Suite

Comprehensive integration tests for the local LLM inference system in Recall Desktop.

## Overview

This test suite validates the complete LLM inference pipeline, from system capability detection to model recommendation, download, and inference.

### Test Files

```
vault/desktop/src-tauri/tests/
├── llm_integration_test.rs          # Main integration tests (36 tests)
├── llm_performance_benchmarks.rs    # Performance benchmarks (11 tests)
├── LLM_TEST_COVERAGE.md             # Detailed coverage analysis
├── LLM_TEST_README.md               # This file
└── helpers/
    ├── mod.rs
    └── llm_helpers.rs                # Test utilities and fixtures
```

## Quick Start

### Run All Non-Ignored Tests

These tests don't require external dependencies (Ollama):

```bash
cd vault/desktop/src-tauri

# Run integration tests
cargo test --test llm_integration_test

# Run performance benchmarks
cargo test --test llm_performance_benchmarks
```

### Run With Ollama (Full Integration)

These tests require Ollama to be running locally:

```bash
# Start Ollama (in separate terminal)
ollama serve

# Pull a model
ollama pull llama2

# Run all tests including Ollama-dependent ones
cargo test --test llm_integration_test -- --ignored
cargo test --test llm_performance_benchmarks -- --ignored --nocapture
```

### Run Specific Test Categories

```bash
# System integration tests
cargo test --test llm_integration_test system_integration

# Trait polymorphism tests
cargo test --test llm_integration_test trait_polymorphism

# Error handling tests
cargo test --test llm_integration_test error_handling

# Streaming tests
cargo test --test llm_integration_test streaming

# Performance tests
cargo test --test llm_integration_test performance

# Concurrency tests
cargo test --test llm_integration_test concurrency
```

## Test Categories

### 1. System Integration Tests (8 tests)

**Purpose**: Test the complete system capability detection and model recommendation workflow.

**Tests**:
- `test_capabilities_detection` - Verify RAM, CPU, GPU detection
- `test_capabilities_summary` - Test human-readable summary generation
- `test_recommendation_workflow` - End-to-end recommendation flow
- `test_low_ram_system_recommendations` - 4GB system filtering
- `test_high_ram_gpu_system_recommendations` - 32GB+GPU recommendations
- `test_best_model_selection` - Best model algorithm
- `test_full_workflow_detection_to_recommendation` - Complete workflow
- `test_system_capabilities_json_serialization` - JSON roundtrip

**External Dependencies**: None

**Run**: `cargo test --test llm_integration_test system_integration`

---

### 2. Trait Polymorphism Tests (4 tests)

**Purpose**: Verify that LLMClient trait enables polymorphic usage of different backends.

**Tests**:
- `test_ollama_client_through_trait` - OllamaClient via trait
- `test_multiple_clients_through_trait` - Multiple instances
- `test_generation_config_defaults` - Default config values
- `test_generation_config_modification` - Config mutation

**External Dependencies**: Ollama (ignored tests)

**Run**: `cargo test --test llm_integration_test trait_polymorphism -- --ignored`

---

### 3. Error Handling Tests (4 tests)

**Purpose**: Validate error handling for various failure scenarios.

**Tests**:
- `test_ollama_connection_error` - Server unavailable
- `test_invalid_url` - Malformed URL handling
- `test_insufficient_ram_filtering` - RAM constraint enforcement
- `test_generate_with_invalid_model` - Non-existent model error

**External Dependencies**: Ollama (some tests)

**Run**: `cargo test --test llm_integration_test error_handling`

---

### 4. Streaming Tests (3 tests)

**Purpose**: Validate streaming text generation functionality.

**Tests**:
- `test_streaming_basic` - Basic stream consumption
- `test_streaming_vs_non_streaming` - Compare modes
- `test_stream_cancellation` - Early termination

**External Dependencies**: Ollama (ignored)

**Run**: `cargo test --test llm_integration_test streaming -- --ignored`

---

### 5. Performance Tests (3 tests)

**Purpose**: Measure inference latency and throughput.

**Tests**:
- `test_inference_latency` - Cold start latency
- `test_warm_cache_performance` - Warm vs cold
- `test_recommendation_performance` - Recommendation speed

**External Dependencies**: Ollama (some tests)

**Run**: `cargo test --test llm_integration_test performance`

---

### 6. Concurrency Tests (3 tests)

**Purpose**: Verify thread safety and concurrent request handling.

**Tests**:
- `test_concurrent_requests` - 5 parallel requests
- `test_concurrent_streaming` - 3 parallel streams
- `test_concurrent_capability_detection` - Thread-safe detection

**External Dependencies**: Ollama (some tests)

**Run**: `cargo test --test llm_integration_test concurrency`

---

### 7. Performance Benchmarks (11 tests)

**Purpose**: Detailed performance characterization and regression detection.

**Benchmarks**:

#### Latency
- `benchmark_cold_start_latency` - First request latency
- `benchmark_warm_latency` - Subsequent request latency (5 samples)
- `benchmark_time_to_first_token` - Streaming TTFT

#### Throughput
- `benchmark_tokens_per_second` - Generation throughput
- `benchmark_concurrent_throughput` - Requests/sec with 10 parallel

#### Memory & Overhead
- `benchmark_capability_detection_overhead` - Detection time (100 iterations)
- `benchmark_recommendation_overhead` - Recommendation time (1000 iterations)

#### Scalability
- `benchmark_queue_depth_scalability` - Queue depths: 1, 2, 5, 10, 20
- `benchmark_streaming_vs_non_streaming` - Mode comparison

#### Stress Tests
- `stress_test_sustained_load` - 50 sequential requests
- `benchmark_mock_vs_real_capabilities` - Mock vs real detection

**External Dependencies**: Ollama (all benchmarks)

**Run**: `cargo test --test llm_performance_benchmarks -- --ignored --nocapture`

---

## Test Helpers

### `helpers/llm_helpers.rs`

Provides reusable utilities for testing:

#### Mock System Creation
```rust
use helpers::llm_helpers::*;

// Create mock systems
let low_end = create_low_end_system();        // 4GB, no GPU
let mid_range = create_mid_range_system();    // 16GB, GPU
let high_end = create_high_end_system();      // 32GB, RTX 4090

// Custom mock
let custom = create_mock_capabilities(16.0, true);
```

#### Service Availability
```rust
// Check if Ollama is available
if is_ollama_available().await {
    // Run test
}

// Get test client
if let Some(client) = get_test_ollama_client().await {
    // Use client
}

// Check GPU availability
if is_gpu_available() {
    // Run GPU test
}
```

#### Test Prompts & Fixtures
```rust
use helpers::llm_helpers::prompts::*;
use helpers::llm_helpers::system_prompts::*;
use helpers::llm_helpers::model_names::*;

let request = OllamaGenerateRequest::new(LLAMA_2, prompts::SIMPLE);
```

#### Assertions
```rust
assert_valid_capabilities(&caps);
assert_valid_score(0.85);
```

#### Performance Thresholds
```rust
use helpers::llm_helpers::thresholds::*;

assert!(duration < thresholds::COLD_START_MAX);
assert!(duration < thresholds::WARM_REQUEST_MAX);
```

#### Conditional Test Skipping
```rust
#[tokio::test]
async fn my_test() {
    skip_if_no_ollama!();
    skip_if_no_gpu!();
    skip_if_no_test_model!();

    // Test code...
}
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: LLM Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      # Run fast unit tests (no external dependencies)
      - name: Run Unit Tests
        run: |
          cd vault/desktop/src-tauri
          cargo test --test llm_integration_test

  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      # Setup Ollama
      - name: Setup Ollama
        run: |
          docker run -d -p 11434:11434 ollama/ollama
          sleep 5
          docker exec ollama ollama pull llama2

      # Run integration tests
      - name: Run Integration Tests
        run: |
          cd vault/desktop/src-tauri
          cargo test --test llm_integration_test -- --ignored
          cargo test --test llm_performance_benchmarks -- --ignored

      # Collect performance metrics
      - name: Archive Performance Results
        uses: actions/upload-artifact@v3
        with:
          name: performance-results
          path: target/criterion/
```

---

## Test Coverage Summary

| Component | Coverage | Tests | Status |
|-----------|----------|-------|--------|
| System Capabilities | 95% | 8 | ✅ Excellent |
| Model Recommendation | 95% | 7 | ✅ Excellent |
| LLMClient Trait | 80% | 4 | ✅ Good |
| OllamaClient | 75% | 11 | ✅ Good |
| Error Handling | 70% | 4 | ✅ Medium |
| Streaming | 75% | 3 | ✅ Good |
| Performance | 90% | 11 | ✅ Excellent |
| Concurrency | 80% | 3 | ✅ Good |
| **Total** | **83%** | **51** | **✅ Good** |

**Gaps**:
- ❌ Model Downloader (0% coverage) - HIGH PRIORITY
- ⚠️ LocalLLMClient (not implemented yet)
- ⚠️ Timeout handling
- ⚠️ Network interruption recovery

See [LLM_TEST_COVERAGE.md](./LLM_TEST_COVERAGE.md) for detailed analysis.

---

## Test Development Guidelines

### Adding New Tests

1. **Choose the right test file**:
   - Integration tests → `llm_integration_test.rs`
   - Performance benchmarks → `llm_performance_benchmarks.rs`

2. **Use appropriate module**:
   ```rust
   mod my_category {
       use super::*;

       #[test]
       fn test_something() {
           // Test code
       }
   }
   ```

3. **Mark external dependencies**:
   ```rust
   #[tokio::test]
   #[ignore] // Requires Ollama
   async fn test_with_ollama() {
       skip_if_no_ollama!();
       // Test code
   }
   ```

4. **Use test helpers**:
   ```rust
   use helpers::llm_helpers::*;

   let caps = create_high_end_system();
   assert_valid_capabilities(&caps);
   ```

### Test Naming Conventions

- `test_*` - Standard unit/integration tests
- `benchmark_*` - Performance benchmarks
- `stress_test_*` - Stress/load tests

### Performance Test Guidelines

1. **Warm up before measurement**:
   ```rust
   let warmup = OllamaGenerateRequest::new("llama2", prompts::SIMPLE);
   let _ = client.generate(warmup).await;
   ```

2. **Use `--nocapture` for benchmark output**:
   ```bash
   cargo test benchmark_* -- --ignored --nocapture
   ```

3. **Collect statistics for variable operations**:
   ```rust
   let mut latencies = Vec::new();
   for _ in 0..iterations {
       let start = Instant::now();
       // Operation
       latencies.push(start.elapsed());
   }
   let avg = latencies.iter().sum::<Duration>() / iterations;
   ```

---

## Debugging Tests

### Enable Logging

```bash
# Set log level
RUST_LOG=debug cargo test --test llm_integration_test -- --nocapture

# Specific module
RUST_LOG=vault_desktop::llm=trace cargo test
```

### Run Single Test

```bash
# Run specific test
cargo test --test llm_integration_test test_capabilities_detection -- --exact

# With output
cargo test --test llm_integration_test test_capabilities_detection -- --exact --nocapture
```

### Debug Ollama Connection

```bash
# Check Ollama is running
curl http://localhost:11434/api/tags

# Check specific model
ollama list

# Pull model if needed
ollama pull llama2
```

---

## Performance Expectations

### Thresholds

| Metric | Threshold | Notes |
|--------|-----------|-------|
| Cold Start | < 30s | Includes model loading |
| Warm Request | < 10s | Model already loaded |
| TTFT | < 5s | Time to first token |
| Tokens/sec | > 10 | Conservative minimum |
| Capability Detection | < 500ms | Should be very fast |
| Recommendation | < 100ms | Should be instant |

### Typical Results (Example System)

**System**: 16GB RAM, NVIDIA RTX 3060, Ryzen 7 5800X

| Benchmark | Result |
|-----------|--------|
| Cold Start | 8.2s |
| Warm Latency (avg) | 1.4s |
| TTFT | 850ms |
| Tokens/sec | 45.3 |
| Concurrent Throughput | 3.2 req/sec |
| Capability Detection | 12ms |
| Recommendation | 0.8ms |

---

## Troubleshooting

### Tests Timeout

**Problem**: Tests hang or timeout

**Solutions**:
- Check Ollama is running: `curl http://localhost:11434/api/tags`
- Increase timeout: Edit `OllamaClient::new()` timeout parameter
- Check system resources: `htop`, `nvidia-smi`

### Tests Flaky

**Problem**: Tests pass sometimes, fail other times

**Solutions**:
- Check concurrent access to Ollama
- Increase warm-up iterations
- Check system load (CPU/memory)
- Use `--test-threads=1` to run serially

### Model Not Found

**Problem**: `test_generate_with_invalid_model` or similar fails unexpectedly

**Solutions**:
- Pull required models: `ollama pull llama2`
- Check model name matches: `ollama list`

### Compilation Errors

**Problem**: Tests don't compile

**Solutions**:
- Check Rust version: `rustc --version` (need 1.70+)
- Update dependencies: `cargo update`
- Clean build: `cargo clean && cargo build`

---

## Future Enhancements

### Planned Test Additions

1. **Model Downloader Tests** (HIGH PRIORITY)
   - Mock HTTP responses
   - Progress tracking validation
   - Cancellation and resume

2. **LocalLLMClient Tests** (When implemented)
   - Model loading from file
   - GPU acceleration verification
   - Inference without Ollama

3. **Advanced Error Scenarios**
   - Timeout handling
   - Network interruptions
   - OOM conditions

4. **Platform-Specific Tests**
   - Apple Silicon (Metal)
   - AMD GPU (ROCm)
   - ARM architecture

### Maintenance

- **Weekly**: Review test pass rates in CI
- **Monthly**: Update performance baselines
- **Per Release**: Run full stress test suite
- **When Adding Features**: Add corresponding tests

---

## Contributing

When adding LLM features:

1. ✅ Add integration tests to `llm_integration_test.rs`
2. ✅ Add performance benchmarks if applicable
3. ✅ Update `LLM_TEST_COVERAGE.md`
4. ✅ Add helpers to `llm_helpers.rs` if reusable
5. ✅ Ensure tests pass both with and without Ollama
6. ✅ Document any new test categories in this README

---

## Resources

- [Ollama Documentation](https://github.com/jmorganca/ollama)
- [tokio Testing Guide](https://tokio.rs/tokio/topics/testing)
- [Rust Test Documentation](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Criterion.rs Benchmarking](https://bheisler.github.io/criterion.rs/book/)

---

**Last Updated**: 2025-11-15
**Test Suite Version**: 1.0
**Total Tests**: 51 (36 integration + 11 benchmarks + 4 helpers)
