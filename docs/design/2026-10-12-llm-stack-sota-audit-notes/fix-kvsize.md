# Fix: sidecar KV sizing overcounted hybrids and lost Gemma 4's header

Branch `fix/kvsize` (from the integration branch at 0f19c85a), uncommitted.

Defect: track D, "Risks". `gguf_metadata::kv_cache_bytes_per_token` charged every layer as full attention. That is 4x too high for the Qwen 3.5/3.6/3.8 hybrids. The header reader also gave up at Gemma 4's per-layer `attention.head_count_kv` list. It then lost the key widths and the sliding window and fell back to 16 heads 240 wide on all 48 layers: 720 KiB a token for Gemma 4 12B instead of 16 KiB plus a bounded sliding cache. The sidecar picked windows far smaller than memory allowed.

## What changed

### `crates/lattice-desktop/src/features/llm/engine/gguf_metadata.rs`

- `:42` new `PerLayer { Uniform(u32), Layers(Vec<u32>) }`. A head count is one number or one per layer, as llama.cpp's `get_key_or_arr` allows.
- `:65` `GgufModelInfo`: `head_count` and `head_count_kv` are now `Option<PerLayer>`. New fields: `key_length_swa`, `value_length_swa`, `sliding_window`, `sliding_window_pattern` (per-layer flags), `full_attention_interval`, `nextn_predict_layers`, `shared_kv_layers`.
- `:153` `kv_cache_bytes(context_tokens) -> Option<u64>` replaces `kv_cache_bytes_per_token`. The cost is no longer linear in the window, so the API prices a window.
- `:179` `layer_caches` lists the layers that keep a cache of their own and what a token of each costs. `:226` `is_recurrent` is llama.cpp's hybrid rule. `:233` `is_sliding` needs both the window and the per-layer flag.
- `:110`, `:114`, `:117` constants for llama-server's allocation: 4 slots (the sidecar passes no `--parallel`), a 512-token micro-batch, 256-cell alignment.
- `:248` `read_model_info` reads every value through `read_value` (`:427`). Integers and integer lists of up to 4096 elements (`:423`) are kept. Text, floats and long lists (the tokenizer) are skipped as before. A sizing key of an unexpected type is passed over instead of ending the scan.
- `:292` keys are matched as `{prefix}.{name}` with an exact name. A nested key such as `gemma4.vision.block_count` is not the language model's.
- `:474` `read_integer` replaces `read_scalar_as_u32` and tells "unreadable header" apart from "value out of range". `:515` `skip_elements` is split out of `skip_value` so a list can be skipped after its type and count are read.

### `crates/lattice-desktop/src/features/llm/engine/sidecar_manager/config.rs`

- `:141` `resident_bytes_of` charges `kv_cache_bytes(context_size)`.
- `:164` `context_size_for` declines up front when the header cannot be sized, as before.
- `:181` the window is the largest whole thousand up to the ceiling whose priced cache fits the spare memory. For a plain model this equals the old `min(spare / bytes_per_token, ceiling)` rounded down to 1024, so plain models get the same window as before. All existing sidecar tests pass unchanged in their numbers.

### Tests

`gguf_metadata.rs` tests (new, all with real header values written through the existing `GgufBuilder`, which gains `i32_array`, `bool_array`, `float_array` and `read`):

- `a_plain_model_costs_the_same_for_every_token_of_its_window`: Llama 3.1 8B, 128 KiB a token at 4K, 8K, 32K and 128K.
- `a_qwen_hybrid_is_charged_for_its_full_attention_layers_only`: Qwen 3.5 9B, 32 KiB a token, 1 GiB at 32K, a quarter of the every-layer count.
- `the_mixture_of_experts_hybrid_counts_one_layer_in_four`: Qwen 3.6 35B-A3B, 20,480 bytes a token, 671,088,640 at 32K.
- `multi_token_prediction_blocks_are_charged_as_full_attention`: 9B MTP 9 layers, Qwen 3.8 27B 17 layers.
- `a_per_layer_list_is_read_and_the_keys_after_it_still_count`: Gemma 4 12B, the regression for the reader stopping.
- `sliding_window_layers_stop_growing_at_their_window`: Gemma 4 12B, 2,046,820,352 bytes at 32K.
- `layers_that_share_an_earlier_cache_cost_nothing`: Gemma 4 E4B, 57,344 bytes at 1 token, 641,728,512 at 32K.
- `a_per_layer_list_shorter_than_the_model_refuses_to_estimate`, `a_sizing_key_of_an_unexpected_type_is_passed_over`, `a_nested_key_is_not_mistaken_for_the_language_model_s`.
- Existing tests moved to the new API. `the_full_set_of_sizing_keys_is_read_together` became the Qwen 3.5 9B test above.

`sidecar_manager/tests.rs`:

- The old `ornith()` fixture was labelled `qwen35` but sized as a plain model. It is now `plain_9b()` with the same numbers, so the existing window tests keep their exact assertions.
- `a_qwen_hybrid_gets_four_times_the_window_its_full_attention_layers_allow`: at 8.65 GB of reported memory the 9B went from 6144 to 24,576.
- `a_qwen_hybrid_clears_the_floor_where_counting_every_layer_did_not`: at 8.0 GB, from the 4096 floor to 9216.
- `gemma_4_sliding_layers_are_charged_their_window_not_the_whole_context`: at 12.0 GB, Gemma 4 12B went from the 4096 floor (what the stopped reader produced) to 23,552. By hand: (1,897,569,881 spare - 1,509,949,440 sliding) / 16,384 = 23,658, down to 23,552. The test also checks that the next thousand does not fit.

## Keys verified in each header, and how

Headers were streamed by HTTP range request from `https://huggingface.co/<repo>/resolve/main/<file>` (the catalog repos and files in `curated.rs`) with a small Python reader. It read the key/value section only (8 to 16 MB each) and saved nothing but the printed keys. Qwen 3.8 27B was read from the local file in `~/.cache/lattice/models`, read-only.

| model | keys that size the cache |
|---|---|
| Qwen3.5-2B | `qwen35.block_count` 24, `head_count_kv` 2, `key_length`/`value_length` 256, `full_attention_interval` 4 |
| Qwen3.5-4B, 9B | `block_count` 32, `head_count_kv` 4, 256/256, `full_attention_interval` 4 |
| Qwen3.5-9B MTP | as 9B, `block_count` 33, `nextn_predict_layers` 1 |
| Qwen3.8-27B (local file) | `block_count` 65, `head_count_kv` 4, 256/256, `full_attention_interval` 4, `nextn_predict_layers` 1 |
| Qwen3.6-35B-A3B | `qwen35moe.block_count` 40, `head_count_kv` 2, 256/256, `full_attention_interval` 4 |
| Gemma 4 E4B | `gemma4.block_count` 42, `head_count_kv` 2 (scalar), `key_length`/`value_length` 512, `key_length_swa`/`value_length_swa` 256, `attention.sliding_window` 512, `attention.shared_kv_layers` 18, `attention.sliding_window_pattern` bool x42 |
| Gemma 4 12B | `block_count` 48, `head_count_kv` i32 x48 (8 sliding, 1 global), 512 / 256 swa, `sliding_window` 1024, `shared_kv_layers` 0, pattern bool x48 |
| Gemma 4 26B-A4B | `block_count` 30, `head_count_kv` i32 x30 (8 sliding, 2 global), 512 / 256 swa, `sliding_window` 1024, `shared_kv_layers` 0, pattern bool x30 |
| Llama 3.1 8B (bartowski Q4_K_M) | `llama.block_count` 32, `head_count` 32, `head_count_kv` 8, `embedding_length` 4096, no key widths |

Gemma 4's pattern is five sliding layers, then one global (`layer % 6 == 5` is global). No header carries `attention.recurrent_layers` or a scalar `sliding_window_pattern`.

The allocation rules were read from llama.cpp at the pinned tag `v0.6.0` (`crates/lattice-desktop/scripts/llama-server.lock`), raw from GitHub:

- `src/models/qwen35.cpp` and `qwen35moe.cpp`: `is_recr = (i < n_layer()) && ((i + 1) % full_attn_interval != 0)`, default interval 4. `attention.recurrent_layers` takes priority when present. So layers 3, 7, 11, ... (0-based) are full attention.
- `src/llama-hparams.cpp`: `n_layer() = n_layer_all - n_layer_nextn`. `has_kv(il)` is false from `n_layer_kv_from_start` on.
- `src/llama-model.cpp`: hybrid Qwen main context filter `il < n_layer() && !is_recr(il)`, so MTP blocks get a cache only in the MTP draft context. `HEAD_COUNT_KV` via `get_key_or_arr`. SWA widths default to the full widths.
- `src/models/gemma4.cpp`: per-layer pattern via `get_arr`, `n_layer_kv_from_start = n_layer_all - shared_kv_layers`. V is still cached when a layer has no V projection.
- `src/llama-kv-cache-iswa.cpp:73`: `size_swa = GGML_PAD(min(size_base, n_swa * (unified ? n_seq_max : 1) + n_ubatch), 256)`.
- `tools/server/server.cpp:168`: without `--parallel`, `n_parallel = 4` and `kv_unified = true`. `common/common.h`: `n_ubatch = 512`.
- `src/llama-kv-cache.cpp`: K is `n_embd_k_gqa(il)` wide per layer. V is per layer with flash attention and `n_embd_v_gqa_max()` without it. `ggml-metal-device.m` supports flash attention for head sizes 256 and 512, so on Apple Silicon auto flash attention is on and V is per layer.

## The formula

For a window of n tokens, f16:

- For each layer `il` below `block_count - shared_kv_layers`:
  - skip it if `full_attention_interval` is set, `il` is in the trunk (`il < block_count - nextn_predict_layers`) and `(il + 1) % interval != 0`;
  - it slides if `sliding_window > 0` and the pattern flags it;
  - bytes a token = `kv_heads(il) * (key + value) * 2`, with the `_swa` widths for a sliding layer, falling back to the full widths, then to `embedding / head_count`;
  - charge n tokens for a full layer, `min(n, roundup256(window * 4 + 512))` for a sliding one.
- `None` when a needed value is missing, a per-layer list is too short, or no layer costs anything.

## Catalog figures (`curated.rs`) against this formula

Weights plus KV plus 1 GB, at 8K and 32K, in GB, rounded up to a tenth as `memory_estimate_gb` does:

| entry | catalog min/comf | formula min/comf |
|---|---|---|
| Qwen 3.5 2B | 2.4/2.7 | 2.4/2.7 |
| Qwen 3.5 4B | 4.1/4.9 | 4.1/4.9 |
| Gemma 4 E4B | 6.2/6.6 | 6.3/6.7 |
| Qwen 3.5 9B | 7.0/7.8 | 7.0/7.8 |
| Qwen 3.5 9B MTP | 7.2/8.1 | 7.2/8.1 |
| **Gemma 4 12B** | **8.8/9.2** | **9.8/10.2** |
| Qwen 3.8 27B | 18.1/19.8 | 18.1/19.8 |
| **Gemma 4 26B A4B** | **18.5/19.0** | **19.1/19.6** |
| Qwen 3.6 35B A3B | 23.6/24.1 | 23.6/24.1 |

Two entries disagree by more than 0.3 GB. Track D charged sliding layers `window + 512` cells, one sequence. llama-server as the sidecar launches it serves 4 slots over a unified cache and allocates `4 * window + 512` cells. For Gemma 4 12B that is 4608 cells instead of 1536, about 1.0 GB more. For Gemma 4 26B A4B it is about 0.6 GB more. E4B differs by 0.06 GB. The table was not changed. The Qwen figures match exactly.

## Verified

All cargo commands ran from `crates/lattice-desktop` with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`.

- `cargo check --lib`: `Finished`, exit 0, no warnings.
- `cargo test --lib -- gguf_metadata sidecar_manager`: `test result: ok. 88 passed; 0 failed; 1 ignored` (the ignored one needs `LATTICE_GGUF_MODEL`).
- `cargo fmt`, then `rustfmt --edition 2021 --check` on the three touched files: clean. `cargo fmt` changed no other file.
- `bash scripts/check-rust-layer-boundaries.sh`: "Rust layer boundary check: clean (syntax-aware)."
- `bash scripts/check-repository-barrier.sh`: "Repository Barrier check: clean (272 production feature files)."
- Every expected number in the new tests was computed first by an independent Python model of the llama.cpp rules above, not by the Rust code.

## Not verified

- `cargo clippy --lib` did not run. After the passing test run, the shared target's `lattice-desktop` build script began failing with "failed to run tauri-build: failed to read plugin permissions". The cause is in the shared target, not this change: `debug/build/tauri-0c4148f3981e549e/out/tauri-core-permission-files` lists `/Users/josh/Code/lattice-temp/target/.../out/permissions/default.toml`. That target was copied from `lattice-temp`, whose own `target` has since been deleted. The same `default.toml` exists under `lattice-imm-target2`. Rewriting that one list to the `lattice-imm-target2` path, or forcing tauri's build script to rerun, should clear it for every agent. I did not touch it because it is outside this worktree.
- The ignored `a_real_gguf_on_this_machine_parses` against the local Qwen 3.8 27B file did not run, for the same reason.
- No llama-server ran, so the formula was not compared with llama-server's own "KV self size" log lines.
- The CPU and Vulkan builds were not checked for flash attention at head size 512. Without flash attention V is cached at the widest layer (see Risks).

## Risks

- **Slot count is assumed.** Sliding layers are charged for 4 slots. The MTP build runs `-np 1` and would need less; it has no sliding layers, so nothing changes today. If the sidecar starts passing `--parallel`, `SERVER_SLOTS` must follow.
- **MTP blocks are always charged.** llama-server caches them only when drafting. For Qwen 3.8 27B, which the catalog does not draft with, that overstates by one layer: 128 MiB at 32K.
- **Recurrent state and checkpoints are not counted.** A Qwen 3.5 9B slot holds about 50 MB of Gated DeltaNet state, so about 200 MB for 4 slots. The 27B holds about 150 MB a slot. `--ctx-checkpoints 32` copies that state per slot into host memory, which is the same pool on Apple Silicon. The 30% headroom in `VRAM_USABLE_FRACTION` has to absorb all of it, and windows are now larger, so that headroom is used more.
- **Flash attention off.** llama.cpp then caches V at the widest layer's width. For Gemma 4 12B the global layers' V would be 2048 wide, not 512, and the per-token cost 2.5x what is charged. Metal supports these head sizes, so Apple Silicon is unaffected.
- **Scalar `sliding_window_pattern` and `attention.recurrent_layers` are not read.** No catalog header uses them. A model that only has them is sized as if every layer attended to the whole window. That overcounts and stays safe.
- **The catalog under-states Gemma 4 12B and 26B A4B** by 1.0 and 0.6 GB (table above). The fit badges for those two are optimistic until `curated.rs` is updated.

## Suggested commit message

```
Size the sidecar's KV cache the way llama.cpp allocates it

The sidecar charged every layer as full attention, about 4x too much
for the Qwen 3.5/3.6/3.8 hybrids, and the GGUF reader gave up at
Gemma 4's per-layer KV-head list, so both got windows far smaller
than memory allowed. The reader now keeps per-layer lists and the
hybrid, sliding-window, MTP and shared-cache keys. The cost is priced
per window: full-attention layers per token, linear-attention layers
not at all, sliding layers up to their window for each of the
server's four slots, shared layers not at all. Plain models get the
same window as before.
```
