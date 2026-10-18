# `infrastructure::ml`

Shared model infrastructure lives here. `instance_pool.rs` holds loaded LLM
instances keyed by what they are, any number at once: one load per key at a
time, leases for work that pins an instance, invalidation that leaves holders
their instance, and least-recently-used eviction of idle ones. `model_cache.rs`
provides single-flight loading and invalidation-safe publication for the
embedding port. Their unit tests live alongside them.

The embedding engine and its loading lifecycle live in
`crates/lattice-desktop/src/features/embedding/` (`loading.rs` and `runtime.rs`).
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

Fresh installs select their embedding model through `default_embedding_model`
in `crates/lattice-core/src/domain/models/embedding_defaults.rs`: machines with an
available GPU backend get `Qwen/Qwen3-Embedding-0.6B` (1024 dimensions), while
CPU-only machines use `sentence-transformers/all-MiniLM-L6-v2` (384 dimensions).
The MiniLM constants remain the fallback when no loaded model can report its
identity or dimensions. Catalog downloads are stored under
`~/.cache/lattice/models/<model_id>/`.
