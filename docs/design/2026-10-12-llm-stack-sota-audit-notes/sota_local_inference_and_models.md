# SOTA local inference and model selection for Lattice (Apple Silicon, October 2026)

Researched 2026-10-12. Lattice's sidecar currently passes only `-m --port --host --no-webui -ngl --ctx-size --jinja` ([readiness.rs](file:///Users/josh/Code/lattice-temp/crates/lattice-desktop/src/features/llm/engine/sidecar_manager/readiness.rs)). It pins `llama_cpp_tag v0.6.0`, which is not an upstream `bNNNN` tag ([llama-server.lock](file:///Users/josh/Code/lattice-temp/crates/lattice-desktop/scripts/llama-server.lock)). Upstream was at b11577 on 12 Oct ([releases](https://github.com/ggml-org/llama.cpp/releases)).

## llama.cpp server features and flags (October 2026)

### Takeaway
Current llama-server changes several defaults (flash attention auto, Jinja on, auto slots, `--fit`). It also ships MTP speculative decoding for Qwen 3.5/3.6 (merged 2026-05-16, b9180). Hybrid Gated DeltaNet models still lose their prompt cache when the prompt prefix changes.

### Cited Findings
- **Current defaults:**
  - `-fa` defaults to `auto`. `--jinja` is enabled by default.
  - `--cont-batching` is on, and `--cache-prompt` is on.
  - `--parallel` defaults to `-1` (auto). `--kv-unified` turns on when slots are auto.
  - `--fit` defaults to `on`: it adjusts unset arguments to fit device memory.
  - `--cache-ram` (host prompt cache) defaults to 8192 MiB. `--ctx-checkpoints` defaults to 32 per slot.
  - `--cache-reuse` defaults to 0 (disabled). `--swa-full` defaults to false.
  - Source: [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
- **Removed or replaced flags:**
  - `--draft-max` and `--draft-min` are removed. Use `--spec-draft-n-max` (default 3) and `--spec-type`.
  - `--mlock` and `--mmap` are replaced by `--load-mode` (none, mmap, mlock, mmap+mlock, dio).
  - Other flags: `--sleep-idle-seconds`, router mode via `--models-dir`, `--reasoning on|off|auto`, `--reasoning-budget`, `--chat-template-kwargs`.
  - Source: [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
- **Server API:**
  - Slot endpoints: `/slots`, and `/slots/{id}?action=save|restore|erase` (needs `--slot-save-path`).
  - Utility endpoints: `/tokenize` (with `with_pieces`), `/detokenize`, `/apply-template`, `/v1/rerank`.
  - Request fields: `response_format` json_schema, `grammar`, `parallel_tool_calls`, `n_probs`, `cache_prompt`, `id_slot`.
  - Reasoning fields: `reasoning_format`, `reasoning_effort`, and `reasoning_control` (ends thinking early via `/v1/chat/completions/control`).
  - Source: [server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
- **MTP speculative decoding:**
  - Merged 16 May 2026 (PR #22673). Older builds fail to load MTP GGUFs. The flag was renamed `--spec-type mtp` → `draft-mtp` on 2026-05-13 ([unsloth 9B-MTP card](https://huggingface.co/unsloth/Qwen3.5-9B-MTP-GGUF)).
  - Limitations: `-np` > 1 and `--mmproj` are not supported with MTP ([unsloth 9B-MTP card](https://huggingface.co/unsloth/Qwen3.5-9B-MTP-GGUF)).
  - Unsloth recommends `--spec-draft-n-max 2`, testing 1–6 ([unsloth Qwen3.6](https://unsloth.ai/docs/models/qwen3.6)).
- **MTP measured on an M5 Max (128 GB), 8-bit, 2026-06-14:**
  - Qwen3.6-27B dense: 18 → 32 tok/s, about +75%.
  - Qwen3.6-35B-A3B: 93 → 105 tok/s, about +12%.
  - Flags used: `--spec-type draft-mtp --spec-draft-n-max 3 -fa on --parallel 1`.
  - Source: [stared bench](https://github.com/stared/benching-local-llms-on-apple-silicon).
- **MTP in the GGUF itself:** Qwen3.5-9B has `mtp_num_hidden_layers: 1` on top of 32 layers ([config.json](https://huggingface.co/Qwen/Qwen3.5-9B/raw/main/config.json)). That extra layer is the `blk.32` that the old pinned build choked on (internal memory note).
- **Earlier MTP history (pre-merge):**
  - Speculative decoding on Qwen3.5 initially failed on hybrid models ([#20039](https://github.com/ggml-org/llama.cpp/issues/20039)).
  - A pre-merge fork found MTP a net negative because recurrent-state checkpoint and restore was too expensive ([NodeNestor](https://github.com/NodeNestor/qwen3.5-27b-mtp-llamacpp)).
- **Hybrid-model cache bugs:** Qwen3.5 can re-process the full prompt on every turn. One report measured about 8 minutes per turn at 15k tokens ([#20225](https://github.com/ggml-org/llama.cpp/issues/20225)).
  - Maintainer position: "not a bug. OpenCode mutates the prefix".
  - Workarounds users reported:
    - Keep the prefix byte-stable.
    - Fix the template's reasoning_content handling.
    - Add an empty user message after tool calls.
    - Use `--ctx-checkpoints 64-128`, `--cache-ram -1`, `--kv-unified`, `--parallel 1`, `--cache-reuse 256`.
  - Source: [#20225](https://github.com/ggml-org/llama.cpp/issues/20225).
- **Checkpoint-restore fix rejected:** a proposed fix for the hybrid checkpoint restore was closed as "incorrect" on 2026-04-26. The maintainer suggested `preserve_thinking` instead, and fork patches remain unmerged ([#22384](https://github.com/ggml-org/llama.cpp/issues/22384)).
- **Reasoning budget:**
  - Builds after 2026-04-10 enable the reasoning-budget sampler for Qwen 3.5 with a default of INT_MAX. This can hang or spill the KV cache ([forge #54](https://github.com/antoinezambelli/forge/issues/54)).
  - The common parameter is now `reasoning_budget_tokens` (PR #20297).
  - A softer warning-then-cutoff mode is a draft only ([PR #25961](https://github.com/ggml-org/llama.cpp/pull/25961)).
- **MoE CPU offload:** `--n-cpu-moe N` keeps the expert weights of the first N layers on CPU, and `--cpu-moe` keeps all of them there ([server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)).
- **Recent Metal fixes:**
  - b11575: `kernel_mul_mm` on large tensors.
  - b11569: `mul_mat_id` with duplicate ids, benchmarked on Qwen3.6-35B-A3B.
  - b11576: flash attention is no longer disabled for many head sizes.
  - Source: [releases](https://github.com/ggml-org/llama.cpp/releases).

### Inferences
- **Upgrade first.** Lattice's pin predates the MTP merge, and the team currently strips MTP tensors from the GGUF. Upgrading to ≥b9180, ideally a current ~b115xx, re-enables those heads for roughly +75% decode on the dense 27B tier at no quality cost.
- **Set `-np 1` explicitly.** The auto default now creates several slots and a unified KV cache, and MTP refuses `-np` > 1.
- **Keep the history prefix byte-identical** across turns: same template, reasoning stripped consistently, no rewritten earlier messages. Pin `id_slot`, and raise `--ctx-checkpoints`. On hybrid models, `--cache-reuse` KV shifting cannot rescue a mutated prefix.
- **Pass an explicit `--reasoning-budget`** (or per-request control) on every thinking call, rather than relying on the INT_MAX default.

### Gaps
- No official changelog states when hybrid checkpoint restore became reliable. Status as of October 2026 is unverified.
- No Apple-Silicon MTP numbers for the 9B, and no MTP acceptance-rate data.
- No source confirmed whether a per-request reasoning budget works for Qwen 3.5 at current HEAD.

## Alternatives to llama.cpp on Apple Silicon

### Takeaway
No alternative clearly beats llama.cpp for a bundled Rust/Tauri sidecar. MLX trades roughly 10–25% slower decode on the newest chips, and sometimes faster decode on older ones, for 15–30% less memory. Ollama's MLX gains are measured against its own old engine.

### Cited Findings
- **M5 Max, decode tok/s:**
  - Qwen3.6-35B-A3B: llama.cpp 93 vs MLX 85, at 44 GB vs 37 GB.
  - Qwen3.6-27B: llama.cpp 18 vs MLX 17, at 41 GB vs 28 GB.
  - Source: [stared bench](https://github.com/stared/benching-local-llms-on-apple-silicon).
- **M1 Max, Qwen3.6-35B-A3B:**
  - Prefill: llama.cpp 891.7 vs MLX 636.3 tok/s.
  - Decode: llama.cpp 54.5 vs MLX 67.8 tok/s.
  - Source: [bishal.io](https://bishal.io/blog/six-local-models-m1-max-llama-cpp-vs-mlx/) (anecdotal).
- **Ollama MLX engine:**
  - v0.19 (2026-03-30) on Qwen3.5-35B-A3B: prefill 1154 → 1810, decode 58 → 112 tok/s, measured against Ollama's own v0.18 engine ([gigazine](https://gigazine.net/gsc_news/en/20260402-ollama-0-19-support-apple-mlx/)).
  - June 2026: a further +20% and NVFP4 support ([Ollama blog](https://registry.ollama.ai/blog/mlx-performance)).
  - Only safetensors models use MLX. GGUF models still run on llama.cpp ([levelup](https://levelup.gitconnected.com/ollama-mlx-nearly-doubles-llm-speed-on-your-mac-58-to-112-tok-s-and-your-old-models-get-none-of-fd1a9ec531fb)).
- **vLLM-Metal:** llama.cpp was about 1.2–1.3× faster on a 1B model, per a Docker benchmark ([aiindigo](https://aiindigo.com/blog/llama-cpp-vs-native-macos-inference-which-is-faster-for-apple-silicon), secondary).
- **BaseRT (research runtime):** claims up to 1.56× llama.cpp decode ([arXiv 2607.00501](https://arxiv.org/html/2607.00501v1)).

### Inferences
- Stay on llama.cpp. It is a single binary, takes GGUF from one pipeline, has grammar and json_schema plus native tool parsing, and now leads on M5. Reconsider MLX only if memory headroom on 16 GB Macs becomes the binding constraint.

### Gaps
- No mistral.rs or candle Metal benchmarks for 2026, and no LM Studio engine comparison.
- No clean M5 prefill comparison for llama.cpp vs MLX.

## Best open-weight models by class (October 2026)

### Takeaway
Qwen still dominates on-device sizes. Qwen3.5-9B is close to Qwen3.5-27B on tool calling: BFCL-V4 66.1 vs 68.5, TAU2 79.1 vs 79.0. The MoE 35B-A3B decodes about 5× faster than the dense 27B. Gemma 4 (Apache 2.0) is the strongest non-Qwen family.

### Cited Findings
- **Qwen3.5 lineup (Feb–Mar 2026, Apache 2.0, 262K context, hybrid with 3 DeltaNet layers per full-attention layer):**
  - Sizes: 0.8B, 2B, 4B, 9B, 27B, 35B-A3B, 122B-A10B, 397B-A17B ([codersera](https://codersera.com/blog/qwen-3-5-complete-guide-2026/)).
  - The 9B has 8 of 32 layers as full attention ([config](https://huggingface.co/Qwen/Qwen3.5-9B/raw/main/config.json)).
- **Qwen3.5 vendor scores:**

  | Benchmark | 9B | 27B | 35B-A3B |
  |---|---|---|---|
  | BFCL-V4 | 66.1 | 68.5 | 67.3 |
  | TAU2 | 79.1 | 79.0 | 81.2 |
  | AA-LCR | 63.0 | 66.1 | 58.5 |
  | IFEval | 91.5 | 95.0 | 91.9 |
  | MMLU-Pro | 82.5 | 86.1 | — |

  Sources: [9B card](https://huggingface.co/Qwen/Qwen3.5-9B), [27B card](https://huggingface.co/Qwen/Qwen3.5-27B). GPT-5-mini scores 55.5 on BFCL-V4 ([27B card](https://huggingface.co/Qwen/Qwen3.5-27B)).
- **Qwen3.6:**
  - 35B-A3B released 2026-04-16 and 27B on 2026-04-22.
  - The 27B scores 77.2 on SWE-bench Verified (vendor).
  - Source: [codersera](https://codersera.com/blog/qwen-3-5-complete-guide-2026/).
  - No Qwen3.6 small sizes were found ([search summary/Reddit](https://reddit.sentinel-team.org/posts/1taafs4/snapshots/2026-05-15T23%3A40%3A01.25497Z)).
- **Qwen3.8-27B:**
  - Release date conflicts: Aug 14 ([codersera](https://codersera.com/blog/qwen-3-5-complete-guide-2026/)) vs Aug 5 ([morphllm](https://www.morphllm.com/best-open-source-llm)).
  - Dense, vision-language, Apache 2.0, 262K context, GPQA 89.2 (vendor).
  - It "overthinks", using about 2× the tokens of 3.6.
  - Qwen 3.7 has no open weights.
  - Source: [codersera](https://codersera.com/blog/qwen-3-5-complete-guide-2026/).
- **Gemma 4 (Apache 2.0):**
  - Sizes: E2B (2.3B effective), E4B (4.5B effective), 12B, 26B-A4B (3.8B active), 31B. Context is 128K for E2B/E4B and 256K for the rest.
  - Tau2 (31B / 26B-A4B / 12B / E4B / E2B): 76.9 / 68.2 / 69.0 / 42.2 / 24.5.
  - MRCR 128K (31B / 26B-A4B / 12B): 66.4 / 44.1 / 43.4.
  - Source: [model card](https://ai.google.dev/gemma/docs/core/model_card_4).
- **Other ~30B-A3B MoEs:**
  - NVIDIA vendor numbers, Nemotron 3 Nano 30B-A3B vs gpt-oss-20b: TauBench v2 Retail 56.9 vs 38.0 ([NVIDIA](https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-nano-30b-a3b)).
  - Anecdotal 13-model tool-call test: gpt-oss-20b 85%, Nemotron 3 Nano 85%, Mistral Small 3.2 42.5% ([jdhodges](https://www.jdhodges.com/blog/local-llms-on-tool-calling-2026-pt1-local-lm/)).
  - Poolside Laguna XS 2.1 (33B-A3B, 2026-07-02) claims 70.9 on SWE-bench Verified ([promptquorum](https://www.promptquorum.com/local-llms/local-llm-model-updates-2026), low reliability).
- **Utility-size models:**
  - Qwen3.5-2B (thinking): BFCL 43.6, TAU2 48.8. Its IFEval in non-thinking mode is only 61.2.
  - Qwen3.5-2B "more prone to entering thinking loops".
  - Qwen3.5-0.8B: IFEval 52.1 in non-thinking mode.
  - Source: [2B card](https://huggingface.co/Qwen/Qwen3.5-2B).
  - Gemma 4 E4B scores 69.4 on MMLU-Pro and E2B 60.0 ([card](https://ai.google.dev/gemma/docs/core/model_card_4)).
- **Deep research:** Tongyi DeepResearch 30B-A3B is still the September 2025 weights as of June 2026 ([rywalker](https://rywalker.com/research/tongyi-deepresearch)).
- **BFCL v4 reliability:** an Epoch audit found possible defects in 48% of sampled items ([Epoch](https://epoch.ai/benchmarks/berkeley-function-calling-leaderboard/review)).

### Inferences
- **Add Qwen3.6-35B-A3B as the 32 GB+ tier.** It decodes about 5× faster than the dense 27B (93 vs 18 tok/s) with comparable BFCL and TAU2 scores. The dense 27B buys little over the 9B on tool calling.
- **Watch long-context quality on the MoE.** Its AA-LCR score is 7.6 points lower than the 27B's.
- **Evaluate Qwen3.8-27B** for 64 GB Macs, with a capped thinking budget.
- **Utility model:** prefer Qwen3.5-4B or Gemma 4 E4B in non-thinking mode with json_schema over the 0.8B/2B. The 0.8B's instruction following is weak.

### Gaps
- No reliable data found on Mistral Small 4, Ministral 2026, Phi-5, GLM-4.x Flash/Air, Llama after 4, WebSailor, or Jan-nano.
- The official BFCL v4 leaderboard was not reached. There is no independent RAG-faithfulness benchmark for these models, and no Qwen3.5-4B card numbers.

## Quantization

### Takeaway
On Qwen3.5-35B-A3B, Q5 and above is near-lossless and Q4 variants cost 2–4× the KLD of Q5. Imatrix-based dynamic quants beat plain Q4_K_M. Attention and SSM tensors must stay at high precision in hybrid models.

### Cited Findings
- **Qwen3.5-35B-A3B, mean KLD:**

  | Quant | Size (GB) | Mean KLD |
  |---|---|---|
  | Q8_K_XL | 36.0 | 0.0026 |
  | Q6_K_XL | 28.2 | 0.0041 |
  | Q5_K_M | 24.5 | 0.0058 |
  | AesSedai Q4_K_M | 20.6 | 0.0096 |
  | UD-Q4_K_XL | 19.2 | 0.0137 |
  | Unsloth Q4_K_M | 18.5 | 0.0192 |
  | IQ4_XS | 16.4 | 0.0235 |
  | MXFP4_MOE | 18.2 | 0.0272 |
  | Q3_K_M | 15.5 | 0.0324 |

  Source: [unsloth](https://unsloth.ai/docs/models/qwen3.5/gguf-benchmarks).
- **Unsloth guidance:**
  - Quantizing `attn_*` is "especially sensitive" in hybrid models, and `ssm_out` "dramatically increases KLD".
  - MXFP4 was retired from the UD quants, and the GGUFs were re-released on 2026-03-05.
  - Source: [unsloth](https://unsloth.ai/docs/models/qwen3.5/gguf-benchmarks).
- **KV-cache quantization:**
  - Qwen3-8B: q8_0 is within the error bar at 4K–32K, and q4_0 is +0.5–1.1% PPL. At 64K depth, decode runs at 55% / 50% of f16 for q8_0 / q4_0 (A100/CUDA) ([sotaaz](https://sotaaz.com/post/llamacpp-kv-cache-quantization-bench-en)).
  - q4_0 was 11.9% slower at about 24K ([#20969](https://github.com/ggml-org/llama.cpp/discussions/20969)).
  - Unsloth suggests trying bf16 KV if output is gibberish ([unsloth Qwen3.6](https://unsloth.ai/docs/models/qwen3.6)).
- **TurboQuant (sub-3-bit KV):**
  - Not merged. It lives only in forks, though one fork has Metal kernels.
  - Results conflict: +1.1% PPL in one report vs "much worse than q4_0" in another.
  - On Qwen3.5-27B only 16 of 64 layers carry KV, at about 64 KB/token in fp16.
  - Source: [#20969](https://github.com/ggml-org/llama.cpp/discussions/20969).

### Inferences
- **Ship UD-Q4_K_XL (or a Q5_K_M-class quant) on 32 GB Macs and Q6/Q8 on 64 GB.**
- **Leave the KV cache at f16, or q8_0 at most, on hybrid Qwen.** Its KV is already about 4× smaller than a dense model's, so q4_0 saves little and slows long contexts.

### Gaps
- No KLD tables for the 9B or 27B (published only as images).
- QTIP, AQLM, EXL3 and HQQ have no usable llama.cpp/Metal path documented in the sources found.

## Sampling

### Takeaway
Follow the Qwen card's sampling settings exactly. Use a lower temperature, top_k 20 and presence_penalty 1.5 in non-thinking mode, and keep min_p at 0.

### Cited Findings
- **Qwen3.5 card settings:**

  | Mode | temp | top_p | top_k | min_p | presence_penalty |
  |---|---|---|---|---|---|
  | Thinking, general | 1.0 | 0.95 | 20 | 0 | 1.5 |
  | Thinking, precise coding | 0.6 | 0.95 | 20 | 0 | 0.0 |
  | Non-thinking, general | 0.7 | 0.8 | 20 | 0 | 1.5 |
  | Non-thinking, reasoning | 1.0 | 1.0 | 40 | 0 | 2.0 |

  - `repetition_penalty` is 1.0 throughout.
  - Thinking is on by default. Disable it with `chat_template_kwargs {"enable_thinking": false}`. The `/think` and `/nothink` switches are not supported.
  - Source: [9B card](https://huggingface.co/Qwen/Qwen3.5-9B).
- **Qwen3.6:** thinking mode uses presence_penalty 0.0. `preserve_thinking: true` is set in the examples ([unsloth Qwen3.6](https://unsloth.ai/docs/models/qwen3.6)).
- **Gemma 4:** temp 1.0, top_p 0.95, top_k 64. Thinking is toggled with `<|think|>` in the system prompt ([card](https://ai.google.dev/gemma/docs/core/model_card_4)).

### Gaps
- The Qwen3.5 card has no explicit warning against greedy decoding, so that pitfall is unverified for 3.5.
- No 2026 evidence found on DRY, XTC or min-p for RAG.

## Embedding and reranker serving

### Takeaway
llama.cpp can serve both Qwen3-Embedding and Qwen3-Reranker. Reranking needs an official-converter GGUF plus `--rerank --pooling rank`. No trustworthy M-series throughput numbers exist for the 0.6B class.

### Cited Findings
- **Reranker setup:**
  - Use the `/v1/rerank` endpoint. `/v1/embeddings` returns zeros for rerankers.
  - Community GGUFs that lack the classifier head or the rerank template silently produce null or bad scores.
  - ggml-org publishes a correct Q8_0 build.
  - Source: [Voodisss card](https://huggingface.co/Voodisss/Qwen3-Reranker-0.6B-GGUF-llama_cpp), [ggml-org](https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF).
- **Batch splitting:** b11223 allows RANK-pooling inputs larger than `n_ubatch` ([release note](https://newreleases.io/project/github/ggml-org/llama.cpp/release/b11223)).
- **MLX vs llama.cpp for embeddings:** one source claims MLX is about 50% faster, e.g. Qwen3-Embedding-8B at 580 vs 410 passages/s on an M5 Max ([contracollective](https://contracollective.com/blog/local-embeddings-apple-silicon-nomic-bge-qwen3-m5-max-2026)). It cites llama.cpp b4900 and MLX 0.21 for a June 2026 M5 test, which is suspect.

### Gaps
- No reliable 0.6B embedding or reranker throughput on M-series for llama.cpp, fastembed/ONNX or candle. This needs an in-house benchmark.

## Memory budgeting

### Takeaway
The default GPU wired cap is about 75% of RAM. Hybrid Qwen KV is small: about 32 KiB/token (f16) on the 9B and about 64 KB/token on the 27B. Weights dominate the budget.

### Cited Findings
- **Wired-memory cap:**
  - Default is about 75% of RAM, e.g. about 21–24 GB on 32 GB and 48 GB on 64 GB.
  - Raise it with `sudo sysctl iogpu.wired_limit_mb=<MB>` (Sonoma+). It does not persist across reboots and is unsupported by Apple. Leave 8–16 GB of headroom.
  - Source: [devnote](https://github.com/ivanopcode/devnote-override-macos-metal-vram-cap).
- **Weight sizes:**
  - Qwen3.5-9B Q4_K_M is about 5.5 GB ([codersera](https://codersera.com/blog/qwen-3-5-complete-guide-2026/)).
  - Qwen3.6-27B: 18 GB at 4-bit, 24 GB at 6-bit. Qwen3.6-35B-A3B: 23 GB at 4-bit. MTP adds about 1 GB ([unsloth](https://unsloth.ai/docs/models/qwen3.6)).
- **9B KV derivation:** 8 attention layers × 4 KV heads × 256 dim × K+V × 2 bytes ([config](https://huggingface.co/Qwen/Qwen3.5-9B/raw/main/config.json)).

### Inferences
- **9B KV cost:** about 1 GiB at 32K context and 4 GiB at 128K. Recurrent state is about 50 MB per sequence or checkpoint.
- **16 GB Macs:** 9B Q4 (5.5 GB) + 32K KV (1 GB) + 0.6B embed and rerank Q8 (about 1.3 GB) + a 2–4B utility model (1.5–3 GB) comes to about 9–11 GB. That fits under the roughly 12 GB cap only without a 27B.
- **32 GB Macs:** 35B-A3B or 27B at UD-Q4 plus 64K context comes to about 22–25 GB. That is at the default cap, so either raise `iogpu.wired_limit_mb` or rely on `--fit`.
- **64 GB Macs:** the 27B or 35B-A3B at Q6/Q8 with 128K context.
