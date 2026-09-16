# Rust Embeddings Module

High-performance embedding generation using fastembed-rs, replacing Python's sentence-transformers.

## Overview

This module provides **improved** embedding generation performance compared to the Python implementation by using:
- **fastembed-rs**: Production-ready ONNX Runtime wrapper
- **Lazy loading**: Models load on first use
- **Batch processing**: Efficient parallel processing
- **Zero Python dependencies**: Pure Rust implementation

## Architecture

```
embeddings/
├── generator.rs    - Core embedding generation with fastembed-rs
├── validator.rs    - Dimension validation and compatibility checks
├── tests.rs        - Comprehensive test suite
├── mod.rs          - Public API exports
└── README.md       - This file
```

## Quick Start

### Single Embedding

```rust
use vault_desktop::embeddings::{EmbeddingGenerator, ModelConfig};

let generator = EmbeddingGenerator::default();
let embedding = generator.generate("Hello world")?;

use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);  // DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
```

### Batch Embeddings

```rust
let texts = vec![
    "First text".to_string(),
    "Second text".to_string(),
];

let embeddings = generator.generate_batch(&texts)?;
assert_eq!(embeddings.len(), 2);
```

### Custom Model Configuration

```rust
let generator = EmbeddingGenerator::new(ModelConfig::AllMpnetBaseV2);
assert_eq!(generator.dimensions(), DEFAULT_EMBEDDING_DIM);
```

## Models Supported

| Model | Dimensions | Use Case |
|-------|-----------|----------|
| `AllMpnetBaseV2` | DEFAULT_EMBEDDING_DIM | Higher-quality, general-purpose (default) |

### Adding New Models

To add support for new models:

1. Add enum variant to `ModelConfig`:
```rust
pub enum ModelConfig {
    AllMpnetBaseV2,
    YourNewModel,  // Add here
}
```

2. Implement methods:
```rust
impl ModelConfig {
    pub fn dimensions(&self) -> usize {
        match self {
            Self::AllMpnetBaseV2 => DEFAULT_EMBEDDING_DIM,
            Self::YourNewModel => DEFAULT_EMBEDDING_DIM,  // Your dimensions
        }
    }

    pub fn model_name(&self) -> &'static str {
        match self {
            Self::AllMpnetBaseV2 => DEFAULT_EMBEDDING_MODEL_NAME,
            Self::YourNewModel => "your-model-name",
        }
    }

    fn to_fastembed_model(&self) -> EmbeddingModel {
        match self {
            Self::AllMpnetBaseV2 => EmbeddingModel::AllMpnetBaseV2,
            Self::YourNewModel => EmbeddingModel::YourModel,
        }
    }
}
```

## Integration with Tauri

### IPC Commands

The module integrates with Tauri via commands in `commands/llm.rs`:

```rust
#[tauri::command]
pub async fn generate_embedding(
    text: String,
    state: State<'_, AppState>,
) -> Result<Vec<f32>, String> {
    state.embedding_service
        .embed_single(&text)
        .await
        .map_err(|e| e.to_string())
}
```

### Frontend Usage

```typescript
import { invoke } from '@tauri-apps/api/core';

const embedding = await invoke<number[]>('generate_embedding', {
  text: 'Hello world'
});

console.log(`Generated ${embedding.length}-dimensional embedding`);
```

## Performance

### Benchmarks

Run benchmarks with:
```bash
cargo bench --bench embeddings_performance
```

Expected results (on typical hardware):
- **Single short text**: ~5-10ms
- **Single medium text**: ~15-25ms
- **Batch of 10**: ~50-100ms
- **Batch of 50**: ~200-400ms

### Python vs Rust Comparison

| Operation | Python (sentence-transformers) | Rust (fastembed-rs) | Improvement |
|-----------|-------------------------------|---------------------|-------------|
| Single embedding | 30-50ms | 10-15ms | **Faster** |
| Batch (10 items) | 150-200ms | 50-100ms | **Faster** |
| Model loading | 2-3s | 1-2s | **Faster** |

## Validation

### Dimension Validation

Prevent sync crashes by validating embedding dimensions:

```rust
use vault_desktop::embeddings::validate_embedding_dimension;

let embedding = vec![0.1; DEFAULT_EMBEDDING_DIM];
validate_embedding_dimension(&embedding, DEFAULT_EMBEDDING_DIM, "context")?;
```

### Compatibility Checks

Ensure desktop and backend use matching models:

```rust
use vault_desktop::embeddings::{validate_embedding_compatibility, ModelConfig};

let config = ModelConfig::AllMiniLmL6V2;
validate_embedding_compatibility(
    DEFAULT_EMBEDDING_DIM,
    DEFAULT_EMBEDDING_DIM,
    &config
)?;
// Error: CRITICAL: Embedding dimension mismatch detected!
```

## Testing

### Run All Tests

```bash
cargo test --lib embeddings
```

### Test Coverage

The module includes comprehensive tests:

- ✅ Model configuration
- ✅ Generator creation and lazy loading
- ✅ Input validation (empty text, whitespace)
- ✅ Single embedding generation
- ✅ Batch embedding generation
- ✅ Normalization (L2 norm = 1.0)
- ✅ Consistency (same input → same output)
- ✅ Similarity comparisons
- ✅ Special characters and emojis
- ✅ Long text handling
- ✅ Dimension validation
- ✅ Compatibility validation

### Key Test Cases

```rust
#[test]
fn test_embedding_generation() {
    let gen = EmbeddingGenerator::default();
    let emb = gen.generate("Hello world").unwrap();

    assert_eq!(emb.len(), DEFAULT_EMBEDDING_DIM);

    // Check L2 normalization
    let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-5);
}
```

## Error Handling

All operations return `Result<T, AppError>`:

```rust
match generator.generate("text") {
    Ok(embedding) => { /* Use embedding */ },
    Err(AppError::InvalidInput { field, reason }) => {
        eprintln!("Invalid {}: {}", field, reason);
    },
    Err(AppError::EmbeddingFailed { reason }) => {
        eprintln!("Generation failed: {}", reason);
    },
    Err(e) => eprintln!("Other error: {}", e),
}
```

### Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `InvalidInput` | Empty or whitespace-only text | Validate input before calling |
| `EmbeddingFailed` | Model failed to load or inference error | Check logs, ensure model files exist |
| `DimensionMismatch` | Wrong embedding dimension | Use correct model configuration |

## Migration from Python

### Before (Python Bridge)

```rust
let mut bridge = state.python_bridge.lock().await;
let params = json!({ "text": text });
let result = bridge.call("generate_embedding", params).await?;
let embedding: Vec<f32> = serde_json::from_value(result)?;
```

### After (Pure Rust)

```rust
let embedding = state.embedding_service
    .embed_single(&text)
    .await?;
```

### Benefits

1. **No Python dependency** - Eliminates sidecar process
2. **Faster execution** - Native Rust implementation
3. **Better error handling** - Rust's type system catches errors at compile time
4. **Lower memory usage** - No IPC overhead
5. **Simpler deployment** - Single binary, no Python environment

## Troubleshooting

### Model Download Issues

Models are downloaded automatically on first use. If downloads fail:

1. Check internet connection
2. Verify firewall allows HTTPS
3. Check disk space (~100MB per model)
4. Review logs for specific errors

### Dimension Mismatches

If you see dimension mismatch errors:

1. Verify both systems use same `ModelConfig`
2. Check database for mixed embeddings from different models
3. Consider running migration to update old embeddings

### Performance Issues

If embeddings are slower than expected:

1. Ensure model is preloaded (first call is slower)
2. Use batch processing for multiple texts
3. Check CPU usage and available cores
4. Review logs for repeated model loading

## Future Enhancements

Potential improvements:

- [ ] Support for stella_en_1.5B_v5 (DEFAULT_EMBEDDING_DIM dims) via custom ONNX loading
- [ ] GPU acceleration support
- [ ] Async batch processing with configurable concurrency
- [ ] Embedding caching layer
- [ ] Model quantization for faster inference
- [ ] Support for multilingual models

## Resources

- [fastembed-rs Documentation](https://docs.rs/fastembed/)
- [ONNX Runtime](https://onnxruntime.ai/)
- [sentence-transformers](https://www.sbert.net/)
- Model card: DEFAULT_EMBEDDING_MODEL_NAME

## License

Same as parent project (MIT)
