# Embedding parity and throughput

Embeddings run on the bundled llama.cpp server (design: `docs/design/2026-10-20-llama-cpp-embeddings.md`). Before that, they ran on Candle. This folder holds the vectors Candle produced, so the llama.cpp path can be checked against them now that Candle's embedding code is gone.

## Reference vectors

`reference/<catalog id>/` holds:

| File | Contents |
|---|---|
| `passages.json` | 200 passages: `[{"text": …, "tokens": …}]`. Every n-th chunk that Candle's `split_text` cut from the documents of `evals/retrieval/synthetic-library-v3.json`. `tokens` is Candle's token count for the chunk. |
| `queries.json` | 40 query texts: every n-th query of the same fixture, raw, without the query prefix. |
| `candle-metal-f16.f32` | Candle on Metal, F16 weights. |

The `.f32` file holds little-endian f32 rows of 1,024 floats each: the 200 passages first, then the 40 queries. Passages went through `embed_batch`, with no document prefix for Qwen3. Queries went through `embed_query`, which applies the `Instruct: … \nQuery:` prefix. Every vector is L2-normalised.

### How they were made

On 2026-10-20, from branch `design/llama-embeddings`, on an M3 Max, with the Qwen3-Embedding-0.6B safetensors that the app downloaded. The test was `qwen3_live_write_parity_reference` in `features/embedding/candle_service/qwen3_live_tests.rs`. That test was deleted along with Candle's embedding code; it stays in git history.

```sh
cd crates/lattice-desktop
export LATTICE_QWEN3_EMBEDDING_DIR=~/.cache/lattice/models/qwen3-embedding-0.6b
export LATTICE_EMBED_REFERENCE_DIR=../../evals/embedding/reference/qwen3-embedding-0.6b
LATTICE_EMBED_REFERENCE_NAME=candle-metal-f16 \
  cargo test --lib qwen3_live_write_parity_reference -- --ignored --nocapture
```

There is no CPU F32 reference. Candle on the CPU, in a test build, ran for 55 minutes on these 78,690 tokens without finishing, so the run was stopped. Every platform's llama.cpp F16 output is checked against the Metal F16 file: Vulkan and CPU on Windows and Linux, and Metal on Macs.

## Parity test

`features/embedding/llama_cpp/parity_tests.rs` (`#[ignore]`) starts the bundled server on the F16 GGUF and embeds the same texts. It then checks the result against every reference file present:
- every vector pair has cosine ≥ 0.9999;
- every query's top-1 passage agrees;
- the server's token count equals `tokens` for every passage.

Run it:
- on every sidecar version bump;
- on every change to a pinned embedding GGUF;
- on each new platform, for example Vulkan on Windows.

Record each run in `results/`.

## Throughput

`features/embedding/llama_cpp/throughput_bench.rs` (`#[ignore]`) measures:
- the model alone, at the index batch size and with the whole set per call;
- the passage index loop end to end;
- a sweep of the server's slot count.

Its results go in `results/` too.
