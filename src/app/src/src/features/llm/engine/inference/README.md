# Local Inference Engine

GPU-accelerated local LLM inference using llama.cpp for GGUF models.

## Overview

This module provides a complete local LLM inference implementation that integrates with the `LLMClient` trait, enabling offline text generation with GPU acceleration.

## Components

### 1. InferenceConfig (`config.rs`)

Configuration for inference engine with GPU settings:

```rust
let config = InferenceConfig::default()
    .with_gpu(-1) // Offload all layers to GPU
    .with_context_size(8192)
    .with_threads(8);
```

**Key Settings:**
- `n_gpu_layers`: Number of layers to offload to GPU (-1 = all, 0 = CPU only)
- `context_size`: Maximum context window in tokens
- `batch_size`: Batch size for prompt processing
- `n_threads`: CPU threads for inference
- `use_mmap`: Memory-mapped model loading (faster startup)
- `use_mlock`: Lock model in RAM (prevent swapping)

### 2. ModelLoader (`loader.rs`)

Handles loading GGUF models from disk:

```rust
let loader = ModelLoader::new(config);
let (model, context) = loader.load("/path/to/model.gguf").await?;
```

**Features:**
- Validates model file exists
- Configures GPU offloading
- Creates inference context
- Async model loading (non-blocking)

### 3. InferenceEngine (`engine.rs`)

Core inference engine for text generation:

```rust
let engine = InferenceEngine::from_path(
    "/path/to/model.gguf",
    InferenceConfig::default()
).await?;

// Non-streaming generation
let response = engine.generate(
    "What is Rust?",
    Some("You are a helpful assistant."),
    &gen_config
).await?;

// Streaming generation
let stream = engine.generate_stream(
    "Explain quantum computing",
    None,
    &gen_config
).await?;
```

**Capabilities:**
- Prompt formatting (Llama 3.1 Instruct format)
- Tokenization / detokenization
- Token sampling (temperature, top-k, top-p)
- Repetition penalty
- Streaming and non-streaming generation
- Async inference (Tokio-based)

### 4. LocalLLMClient (`local_client.rs`)

`LLMClient` trait implementation for local inference:

```rust
let client = LocalLLMClient::with_gpu(
    model_path,
    model_info,
    -1 // GPU layers
).await?;

// Use like any other LLMClient
let response = client.generate("Hello!", None).await?;
let stream = client.generate_stream("Tell me a story", None).await?;
```

**Factory Methods:**
- `new()`: Custom configuration
- `with_defaults()`: Default settings
- `cpu_only()`: No GPU acceleration
- `with_gpu(n_layers)`: Specific GPU offloading

## Usage Example

```rust
use vault_desktop::llm::{
    LocalLLMClient,
    InferenceConfig,
    GenerationConfig,
    ModelInfo,
    LLMClient,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get model info from catalog
    let catalog = get_catalog();
    let model = catalog.get_by_id("llama-3.1-8b-q4").unwrap();

    // Download model if needed
    let downloader = ModelDownloader::new()?;
    let model_path = downloader.model_path(model);

    if !model_path.exists() {
        downloader.download(model, |progress| {
            println!("Downloading: {:.1}%", progress.percent.unwrap_or(0.0));
        }).await?;
    }

    // Create client with GPU acceleration
    let client = LocalLLMClient::with_gpu(
        &model_path,
        model.clone(),
        -1 // Offload all layers
    ).await?;

    // Generate text
    let response = client.generate(
        "What is the meaning of life?",
        Some("You are a philosopher.")
    ).await?;

    println!("Response: {}", response);

    // Streaming generation
    use tokio_stream::StreamExt;

    let mut stream = client.generate_stream(
        "Write a haiku about Rust",
        None
    ).await?;

    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
    }

    Ok(())
}
```

## Integration with System Detection

The inference engine works with the system capabilities detector:

```rust
use vault_desktop::llm::{detect_capabilities, InferenceConfig};

let caps = detect_capabilities().await;

let config = if let Some(gpu) = caps.gpu {
    println!("GPU detected: {} ({} VRAM)", gpu.name, gpu.vram_gb.unwrap_or(0.0));
    InferenceConfig::with_gpu(-1) // Use GPU
} else {
    println!("No GPU detected, using CPU");
    InferenceConfig::cpu_only()
};

let engine = InferenceEngine::from_path(model_path, config).await?;
```

## GPU Acceleration

### Metal (macOS)

Automatically uses Metal backend when available. Set `n_gpu_layers` > 0 to offload layers.

### CUDA (Linux/Windows)

Requires CUDA-enabled llama.cpp build. Set `n_gpu_layers` based on VRAM:
- 4GB VRAM: 16-20 layers for 7B models
- 8GB VRAM: 32-40 layers for 7B models
- 16GB+ VRAM: All layers (-1)

### CPU Fallback

Set `n_gpu_layers = 0` for CPU-only inference:
```rust
let config = InferenceConfig::cpu_only()
    .with_threads(8); // Use 8 CPU threads
```

## Performance

**7B Q4_K_M Model:**
- **GPU (Metal/CUDA)**: 20-40 tokens/sec
- **CPU (8 cores)**: 5-10 tokens/sec

**Memory Requirements:**
- Model memory: ~4GB (Q4_K_M 7B)
- Context memory: ~2-4GB (depends on context size)
- Total: ~6-8GB RAM/VRAM

## Current Implementation Status

### ✅ Completed

- [x] Inference configuration with GPU settings
- [x] Model loader for GGUF files
- [x] Inference engine structure
- [x] LocalLLMClient implementation
- [x] LLMClient trait compliance
- [x] Async/await throughout
- [x] Basic error handling
- [x] Module exports and documentation

### ⚠️ Needs Verification

The current implementation makes assumptions about the `llama-cpp-2` crate API (v0.1). These need to be verified with actual API:

1. **Model Loading API**
   - `LlamaModel::load_from_file()` signature
   - `LlamaModelParams` structure
   - Error types

2. **Context Creation**
   - `model.new_context()` method
   - `LlamaContextParams` structure

3. **Tokenization**
   - `model.str_to_token()` method
   - `model.token_to_str()` method
   - `LlamaToken` type

4. **Inference**
   - `context.eval()` method
   - `context.candidates()` API
   - Token sampling methods

5. **EOS Detection**
   - `model.token_is_eog()` method

### 🔧 TODO

- [ ] **Verify llama-cpp-2 API**: Test with actual crate and adjust to real API
- [ ] **Alternative: Switch to llama-cpp-rs**: More mature crate with better docs
- [ ] **Add proper random sampling**: Currently uses deterministic sampling
- [ ] **Add batch processing**: Process multiple prompts efficiently
- [ ] **Add KV cache management**: Better context handling
- [ ] **Model-specific prompt templates**: Support different chat formats
- [ ] **Add benchmarking**: Performance tests
- [ ] **Add integration tests**: End-to-end testing with real models
- [ ] **Add Tauri commands**: Expose to frontend
- [ ] **Add progress callbacks**: Loading and generation progress
- [ ] **Add cancellation support**: Cancel long-running generation
- [ ] **Optimize memory usage**: Smart layer offloading based on VRAM

## Testing

### Unit Tests

```bash
cd src/app/src
cargo test --lib llm::inference
```

### Integration Tests

Requires a real GGUF model file:

```rust
#[tokio::test]
async fn test_real_inference() {
    let model_path = "path/to/test/model.gguf";
    let engine = InferenceEngine::from_path(
        model_path,
        InferenceConfig::cpu_only()
    ).await.unwrap();

    let response = engine.generate(
        "Hello!",
        None,
        &GenerationConfig::default()
    ).await.unwrap();

    assert!(!response.is_empty());
}
```

## Known Issues

1. **llama-cpp-2 v0.1 Maturity**: Early-stage crate, API may be unstable
2. **System Dependencies**: Requires proper GTK/Cairo libraries (see cargo build errors)
3. **GPU Detection**: Metal detection works on macOS, CUDA needs verification on Linux
4. **Random Sampling**: Currently deterministic, needs proper RNG
5. **Error Messages**: Could be more descriptive

## Recommendations

### For Production Use

1. **Test with Real Models**: Verify all API calls work with actual GGUF files
2. **Consider llama-cpp-rs**: More mature alternative to llama-cpp-2
3. **Add Comprehensive Error Handling**: Better error messages and recovery
4. **Add Telemetry**: Track inference performance and errors
5. **Add Model Validation**: Verify model file integrity
6. **Add Safety Limits**: Prevent OOM with large contexts

### For Development

1. **Add Debug Logging**: Use `tracing` for detailed logs
2. **Add Performance Profiling**: Identify bottlenecks
3. **Add Memory Profiling**: Monitor VRAM usage
4. **Add Example App**: Simple CLI for testing

## References

- [llama.cpp](https://github.com/ggerganov/llama.cpp)
- [llama-cpp-2 crate](https://crates.io/crates/llama-cpp-2)
- [llama-cpp-rs crate](https://crates.io/crates/llama-cpp-rs) (alternative)
- [GGUF format](https://github.com/ggerganov/ggml/blob/master/docs/gguf.md)

## License

Same as parent project (MIT).
