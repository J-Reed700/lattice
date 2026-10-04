# `infrastructure::ml`

Shared model infrastructure lives here. `model_cache.rs` provides single-flight
loading and invalidation-safe publication for both LLM and embedding ports.
Its unit tests live alongside the implementation.

The embedding engine and its loading lifecycle live in
`src-tauri/src/features/embedding/` (`loading.rs` and `runtime.rs`).
`infrastructure/ml/mod.rs` also re-exports a few embedding types:

| Re-export | Defined in `features/embedding/` |
|-----------|----------------------------------|
| `CandleEmbeddingService` | `candle_service.rs` |
| `EmbeddingGenerator`, `ModelConfig` | `generator.rs` |
| `RemoteEmbeddingService`, `DEFAULT_REMOTE_EMBEDDING_MODEL`, `DEFAULT_REMOTE_EMBEDDING_URL` | `remote_service.rs` |
| `validate_embedding_dimension`, `DimensionMismatchError` | `validator.rs` |

New embedding consumers should import from `crate::features::embedding` directly.

## Where embeddings actually come from

- **Local**: `CandleEmbeddingService` runs BERT-family checkpoints (BERT,
  DistilBERT, XLM-RoBERTa, MPNet, Jina v2, Nomic, ModernBERT) and Qwen3
  embedding models on Candle, Metal first with CPU fallback. Output dimension
  comes from the model's `config.json` `hidden_size`; vectors are L2-normalized.
  fastembed and ONNX Runtime are no longer dependencies.
- **Remote**: `RemoteEmbeddingService` calls an OpenAI-compatible
  `/v1/embeddings` endpoint (default `text-embedding-3-small`).
- `EmbeddingGenerator` is a legacy Candle-backed shim kept for the
  `generate_embedding` / `generate_embeddings_batch` commands of the
  `embeddings` Tauri plugin (`features/embedding/plugin.rs`).

The built-in default model is `sentence-transformers/all-MiniLM-L6-v2` (384
dims), defined in `src-tauri/src/domain/models/embedding_defaults.rs`. Model files
are stored under `~/.cache/lattice/models/<model_id>/`.
