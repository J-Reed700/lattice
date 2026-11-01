# `infrastructure::ml`

Shared model infrastructure lives here:

| File | What it is | Who uses it |
|------|------------|-------------|
| `instance_pool.rs` | Loaded LLM instances keyed by what they are, any number at once: one load per key at a time, leases for work that pins an instance, invalidation that leaves holders their instance, and least-recently-used eviction of idle ones. | `features/llm/instances.rs` |
| `model_cache.rs` | Single-flight loading and invalidation-safe publication; a dead value is a miss. | the embedding runtime (`features/embedding/runtime.rs`) |
| `tokenizer.rs` | Loads a Hugging Face `tokenizer.json`. | the embedding splitter (`features/embedding/llama_cpp/`) and the Qwen3 reranker |
| `compute_device.rs` | Picks the Candle device: Metal first, then CPU. | the cross-encoder reranker and Whisper |
| `artifact_identity.rs` | The SHA-256 of an embedding GGUF, which names its vector space. | activation, download, and the standalone embedding server |

Their unit tests live alongside them.

## Where embeddings come from

Embeddings run on a second bundled llama-server, started with `--embedding`
for the active GGUF (`features/embedding/llama_cpp/`, started through
`interfaces/di/modules/embedding.rs`). The model's dimension, pooling and
trained window come from its GGUF header; passages are split with the
`tokenizer.json` beside the GGUF. Tools that run without the app (the evals and
the ignored live tests) start their own server through
`interfaces/di/modules/standalone.rs`.

The reranker runs the same way, on a `llama-server --reranking`
(`features/search/engine/llama_reranker/`). Candle stays a dependency for
Whisper and for the checkpoint rerankers `retrieval_eval` can still load; no
embedding runs on it.

Fresh installs select their embedding model through `default_embedding_model`
in `crates/lattice-core/src/domain/models/embedding_defaults.rs`. Catalog
downloads are stored under `~/.cache/lattice/models/<model_id>/`.
