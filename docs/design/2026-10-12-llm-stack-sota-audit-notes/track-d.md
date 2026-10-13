# Track D: chat catalog refresh

## Changes

The chat catalog in `curated.rs` is now a `CuratedChatModel` table. Every entry is one Unsloth GGUF with a pinned size and SHA-256. Downloads now use the `files` path, which verifies the digest; the old single-file path never did. Memory is weights plus the KV cache read from the GGUF header plus 1 GB of runtime, at an 8K window (min) and a 32K window (comf).

| id | file | bytes | sha256 | min/comf GB |
|---|---|---|---|---|
| qwen3.5-2b-q4_k_m | Qwen3.5-2B-Q4_K_M | 1,280,835,840 | aaf42c8b… | 2.4/2.7 |
| qwen3.5-4b-q4_k_m | Qwen3.5-4B-Q4_K_M | 2,740,937,888 | 00fe7986… | 4.1/4.9 |
| gemma-4-e4b-it-q4_k_m | gemma-4-E4B-it-Q4_K_M | 4,977,171,584 | 85a896a0… | 6.2/6.6 |
| qwen3.5-9b-q4_k_m | Qwen3.5-9B-Q4_K_M | 5,680,522,464 | 03b74727… | 7.0/7.8 |
| qwen3.5-9b-mtp-q4_k_m | (9B-MTP repo) Qwen3.5-9B-Q4_K_M | 5,868,826,976 | e8dd9481… | 7.2/8.1 |
| gemma-4-12b-it-q4_k_m | gemma-4-12b-it-Q4_K_M | 7,121,861,440 | 0a270ec9… | 8.8/9.2 |
| qwen3.8-27b-ud-q4_k_m | Qwen3.8-27B-UD-Q4_K_M | 16,464,440,224 | 322e194f… | 18.1/19.8 |
| gemma-4-26b-a4b-it-ud-q4_k_m | gemma-4-26B-A4B-it-UD-Q4_K_M | 16,947,541,728 | f2c28b3d… | 18.5/19.0 |
| qwen3.6-35b-a3b-ud-q4_k_xl | Qwen3.6-35B-A3B-UD-Q4_K_XL | 22,360,456,160 | 707a55a8… | 23.6/24.1 |

- **Removed:** TinyLlama, Phi-3/3.5, Mistral 7B, Llama 3.1/3.2 and Mixtral.
- **27B and 26B use UD-Q4_K_M** because those repos publish no plain Q4_K_M.
- **Catalog browser** (`model_management/commands.rs`) now shows catalog entries, replacing Hugging Face rows for the same repository; the version list returns the catalog entry.
- **First run** now uses exact byte sizes.

**MTP:**
- Field `CuratedChatModel::mtp_draft: bool`; only the 9B MTP entry sets it.
- Read it with `chat_model_uses_mtp_draft(&Path)`. It matches the `models/<id>/<file>` path, because the 9B and the MTP build share a file name.
- Qwen 3.8 27B's GGUF also has an MTP layer but is not flagged.

**Thinking defaults (read from each GGUF's template):**
- Off: Qwen 3.5 2B/4B/9B and Gemma 4.
- **On:** 9B-MTP, 3.6 and 3.8.
- Qwen 3.8 raises an error on any `reasoning_effort` other than xhigh, high, medium or low.
- Send `enable_thinking` explicitly.

## Fit on Apple Silicon

`computeModelFit` verdicts. First run: 2B below 8 GB, 4B to 16 GB, 9B above.

| RAM | Fits | Tight | Too large |
|---|---|---|---|
| 8 | 2B, 4B | none | the rest |
| 16, 24 | up to Gemma 12B | none | 27B, 26B, 35B |
| 32 | up to Gemma 12B | 27B, 26B | 35B-A3B |
| 36 | all but 35B | 35B-A3B | none |
| 64 | all | none | none |

**The 35B-A3B needs 36 GB, not 32:** its 22.4 GB of weights fill the app's whole 70% budget on 32 GB.

## Verified
- Each file's size and digest from the HF tree API (`lfs.oid`).
- Every resolve URL: HEAD 302 then 200, `x-linked-size`/`x-linked-etag` matching.
- GGUF headers via HTTP range requests: architecture, context, KV shape, MTP layers, templates.
- The pinned binary contains the gemma4 chat parser and the laguna model code.
- `cargo check --lib`, `cargo test --lib` (116 pass), `cargo fmt`, `tsc` and vitest are clean. Only the bin's `generate_context!` fails (no frontend build here).

## Laguna: not added
- Laguna XS 2.1 is a coding-agent model (33B mixture-of-experts, 3B active, OpenMDW license).
- Its own model card shows Qwen3.6-35B-A3B ahead on all four of its benchmarks, at the same memory cost.
- The card warns of empty output on Metal; the fix (PR #25389) was closed unmerged.
- No grounding evidence.

## Not verified
- Nothing was run.
- Unmeasured: MTP speed-up, Gemma 4 runtime tool calls, Metal working-set limits.
- The UD-Q4_K_XL quality data (KL divergence) is for Qwen 3.5 35B-A3B, not 3.6.

## Risks
- **Sidecar context sizing** (`gguf_metadata::kv_cache_bytes_per_token`): it counts every layer as full attention, about 4x too high for Qwen hybrids, and it stops reading at Gemma 4's per-layer KV-head array. The sidecar will choose smaller windows than these figures allow.
- **Router default** (`settings.rs`, Track A) names the removed `phi-3.5-mini-instruct-q4_k_m`.
- **Placeholder:** an empty, git-ignored `binaries/llama-server-aarch64-apple-darwin` lets `tauri-build` run.

## Commit
```
Refresh the chat catalog to pinned, current-generation GGUFs

The catalog offered pre-Qwen-3.5 models, overstated memory and
downloaded without a digest. Entries are now Qwen 3.5/3.6/3.8 and
Gemma 4 files pinned by size and SHA-256, with memory from each GGUF
header so the fit badge is true. Adds a 9B MTP build (mtp_draft) and
lists catalog entries in the model browser.
```
