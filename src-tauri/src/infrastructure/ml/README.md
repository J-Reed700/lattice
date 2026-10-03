# `infrastructure::ml` (re-export shim)

This module holds no embedding code. The embedding engine lives in
`src-tauri/src/features/embedding/`, and `infrastructure/ml/mod.rs` only
re-exports a few of its types under the old `crate::infrastructure::ml::*` path:

| Re-export | Defined in `features/embedding/` |
|-----------|----------------------------------|
| `CandleEmbeddingService` | `candle_service.rs` |
| `EmbeddingGenerator`, `ModelConfig` | `generator.rs` |
| `RemoteEmbeddingService`, `DEFAULT_REMOTE_EMBEDDING_MODEL`, `DEFAULT_REMOTE_EMBEDDING_URL` | `remote_service.rs` |
| `validate_embedding_dimension`, `DimensionMismatchError` | `validator.rs` |

No code in `src-tauri/src` or `src-tauri/tests` imports through this path today;
new code should import from `crate::features::embedding` directly.

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
dims), defined in `src-tauri/src/domain/embedding_constants.rs`. Model files
are stored under `~/.cache/lattice/models/<model_id>/`.

## Note

`tests.rs` in this directory is not declared by `mod.rs`, so it is never
compiled. It targets the removed fastembed API (`crate::embeddings`,
`ModelConfig::AllMpnetBaseV2`).
