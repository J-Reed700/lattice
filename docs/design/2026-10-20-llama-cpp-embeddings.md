# Embeddings on the bundled llama.cpp sidecar

Date: 2026-10-20. Status: design, ready to build; Josh's decisions recorded in §18 (2026-10-20). Code read on `design/llama-embeddings` at main `bffb28c3`; the Activity drawer (#142) read on `feat/activity-panel` in `/Users/josh/Code/lattice-activity`. `path:line` is under `crates/lattice-desktop/src/` unless prefixed `crates/`, `scripts/`, `evals/` or `.github/`. Anything not checked in code, in a log, or by running the pinned binary is marked **unverified**.

## 0. One screen for Josh

**What changes.** The embedding model (Qwen3-Embedding-0.6B, now the only curated embedding model on every machine; all-MiniLM-L6-v2 leaves the catalog, §18 Q2) stops running inside the app on Candle and runs in a second `llama-server` process, the same binary that already runs chat, OCR and vision. The model file becomes a GGUF (F16) instead of a safetensors folder. One process manager starts, watches and stops both kinds of server.

**What you will notice.**
- On your Mac: indexing gets a little faster (the model alone goes from about 3,400–3,800 tokens/s to about 4,200–5,300 tokens/s; the index loop's own cost is unchanged, so expect roughly 15–40 % more passages per second). Search results do not move: the F16 GGUF produces the same vectors as Candle (cosine ≥ 0.99999 on 480 passages; identical top result on 45 of 45 queries).
- On Windows and Linux PCs: embeddings run on the GPU through Vulkan for the first time. Today Candle there is CPU-only F32 at 78–152 tokens/s; llama.cpp on the CPU alone is 720–890 tokens/s at F16, and a GPU is faster again. They also stop being handed the weaker MiniLM model: today the default asks Candle whether a GPU exists, Candle (no CUDA, no Vulkan) says no, and MiniLM is picked. After this change every machine gets Qwen3; a machine with no usable GPU runs it on the CPU.
- The Activity drawer lists the embedding server like any other model: loading, running, what is embedding through it, GPU bytes.
- Settings loses two switches nobody should flip: "late chunking" (measured worse than chunk-first on every metric) and the BGE-M3 sparse branch (no curated model has it, every eval ran with it off). Both are deleted, not hidden.
- One download on first start after the upgrade: the GGUF (1.2 GB) plus `tokenizer.json` (11 MB). Your old safetensors folder stays on disk until you delete it.

**What stays the same.** Candle stays for the reranker and Whisper. The chunker, the context prefixes, the `Instruct:` query prefix, the hybrid search, RRF, Matryoshka truncation, the vector files, `EmbedTurns`, course progress, the eval harness: all unchanged. Your existing indexes keep working after a one-off hand conversion of identity strings (§7.4), because the new identity of the F16 space is simply the GGUF file's SHA-256, which Hugging Face publishes and `shasum` reproduces.

**What you decided** (§18, 2026-10-20): no Q8_0 option; MiniLM dropped, Qwen3 for everyone; no user-owned `/v1/embeddings` server in this build; the old safetensors folder is never deleted automatically (its size and a Remove button in Settings → Models); `-np 4`.

## 1. What exists today (verified)

**The port.** `EmbeddingPort` (`crates/lattice-core/src/application/ports/embedding_port.rs:58`) has `embed_single`, `embed_query` (default: `embed_single`), `embed_batch`, `split_text` / `window_parts` (chunking with the model's own tokenizer, UTF-8 byte offsets), `uses_late_chunking` + `embed_span_chunks`, `supports_sparse` + `embed_sparse_batch` + `embed_batch_with_sparse` + `embed_sparse_query`, `model_identity`, `accelerator_bytes`, `dimension`, `is_ready`. Sixteen implementors, all but one of them test fakes; the real one is `CandleEmbeddingService` (`features/embedding/candle_service/ports.rs:19`). `LoadedEmbeddingModelPort` (read-only "current model") and `EmbeddingIndexBinding::bind_first` (search binds its vector space before the model is published) sit beside it.

**The Candle engine.** `features/embedding/candle_service.rs` (283 lines) plus `candle_service/{loading,forward,batching,spans,config,ports,tests,throughput_bench,qwen3_live_tests}.rs`, `qwen3_encoder.rs` (a cache-free batched Qwen3 forward pass written because `candle_transformers::models::qwen3` cannot batch), `sparse_head.rs` (BGE-M3's `sparse_linear.pt`), `late_chunking.rs` (pure pooling rules + `vector_identity`), `input_policy.rs` (tokenizer-based chunk splitting; `RETRIEVAL_CHUNK_TARGET_TOKENS = 480`; the model window is `model_token_limit(dir).min(2048)`, `candle_service/loading.rs:100`), `prefixes.rs` (the family table: Qwen3 `Instruct: … \nQuery:` on queries only, E5, Nomic, BGE-en). Architectures: BERT, DistilBERT, XLM-R, MPNet, JinaBERT, NomicBERT, ModernBERT, Qwen3. Weights are F16 on Metal, F32 everywhere else (`config.rs weights_dtype`). Metal is compiled in only under `cfg(all(target_os = "macos", target_arch = "aarch64"))` (`crates/lattice-desktop/Cargo.toml:197`); `compute_device.rs` tries Metal, then `Device::cuda_if_available` (never compiled in), then CPU. 7,837 lines in `features/embedding/`.

**Lifecycle.** `EmbeddingRuntime` (`features/embedding/runtime.rs`): single-flight `get_or_load`, failure cooldown, `invalidate`. `EmbeddingLoader` (`loading.rs:30`) reads the active row (`ActiveModels::get_active_embedding_model`), requires a recorded `embedding_artifact_identity`, opens `CandleEmbeddingService::open(dir, identity)` off the runtime thread, then `index.bind_first(&service, late_chunking)`. `Container::open_embedding_for_activation` (`di.rs:34`) opens a model for the activation path. `AppEmbedders` (`di.rs:66`) is the `EmbedderSource` every passage index builds with. The model is never unloaded.

**Identity.** `infrastructure/ml/artifact_identity.rs:compute` hashes `lattice-embedding-input-v2`, `config.json`, `tokenizer.json`, `tokenizer_config.json`, `sentence_bert_config.json`, `1_Pooling/config.json`, `model.safetensors` (and `pytorch_model.bin`). Recorded on the `models` row at activation (`set_active_embedding_model.rs:34`, `download/saga.rs:314`), column `embedding_artifact_identity` with a CHECK of exactly `sha256:` + 64 lowercase hex (`migrations/20260916000000_init_schema.sql:990`). The vector-space key is `vector_identity(identity, strategy, prefixes)` (`late_chunking.rs:97`): the bare identity, `+late-chunking-v1` for late chunking, `+prefix-<id>` when the family has a document prefix. Search computes the same key before the model loads (`interfaces/di/modules/search.rs:index_identity`). It names: `embedding_generation_vectors.model_identity`, `text_embeddings.model_name` (bound as the identity in `generation::restore_page`), `chunk_sparse_terms.model_identity`, the library file `usearch-<identity with ':'→'-'>[-<layout>].usearch` (`runtime_index.rs:24`), and every passage index's `meta.identity` + `vectors-<identity>.usearch` (`infrastructure/passage_index/vectors.rs:22`, `store.rs:30`).

**Catalog and download.** Two curated embedding entries (`crates/lattice-core/src/domain/models/curated/embedding.rs`): `qwen3-embedding-0.6b` (safetensors, 1,024 dims, `context_length: 2048`, Matryoshka) and `all-minilm-l6-v2` (safetensors, 384 dims, 512). Default: Qwen3 where `gpu_acceleration_available()` (Candle's view), else MiniLM (`crates/lattice-core/src/domain/models/embedding_defaults.rs:65`, `first_run_setup.rs:135`). Hugging Face search (`model_management/commands.rs:596`) resolves embedding repos to safetensors file lists and HEAD-probes for `sparse_linear.pt` (`download_model/metadata.rs:39-80`); an ONNX path is still described as "legacy… on its way out". Downloads are ranged, resumable, SHA-256 checked when a checksum is given (`features/download/engine/transfer.rs:1`, `:173`). GGUF downloads for chat run `compatibility::check_download` against `scripts/llama-architectures.txt` (159 names from the pinned `v0.7.0`, including `bert`, `nomic-bert`, `jina-bert-v2`, `jina-bert-v3`, `gemma-embedding`, `qwen3`; not `modernbert`, `mpnet`, `distilbert`, `xlm-roberta`).

**The sidecar.** `features/llm/engine/sidecar_manager/*`: `SidecarConfig { model_path, n_gpu_layers, context_size, mtp_capable, speculative_decoding, parallel_slots, seed, kv_cache_quantization, projector_path }` is `Eq + Hash` and keys `SharedProcesses` so two roles share one process (`startup.rs:33 start_shared`). `start_with_fallback` walks Primary-GPU → Primary-CPU → `Cpu` binary (Windows/Linux only). Preflight runs `--version` once per binary. Port from `TcpListener::bind("127.0.0.1:0")`, API key in `LLAMA_API_KEY` (never argv), readiness is `GET /health` 200 with a wait that bounds silence, not duration (`readiness.rs:373`; `READINESS_IDLE_TIMEOUT` 90 s, `READINESS_ATTEMPT_CAP` 600 s, `STARTUP_TOTAL_BUDGET` 900 s in `sidecar_manager.rs:93-104`), Windows Job Object, `kill_all` on exit. `SidecarRegistry::resident_gpu_bytes` prices running servers from the GGUF header (`registry.rs:274`, `gguf_metadata.rs:kv_cache_bytes`, `compute_buffer_bytes`). Device detection is ggml's own `--list-devices` on the bundled binary (`system/backend_devices.rs`). The OCR feature runs its own second server with a 60 s idle reaper (`ocr/server.rs`). `LlmInstances` (`features/llm/instances.rs`) admits new servers against GPU memory and evicts idle models to make room; it never refuses on the estimate; the embedding model's `accelerator_bytes` is subtracted as "in-app bytes" (`features/llm/loading.rs:83`).

**Binaries.** Pinned by `crates/lattice-desktop/scripts/llama-server.lock` (`llama_cpp_tag v0.7.0`, release `llama/v0.7.0-r1`, repo `J-Reed700/lattice`): Metal (aarch64 mac), CPU (x86_64 mac), Vulkan + CPU (Windows), Vulkan + CPU (Linux). The local aarch64 binary reports `version: 0.7.0-dev (build 11669)`. CI checks against empty placeholder sidecars (`.github/workflows/ci.yml:78-88`), so nothing in CI can run a model.

**Indexing callers.** The library (`indexing/use_cases/embedding_input.rs`, `index_file/prepare.rs`, `reindex_document.rs`), web ingestion (`web/services/ingestion/service.rs:177`) and every passage index (`infrastructure/passage_index/build.rs`, `BATCH_SIZE = 16`, turn-taking through `EmbedTurns`) call `split_text`, `embed_batch`, and, where enabled, `embed_span_chunks` / `embed_batch_with_sparse`. Course progress measures pace from `embedded(done, total)` (`course_index/job.rs:111-135`).

**Evals.** `evals/retrieval/` (v3 fixture: 379 documents, 194 questions); `cargo run --example retrieval_eval -- --mode production` builds the embedder as `CandleEmbeddingService` (`examples/retrieval_eval/production.rs:45`). 2026-09-19 results: late chunking "worse than chunk-first on every metric", off by default; the shipped recipe runs `--sparse off`. `throughput_bench.rs` is an ignored test over real weights.

## 2. What was measured (M3 Max, 36 GB; Qwen3-Embedding-0.6B)

Raw material: `/private/tmp/claude-501/-Users-josh-Code-lattice-temp/92a5efd0-704e-4be8-b2c3-a11600bad2a8/scratchpad/embed-bench/` (`scripts/serve.sh`, `llama_bench.py`, `compare.py`, `out/*.log`). Server launched with `--embedding --pooling last -ngl 99 -fa on -np N -c C -b UB -ub UB`; requests to `/v1/embeddings`.

| Measurement | Result |
|---|---|
| Vectors, Candle vs llama.cpp **F16**, 480 passages + 45 queries | cosine min 0.99999, p50 1.00000; identical top-1 45/45; identical ordered top-10 37/45; max score difference in a top-10 0.0012. Candle batch-1 vs Candle batch-16 itself: 42/45, so the remaining disagreement is float noise. |
| Vectors, Candle vs llama.cpp **Q8_0** | cosine min 0.99922, p50 0.99965; top-1 45/45; ordered top-10 18/45; max score difference 0.0065. A different space. |
| Metal, model alone, F16, 16 passages per call | prose 15.9–18.3 passages/s (4,214–4,852 tok/s); code 8.6–12.7 passages/s (4,280–5,127 tok/s). Whole set in one call: +4–8 %. |
| Metal, Candle after #134 (one passage per pass) | prose 3,849 tok/s, code 3,381–3,496 tok/s (`pairs.log`). Index loop end to end today: 13.9 passages/s prose, 6.5 code. |
| CPU, 8 threads | Candle F32: 78–152 tok/s. llama.cpp F16: 720–886 tok/s. Q8_0: 1,211–1,296 tok/s in the clean run (one earlier run gave 152–200 tok/s; cause not identified, so Q8_0 CPU pace is **unverified**). |
| Startup on a warm disk | `load_model` done 1.7 s after spawn (`server1.log`), `/health` 200 shortly after. Measured once. |
| Over-long input | HTTP 400 `{"error":{"code":400,"type":"exceed_context_size_error","n_prompt_tokens":3002,"n_ctx":2048}}`. The server names the numbers; no probing needed. |
| `-b 2048 -ub 512` with a 1,002-token input | Server logs `embeddings enabled with n_batch (2048) > n_ubatch (512)… setting n_batch = n_ubatch = 512` and still embeds it: Qwen3-Embedding is causal (`print_info: causal attn = 1`), so a sequence may span micro-batches. Non-causal BERT-family models assert `n_ubatch >= n_tokens` (llama.cpp source; see §19). |
| GGUF header, f16 file | `arch = qwen3`, `n_ctx_train = 32768`, `n_embd = 1024`, `n_layer = 28`, `pooling type = 3` (last), `file type = F16`. The pooling mode is in the file; `--pooling` can be omitted. |
| Compute buffer at `-ub 2048`, Metal | 114 MiB. |
| `/v1/embeddings` response | `{model, object, usage{prompt_tokens,total_tokens}, data[{index, embedding[1024]}]}`; vectors L2-normalised (`--embd-normalize` default 2). Candle also L2-normalises (`forward.rs:122`). |
| Token counts | llama.cpp `usage.prompt_tokens` totals equal Candle's HF-tokenizer totals on all three sets (64,599 / 79,638 / 42,483 tokens). |
| Files | `Qwen3-Embedding-0.6B-f16.gguf` 1,197,629,632 bytes, SHA-256 `421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340`; `-Q8_0.gguf` 639,150,592 bytes, `06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439`. Both equal the LFS hashes Hugging Face shows for `Qwen/Qwen3-Embedding-0.6B-GGUF` (tree head `370f27d7550e0def9b39c1f16d3fbaa13aa67728`, Apache-2.0). |

## 3. Goals, non-goals, rules

Goals: embeddings on every platform through the bundled llama-server; F16 vectors equal to today's; no re-embed for the owner after a hand conversion; one process manager for every local server; Candle embedding code deleted; Activity and course progress show the server; a parity test, a throughput benchmark and a retrieval eval that prove it.

Non-goals (named so nobody builds them by accident): reranker and Whisper on llama.cpp/whisper.cpp (§11); a remote `/v1/embeddings` backend (§10, deferred, port ready); per-token embeddings (`--pooling none`); Q8_0 as a shipped option (§18 Q1); any converter code in the app.

Rules the design obeys: (1) not built for one machine; (2) no probing a server for capabilities, local device detection through the sidecar's `--list-devices` is allowed; (3) no arbitrary limits or timeouts, every number is traced to the chunker, the model header, a measurement or a user setting, and no new wait constants are added; (4) no machine-fitted context sizing (§5.1 shows the embedding window is the chunker's limit, not the machine's); (5) pre-release, no shims, schema changes in a new dated migration; (6) the owner converts his data by hand; (7) every PR that touches storage opens with "What changes for you on first start" (§13); (8) at most three builders running `cargo check` only (§14); (9) a Windows checklist (§15).

## 4. Architecture

### 4.1 The port after this change

`EmbeddingPort` keeps: `embed_single`, `embed_query`, `embed_batch`, `split_text`, `window_parts`, `model_identity`, `dimension`, `is_ready`. It loses: `uses_late_chunking`, `embed_span_chunks`, `supports_sparse`, `embed_sparse_batch`, `embed_batch_with_sparse`, `embed_sparse_query`, `sparse_not_supported`, `span_chunk_texts`, `SpanEmbeddingGroup`, and `accelerator_bytes` (the server's bytes are counted by the sidecar registry, §4.4; keeping both would count them twice). Every fake implementor is trimmed to match. `LoadedEmbeddingModelPort` stays (health and Activity read it). `EmbeddingIndexBinding::bind_first(model)` loses its `late_chunking` argument.

Nothing in the port names llama.cpp, a process, a file or a GPU. A remote adapter (§10) implements the same eight methods.

### 4.2 The adapter: `features/embedding/llama_cpp/`

New module, replacing `candle_service/`:

| File | Holds |
|---|---|
| `mod.rs` | `LlamaEmbeddingService` and `impl EmbeddingPort`. |
| `facts.rs` | `EmbeddingModelFacts { identity: ArtifactIdentity, dimension: usize, pooling: Pooling, trained_tokens: u32, architecture: String, display_name: String }`, read from the GGUF header with `gguf_metadata::read_model_info` (two new fields on `GgufModelInfo`: `pooling_type: Option<u32>` from `{arch}.pooling_type`, `file_type: Option<u32>` from `general.file_type`; the key names are llama.cpp's `LLM_KV_POOLING_TYPE` / `LLM_KV_GENERAL_FILE_TYPE`, **confirm against the pinned f16 file in the reader's unit test**). A header without a pooling type, or with `none` (0), is refused at activation with the reason: `/v1/embeddings` requires pooled output. |
| `launch.rs` | `EmbeddingLaunch { gguf: PathBuf, gpu: bool, window_tokens: u32, slots: u32 }` and `trait EmbeddingServerStarter { async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningEmbeddingServer>> }`. `RunningEmbeddingServer { endpoint, api_token, gpu_bytes: Option<u64>, alive: Arc<dyn Fn() -> bool> }` is the only thing the adapter knows about the process. The trait is implemented in the composition root (§4.3), and by a wiremock-backed fake in tests. |
| `client.rs` | The HTTP side: `POST {endpoint}/v1/embeddings` with `{"input": [...]}` and `Authorization: Bearer`, response sorted by `index`, `usage.prompt_tokens` logged at `debug` for pace diagnostics. Error mapping in §5.2. |
| `identity.rs` | `space_identity(file_identity: &ArtifactIdentity, prefixes: &EmbeddingPrefixes) -> String`: the file hash, plus `+prefix-<id>` when the family prefixes documents (today's rule minus the late-chunking suffix). Shared with `interfaces/di/modules/search.rs::index_identity`, which must compute the same string before the model loads. |
| `tests.rs` | Unit tests against wiremock (§12.1). |
| `parity_tests.rs` | The ignored live parity test (§12.2). |
| `throughput_bench.rs` | The ignored benchmark (§12.3), rewritten from today's. |

`LlamaEmbeddingService` fields: `server: Arc<RunningEmbeddingServer>`, `http: reqwest::Client`, `facts`, `policy: InputPolicy` (unchanged module; its tokenizer comes from `tokenizer.json` beside the GGUF, its `max_tokens` from §5.1), `prefixes: &'static EmbeddingPrefixes` (resolved by `prefixes_for(model_id)`, then by architecture: `qwen3` → `QWEN3_INSTRUCT`, else `NONE`; `config.rs::resolve_prefixes` is reduced to that), `slots: Semaphore(launch.slots)`, `in_flight: AtomicU32`, `query_cache` (the existing 256-entry LRU, unchanged).

Constructors: `connect(server, facts, policy, prefixes)` (no process knowledge; what the eval example and a future remote adapter use) and `start(starter, model: &DownloadedModel, launch)` (reads the header, starts the server, connects).

Who creates it: `EmbeddingLoader::try_load_active_embedding_model` (`loading.rs`) replaces `CandleEmbeddingService::open(&dir, identity).with_strategy(strategy)` with `LlamaEmbeddingService::start(...)`; `Container::open_embedding_for_activation` (`di.rs`) likewise; the search module's `backfill_model` loader (`interfaces/di/modules/search.rs:275`) likewise. The loader gains an `Arc<dyn EmbeddingServerStarter>` field, handed in by `Container::active_embedding()`.

### 4.3 One process manager, two kinds of server

The embedding server is started through the existing `SidecarManager::start_shared(app, SidecarConfig)`, so it inherits preflight, the fallback ladder, port reservation, the API key in the environment, the silence-bounded readiness wait, the Windows Job Object, `kill_all` on exit, and the registry's GPU accounting. Two changes inside `features/llm/engine/sidecar_manager/`:

1. `SidecarConfig` gains `pub embedding: Option<EmbeddingServing>` with `EmbeddingServing { ubatch: u32 }` (`Eq + Hash` derive unchanged). `Some` means "an embedding server of this GGUF". The field is part of the key, so a chat server and an embedding server of the same file would be two processes (they never are: no GGUF is both).
2. `build_server_args` (`readiness.rs:70`) branches on it. Embedding mode emits `-m <gguf> --embedding -c <slots × window> -np <slots> -b <ubatch> -ub <ubatch> -ngl <0|99> -fa auto --host 127.0.0.1 --port <p> --no-webui` and nothing else: no `--jinja`, `--ctx-checkpoints`, `--cache-reuse`, `--cache-ram`, speculative decoding, `--mmproj`, `-ctk/-ctv`. `--pooling` is not passed (the GGUF declares it, §2). `--embd-normalize` is left at its default (2, Euclidean), which matches Candle's L2 normalisation.

The feature-edge ratchet (`scripts/rust-architecture-check/feature-graph.txt`) holds `features::embedding → features::llm` at zero, so the adapter never imports the sidecar manager. The bridge lives where both are visible: a new `interfaces/di/modules/embedding.rs` implements `EmbeddingServerStarter` as: build the `SidecarConfig` (`model_path = gguf`, `n_gpu_layers = 99 if launch.gpu else 0`, `context_size = slots × window`, `parallel_slots = Some(slots)`, `embedding = Some(EmbeddingServing { ubatch: window })`, everything else default/false), ask `LlmInstances` to make room (§4.4), call `SidecarManager::start_shared`, wrap the `SidecarHandle` as `RunningEmbeddingServer` (`gpu_bytes` from `SidecarRegistry` arithmetic). Alternative rejected: an `allow features::embedding features::llm` line in `feature-graph.txt`; it would work, but the composition-root pattern is the one `AppEmbedders` already uses and adds no edge.

`launch.gpu` comes from `BackendDevices::detect(app)` (ggml's `--list-devices`): true when any device `is_accelerator()`. This is the same local detection the chat server uses; it is not a probe of a server. On an Intel Mac the Primary binary is the CPU build, so `-ngl 99` is harmless and the detector reports no accelerator anyway.

### 4.4 Lifecycle

**Start.** Lazy, as today: the first `get_or_load_embedding` (a search, an index build, activation) starts the server. `EmbeddingRuntime` keeps its single-flight load and its `invalidate`.

**Readiness.** The existing `await_ready`: `/health` 200, silence bound reset by every log line and every 503. The design adds no timing constant. The Activity drawer shows "Loading Qwen3 Embedding 0.6B" from the existing `EmbeddingActivity` source while this runs (`features/embedding/activity.rs` on #142).

**Memory beside the chat model.** The registry already prices every running server from its GGUF header (`resident_gpu_bytes`, `other_models_gpu_bytes`), and the embedding server is a registered server with a real `context_size`, so chat admission sees it without new code. For Qwen3-Embedding-0.6B the price is the file (1,197,629,632 bytes) + KV for `slots × window` tokens (28 layers × 2 × 8 KV heads × 128 × 2 bytes ≈ 112 KiB per token; 8,192 tokens ≈ 896 MiB) + the compute buffer (`compute_buffer_bytes`; 114 MiB measured on Metal). `LlmRuntime::in_app_model_bytes` and its `LoadedEmbeddingModelPort` field are removed (`features/llm/loading.rs:70-90`, constructor at `interfaces/di/container.rs:215`), or the server would be counted twice.

**Making room the other way.** Before the starter calls `start_shared`, it runs the same admission as a chat model: `LlmInstances` gets a `pub(crate) async fn make_room_for(&self, config: &SidecarConfig)` that prices `config` and evicts idle LLMs exactly as `acquire` does (`instances.rs:403-430`), refusing nothing. A 2.2 GB embedding server rarely needs it; on an 8 GB card beside a 6 GB chat model it does.

**Idle.** No timer. The embedding server is unloaded in three cases only: (a) chat admission needs the memory and the server is idle (no request in flight, `EmbedTurns::standing()` empty); for this, `LlmInstances` gains `with_evictable(Arc<dyn IdleServer>)` where `trait IdleServer { fn idle_gpu_bytes(&self) -> Option<u64>; fn stop(&self); }` lives in `features/llm/instances/admission.rs` and the embedding runtime implements it in the composition root; (b) the active model changes (`invalidate_embedding_cache`, which now also stops the process); (c) app exit (`kill_all`). Alternatives rejected: a TTL like OCR's 60 s (an arbitrary number; OCR is wanted for a minute at a time, embeddings are wanted every search), and never unloading (today's behaviour, but the admission path already exists and this is the one honest trigger).

**Crash.** `RunningEmbeddingServer::alive()` reads the handle's child state. On a transport error (connection refused/reset) the adapter checks `alive()`; if the child died, it calls `runtime.invalidate()`, which stops and forgets the handle, reloads once through the single-flight path (which re-runs admission and the fallback ladder), and retries the same batch once. A second failure returns the error with the server's log tail, the way chat does. An HTTP error from a live server is returned as is (§5.2), never retried. Retries have no timer: the readiness wait is the retry's only wait.

**Stop.** `invalidate_embedding_cache` → the `SidecarHandle` drops → `kill()`; app exit → `kill_registered_sidecars`.

### 4.5 Activity and course progress

- `EmbeddingActivity` (#142) already lists the model while it loads. It extends to the running server: one item `kind = "model.embedding"`, title the catalog name, `state = Running`, `detail = "<n> requests in flight"` from `in_flight`, `uses = [GPU bytes]` from `RunningEmbeddingServer::gpu_bytes`, `since` from the handle. The indexes embedding through it already appear as their own items with pace (`FolderActivity`, `LearningActivity`) and `EmbedTurns::standing()` says who waits for whom; nothing changes there.
- `ModelActivity` (`features/llm/activity.rs`) lists `LlmInstances::loading()`; the embedding server is not an `LlmInstances` entry, so it stays in `EmbeddingActivity`. Both feed the same `activity_snapshot`.
- Course progress (`CourseProgress::embedding/embedded`) measures pace from the loop and is backend-agnostic. No change; the number goes up.

## 5. Server settings and request shape

### 5.1 Flags, each with its source

| Flag | Value | Derived from |
|---|---|---|
| `--embedding` | on | the server serves embeddings only. |
| `-c` | `slots × window` | per-slot window × slot count; llama-server allots `n_ctx / n_parallel` to each slot (`n_ctx_slot = 2048` in the logs for `-c 8192 -np 4`). |
| `window` | `min(n_ctx_train, MAX_PASSAGE_TOKENS)` | `n_ctx_train` from the GGUF header; `MAX_PASSAGE_TOKENS = 2048` is today's existing per-passage ceiling (`candle_service/loading.rs:100`, catalog `context_length`), a retrieval decision ("a 2048-token vector is a blurry average", `input_policy.rs:8`), moved to `input_policy.rs` as a named constant. It is the longest text `split_text` ever produces, so the window is the chunker's limit, not the machine's. This is why rule 4 holds: nothing here reads RAM or VRAM. A BERT-family model picked through Hugging Face search with `n_ctx_train = 512` gets window 512. |
| `-np` | 4 | llama-server's own default slot count in this build (auto = 4; the chat server relies on it, `startup.rs:21-25`). Passed explicitly only so `-c` can be computed. Not a tuning knob in Settings. |
| `-ub`, `-b` | `window` | a non-causal model must see its whole input in one micro-batch (llama.cpp asserts `n_ubatch >= n_tokens`; the server now clamps `n_batch` to `n_ubatch`, §2). Setting both to the window makes every supported family work the same way; the cost is the compute buffer (114 MiB at 2048 on Metal). |
| `-ngl` | 99 or 0 | `BackendDevices` reports an accelerator or not; the fallback ladder handles a GPU that then fails. |
| `-fa` | `auto` | the build's default; ggml enables it where the backend supports it. Parity was measured with FA on (Metal) against Candle (no FA): equal, so FA does not move vectors beyond float noise. Vulkan FA behaviour is **unverified**; the Windows checklist (§15) compares `auto` with `off`. |
| `--pooling` | not passed | the GGUF carries `pooling_type` (verified for the f16 file). Refuse a file that lacks it. |
| `--embd-normalize` | default (2) | Euclidean, equal to Candle's `l2_normalize`. |
| `--host`, `--port`, `--no-webui`, `LLAMA_API_KEY` | as chat | loopback only, reserved port, key in the environment. |
| threads | default | not set, as for the chat server; the CPU benchmark records what the default gives. |

### 5.2 Requests

- Documents: `embed_batch(texts)` prepends the family's document prefix (empty for Qwen3), sends all texts of the batch (16 from the passage index, up to 100 from `generate_embeddings_batch`) as one `input` array, takes one slot permit, and returns vectors in input order. The server spreads the inputs over its slots.
- Queries: `embed_query(text)` applies `prefixes.query_input` (`Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:` for Qwen3), checks the LRU, sends a one-element `input`. The query waits at most for the batch in front of it, as today.
- Empty text is refused before any request (`AppError::InvalidInput`), as today.
- Dimension: `facts.dimension` from the header (`n_embd`), asserted equal to the first response's vector length; a mismatch is a hard error naming both numbers. Matryoshka truncation stays in the vector index (`VectorIndexCompression::truncated`), untouched.
- Errors: HTTP 400 `exceed_context_size_error` → `AppError::InvalidInput` carrying `n_prompt_tokens` and `n_ctx` (it means the chunker and the server disagree about length; a bug, not a retry); other 4xx/5xx → `AppError::EmbeddingFailed { reason: <server message> }`; transport error → §4.4 crash path.
- No hidden requests: no warm-up call, no `/props`, no `/tokenize`, no `/v1/models`. The first real request tells us everything the response shape can.

### 5.3 Parity contract

The F16 GGUF of a checkpoint and Candle's F16/F32 run of the same checkpoint are the same vector space when, over the reference set (§12.2), every vector pair has cosine ≥ 0.9999 and the top-1 agrees on every query. Measured: 0.99999 / 45 of 45. Q8_0 fails the first clause (0.99922), which is why it is a different space (§6).

## 6. Platforms and precision

**Every platform, confirmed.** The recommendation to ship this on macOS too stands, for three reasons: (1) one engine for every local model on every OS, one process manager, one set of failure modes; (2) the Mac also gains speed (model alone +10–40 %, batching that Candle on Metal could not use); (3) Candle embedding code is deleted only if nothing depends on it, and a Mac-only Candle path would keep 7,800 lines alive for one platform. Intel Macs: Candle F32 on CPU → llama.cpp F16 on CPU, 5–10× by the M3 CPU measurement (**the ratio on an Intel Mac is unverified**).

| Platform | Binary | Backend | Route |
|---|---|---|---|
| macOS arm64 | `llama-server-aarch64-apple-darwin` | Metal | Primary, `-ngl 99`. |
| macOS x86_64 | `llama-server-x86_64-apple-darwin` | CPU | Primary (it is a CPU build); detector reports no accelerator → `-ngl 0`. |
| Windows x64 | `llama-server-x86_64-pc-windows-msvc.exe`, fallback `llama-server-cpu-…` | Vulkan, CPU | Primary with `-ngl 99` when a device is listed; the existing ladder falls to `-ngl 0`, then to the CPU build. |
| Linux x64 | `llama-server-x86_64-unknown-linux-gnu`, fallback `llama-server-cpu-…` | Vulkan, CPU | same as Windows. |

No CUDA, ROCm or SYCL build exists in the lock; nothing here needs one. The build is picked by the platform (compile time) and the ladder (run time), never by asking a server.

**Precision: F16 everywhere.** F16 is the only precision that keeps today's vector space (§2). Q8_0 is 1.4–1.7× faster on the CPU in the clean run but is its own space: choosing it later means re-embedding everything, and offering both means two identities for one catalog entry. Default: F16 on Metal, Vulkan and CPU. Q8_0 is §18 Q1.

**One default model, no device rule.** MiniLM leaves the catalog (§18 Q2), so `default_embedding_model(gpu)` loses its argument and returns Qwen3 on every machine (`crates/lattice-core/src/domain/models/embedding_defaults.rs`). The four `gpu_acceleration_available()` calls in `first_run_setup.rs:135,223,260,309` and the one in `interfaces/commands/model_setup.rs:70` go with it. The device decides only `-ngl` (§5.1), through `BackendDevices`. `compute_device.rs` stays for the reranker and Whisper. Not to be confused: the reranker's `ms-marco-MiniLM` cross-encoder (`search/engine/reranker.rs`, `reranker_setup.rs`) is a different model and stays on Candle untouched.

## 7. Identity

### 7.1 The rule

The identity of a local embedding model is **the SHA-256 of its GGUF file**, written `sha256:<64 hex>`: the same `ArtifactIdentity` type, the same 71-character CHECK on `models.embedding_artifact_identity`, the same column. The vector-space key is `space_identity(identity, prefixes)` = the identity, plus `+prefix-<id>` when the family prefixes documents (E5, Nomic). No strategy suffix, since late chunking is gone.

Why the file hash and nothing else: the vectors depend on the weights, the quantisation, the pooling and the normalisation. The first three are bytes of the GGUF; the fourth is the server default we pin. `tokenizer.json` is deliberately not in the hash: it decides where chunks are cut, not what a stored chunk's vector is, and changed chunks are already re-embedded by content hash (`embedding_generation_vectors.content_hash`, passage stores' `content_hash`). The llama-server version is not in the hash either, as Candle's version was not; §7.2 says how a server bump is checked instead.

Consequences: F16 and Q8_0 of the same checkpoint are different spaces (correct, §2); two F16 conversions of the same weights by different converters are different spaces (conservative: a re-embed, never a silent mix); a user's own GGUF from Hugging Face gets an honest identity without any catalog knowledge. For Qwen3-Embedding-0.6B F16 the identity is `sha256:421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340`, which Hugging Face prints on the file page and `shasum -a 256` reproduces.

`infrastructure/ml/artifact_identity.rs::compute(dir)` becomes `compute(file: &Path)` streaming the one file (64 KiB reads as today); `establish(model)` reads the recorded identity or hashes the GGUF in the background, as today. The download engine already produces the file's SHA-256 (`transfer.rs:67`), so activation straight after a download reuses it rather than hashing 1.2 GB twice.

### 7.2 How it is proven honest

An identity may only claim "same space as Candle's" if the vectors are. The proof is the parity test (§12.2) over reference vectors produced by Candle before Candle is deleted and committed under `evals/embedding/reference/`. It asserts §5.3 for every curated GGUF. It runs: before PR 3 merges (by the coordinator), on every sidecar bump (added to the bump procedure in `crates/lattice-desktop/binaries/README.md`), and on every curated GGUF pin change. If a future llama.cpp changed a kernel enough to fail it, the fix is a new catalog pin with a new identity, not a quiet drift.

### 7.3 Migration (one new dated file)

`crates/lattice-desktop/migrations/20261020120000_llama_cpp_embeddings.sql`:

```sql
-- Sparse retrieval is removed (no curated model has a sparse head; every eval ran with it off).
DROP INDEX IF EXISTS idx_chunk_sparse_terms_lookup;   -- init_schema.sql:1420
DROP TABLE IF EXISTS chunk_sparse_terms;
-- Safetensors embedding models cannot be loaded by this version. Their rows go; their files stay for the user.
DELETE FROM models WHERE model_type = 'embedding' AND storage_kind = 'local_dir';
```

`embedding_generation_vectors`, `text_embeddings`, the `models` CHECKs and every index file keep their shape; only their identity strings differ, and a different identity is already handled: the library prepares and backfills a new generation (`generation::prepare`), every passage index drops its vectors file and re-embeds (`vectors.rs:7-11`). The settings contract loses `embedding_strategy` (`crates/lattice-core/src/application/contracts/settings/retrieval.rs:123`); no settings contract uses `deny_unknown_fields` (verified by grep), so a saved settings file with the old key still loads and the key is dropped on the next save.

### 7.4 The owner's hand conversion (outside the repo)

Josh runs this once, on a copy first, then on his data. Inputs: `OLD = sha256:<today's identity>` (from `SELECT embedding_artifact_identity FROM models WHERE is_active_for_embedding = 1`), `NEW = sha256:421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340`. Steps:

1. Download `Qwen3-Embedding-0.6B-f16.gguf` and `tokenizer.json` into `~/.cache/lattice/models/qwen3-embedding-0.6b/` (the GGUF from `Qwen/Qwen3-Embedding-0.6B-GGUF` at `370f27d7…`, the tokenizer from `Qwen/Qwen3-Embedding-0.6B`); check the hash.
2. `lattice.db`: `UPDATE embedding_generation_vectors SET model_identity = NEW WHERE model_identity = OLD; UPDATE text_embeddings SET model_name = NEW WHERE model_name = OLD;` then insert the `models` row for the GGUF (`storage_kind = 'local_file'`, `storage_path` = the file, `model_type = 'embedding'`, `architecture = 'qwen3'`, `is_active_for_embedding = 1`, `embedding_artifact_identity = NEW`) and let the migration delete the safetensors row (or delete it by hand).
3. Library index: rename `usearch-sha256-<old hex>[-<layout>].usearch` to the `NEW` spelling (`:` → `-`), and the matching `.manifest.json`; if the manifest records the generation string inside, edit it too (**unverified**: read `infrastructure/vector_index/dimension_metadata.rs` before running).
4. Every `<data_dir>/folder-index/*/` and `<data_dir>/course-index/*/`: `UPDATE meta SET value = NEW WHERE key = 'identity' AND value = OLD` in `chunks.db`, and rename `vectors-<OLD>.usearch` → `vectors-<NEW>.usearch`.
5. Start the app. Expected: zero re-embedding (the "What changes for you" numbers in §13 are what happens *without* this script).

The sparse table is dropped by the migration; nothing to convert. Late-chunking generations never existed on his data (the setting was off).

## 8. Models: catalog, download, tokenizer, first start

**Catalog** (`curated/embedding.rs`): the `all-minilm-l6-v2` entry is deleted, with its references in `curated.rs`, `curated_tests.rs`, `catalog_cache.rs`, `classifier.rs`, `shared/constants.rs`, `embedding/prefixes.rs`, `vector_index/compression.rs`, the tests and snapshots that name it, and the frontend mocks (`src/tests/mocks/latticeApiMock.ts`, `SearchTab.test.tsx`, `useDownloads.test.ts`, `BatchFileImport.test.tsx`); builders grep `all-minilm`/`AllMiniLm` and leave the reranker's `ms-marco-MiniLM` alone. The Qwen3 entry becomes `format: ModelFormat::Gguf` with one weights file plus `tokenizer.json`, each `ModelFileMetadata` carrying `size_bytes` and `checksum: Some(sha256)` so the engine verifies. Qwen3: `https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/370f27d7550e0def9b39c1f16d3fbaa13aa67728/Qwen3-Embedding-0.6B-f16.gguf` (1,197,629,632 B, `421a27e5…`) and `https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/resolve/<pinned revision>/tokenizer.json` (**revision and hash to pin at build time**). `supported_quantizations` becomes `["F16"]`. `context_length` stays 2048 and is documented as the per-passage ceiling (§5.1). `embedding_model_supports_matryoshka` unchanged.

**Download.** The multi-file path (`download_model/multi_file.rs`) already downloads a list with resume and checksums; the GGUF list is two files. `compatibility::check_download` runs for the GGUF (architecture in `llama-architectures.txt`). `build_safetensors_embedding_file_list`, `build_onnx_embedding_file_list`, `fallback_url_for_onnx_metadata`, the `sparse_linear.pt` HEAD probe and `SPARSE_HEAD_PT` are deleted. The row records `storage_kind = 'local_file'`, `storage_path` = the GGUF; `ModelLocation::enclosing_dir` finds `tokenizer.json` beside it.

**Tokenizer for chunking.** `InputPolicy` keeps the `tokenizers` crate (which stays in the tree for the reranker and Whisper regardless) and loads `tokenizer.json` from the model's directory. Counts matched the server's on 186,720 tokens of real text (§2); the parity test asserts `usage.prompt_tokens` equals `policy.count` for every reference passage so a divergence fails loudly. Alternatives rejected: `/tokenize` per passage (exact, but makes a synchronous chunker async across four call sites and ties chunking to a running server); rebuilding a tokenizer from the GGUF header (BPE merges are there, WordPiece needs more; a later option).

**Hugging Face search.** Embedding results list GGUF files like chat results do; the user picks one file, `tokenizer.json` is added when the repo has it. A repo without `tokenizer.json` downloads but cannot be activated for search, with the reason shown (**known limit, pre-release**). Supported architectures are whatever the pinned build lists; the catalog entry copy says so.

**First start on a fresh install.** Unchanged flow (`download_default_model`), with the GGUF list and one default model for every machine (§6). **First start after the upgrade** (§13): the migration has removed the safetensors row; the app behaves as "no embedding model active" (lexical search only, Explorer and course indexes wait), Settings → Models recommends the download, and shows the old folder's size with a Remove button (§18 Q4).

## 9. Sparse head, late chunking, other encoders

| Component | What it is | Who uses it | Decision | Evidence |
|---|---|---|---|---|
| `sparse_head.rs`, `SparseEmbedding`, `sparse_term_store_port.rs`, `search/engine/sparse_search/*`, `chunk_sparse_terms`, `prepare.rs:209-262`, `reindex_document.rs:286`, eval `--sparse` | BGE-M3's learned sparse term weights, scored as a third retrieval branch | only a BGE-M3 checkpoint fetched through HF search; no curated model | **Delete.** | No curated model has a head; the shipped recipe runs `--sparse off`; no result file records a sparse gain; llama.cpp cannot run the head (it is a separate `Linear` over per-token states), so keeping it means keeping Candle for one unevaluated model. BM25 (FTS5) remains the lexical branch of the fusion. |
| `late_chunking.rs` (pooling rules), `candle_service/spans.rs`, `uses_late_chunking`, `embed_span_chunks`, `EmbeddingStrategySettingDto`, `SearchTab.tsx` option, eval `--strategy` | embed a whole span once, mean-pool token states per chunk | off by default | **Delete.** | 2026-09-19 V3 results: "worse than chunk-first on every metric" (nDCG 0.7110 vs 0.7640 for the shipped index). Possible on llama.cpp only via `/embeddings` with `--pooling none` (per-token vectors, 8 MB per 2048-token span as JSON). Not worth carrying. |
| `qwen3_encoder.rs`, `ModelVariant`, `forward.rs::encoder_embed`, `batching.rs` | Candle forward passes: the cache-free Qwen3 encoder and the BERT-family encoders, with length-bucketed batching | `CandleEmbeddingService` only | **Delete** with the service. | llama.cpp's `qwen3` and `bert`-family graphs replace them (`llama-architectures.txt`). Encoders llama.cpp lacks (ModernBERT, MPNet, DistilBERT as such; XLM-R / BGE-M3 **unverified**, llama.cpp converts BGE-M3 as `bert`) are lost; none is curated. |
| `candle_service/*`, `generator.rs` ("legacy thin generator… Candle-backed shim"), ONNX mentions in `mod.rs`, `ports/mod.rs:56`, `metadata.rs` | the engine, a shim, stale docs | — | **Delete / rewrite.** `generate_embedding*` Tauri commands stay and call the port. | — |
| `compute_device.rs`, `candle-*` crates, `tokenizers`, `infrastructure/ml/{instance_pool,model_cache,tokenizer}.rs` | shared Candle plumbing | reranker, Whisper | **Keep.** `README.md` there is rewritten. | — |

## 10. A user's own `/v1/embeddings` server (deferred, port ready)

Should embeddings be able to point at a user's OpenAI-compatible, Ollama or cloud endpoint, as chat can? Later, not now. Reasons: (1) rule 2 forbids learning the dimension, window and pooling by probing, so the user would declare them, and a wrong declaration corrupts an index silently; (2) a remote space has no file to hash, so its identity would be the user's declaration (`sha256(scheme ‖ host ‖ model name ‖ dimension ‖ prefix id)`), and a server-side model swap behind the same name would mix spaces with no way to notice; (3) nothing today asks for it, and this build already carries a format change.

What keeps the door open: the port names no process; `LlamaEmbeddingService::connect(endpoint, token, facts, policy)` is already the "talk to any `/v1/embeddings`" half; `EmbeddingModelFacts` is the declaration a remote adapter would be built from; `split_text`'s default implementation works with no tokenizer (whole text). A remote backend is one adapter, one settings surface and one identity rule, in a later design.

## 11. Later options, not in scope

- **Reranker on llama.cpp:** `--reranking` with `--embedding --pooling rank` serves `/v1/rerank` for models like `bge-reranker-v2-m3` (server README). Qwen3-Reranker would need a GGUF with a rank head; whether the pinned build supports it is **unverified**. Candle's reranker stays until measured.
- **Whisper on whisper.cpp:** it builds with `GGML_VULKAN=1` and runs on Vulkan (third-party guides; the official flag list is **unverified**). It would be a third pinned binary. Not touched here.

## 12. Tests, benchmark, eval

### 12.1 Unit tests (wiremock, already a dev-dependency)

In `features/embedding/llama_cpp/tests.rs`, against a wiremock `/v1/embeddings`:
- batch of 3 returns vectors in input order even when the server answers `data` shuffled by `index`;
- `embed_query` sends the Qwen3 `Instruct:` prefix and caches; `embed_batch` sends documents bare; an E5-prefixed fake sends both prefixes and its `space_identity` carries `+prefix-e5`;
- 400 `exceed_context_size_error` → `InvalidInput` with both numbers in the message; 500 → `EmbeddingFailed` with the server's text, no retry;
- connection refused with `alive() == false` → one reload through a fake starter, one retry, success; refused twice → error with the log tail;
- dimension mismatch between header and response → hard error;
- `space_identity` equals what `interfaces/di/modules/search.rs::index_identity` computes for the same row (one shared function, one test);
- `GgufModelInfo` reads `pooling_type` and `file_type` from a header fixture, and `facts` refuses `pooling_type = 0`;
- `build_server_args` in embedding mode emits exactly the flags of §5.1 and none of the chat flags; `SidecarConfig` with `embedding: Some` hashes differently from the same path without.

### 12.2 Live parity test (ignored)

`features/embedding/llama_cpp/parity_tests.rs`, `#[ignore]`, needs `LATTICE_EMBED_GGUF` and the bundled binary. Reference vectors live in `evals/embedding/reference/<catalog id>/{passages.json, queries.json, candle-vectors.json}`: 200 passages and 40 queries drawn from `evals/retrieval/synthetic-library-v3.json` (shareable text), embedded by today's `CandleEmbeddingService` **before PR 3 deletes it** (PR 0, §14), F16 on Metal and F32 on CPU recorded as two files. The test starts the server with §5.1's flags, embeds, and asserts §5.3 (cosine ≥ 0.9999 each, top-1 agreement), plus `usage.prompt_tokens == policy.count(text)` per passage. Results are recorded in `evals/embedding/results/<date>-parity-<model>-<machine>.md`. The scratchpad agreement set (480 passages of Josh's own text) is not committed.

### 12.3 Throughput benchmark (ignored)

`throughput_bench.rs` rewritten for the adapter: model alone at `BATCH_SIZE` per call and whole set per call; the passage index loop end to end (`infrastructure/passage_index/build::run`), so the loop's cost is visible; a slot sweep `-np ∈ {1, 2, 4}` to record what the default costs and buys. Output lines in the format of `out/llama_runs.log` (passages/s, tok/s, tokens, seconds), recorded in `evals/embedding/results/`. Run on the Mac by the coordinator and on the Windows PC by Josh (§15).

### 12.4 Retrieval eval

`examples/retrieval_eval/production.rs` constructs `LlamaEmbeddingService::connect` against a server the example spawns itself from `LATTICE_LLAMA_SERVER` (no Tauri in an example), with the `--sparse` and `--strategy` flags removed. Gate: `synthetic-library-v3` Recall@5 / nDCG@10 within run noise of the 2026-10-12 shipped-path baseline (`evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md`); since vectors are equal, expect identical ranks save float ties. Recorded in `evals/retrieval/results/` with the recipe.

## 13. Upgrade path: "What changes for you on first start"

Each PR that touches storage (PR 2, PR 3) opens with this section, measured by the coordinator on a **copy** of Josh's data (`lattice.db`, `<data_dir>/folder-index/*`, `<data_dir>/course-index/*`, `~/.cache/lattice/models/`) before merge. Numbers to measure and state:

- `SELECT count(*) FROM chunk_sparse_terms` → rows dropped by the migration (expected 0; say so).
- `SELECT count(*) FROM models WHERE model_type='embedding' AND storage_kind='local_dir'` → rows deleted (expected 1), and the folder's size on disk (expected ≈ 1.2 GB) that remains until removed.
- Library: `SELECT count(*) FROM text_chunks` and `SELECT count(*) FROM embedding_generation_vectors WHERE model_identity = OLD` → chunks that would re-embed without the hand conversion, and the time at the measured pace (13.9 → ~16+ passages/s for prose).
- Every `folder-index/*/chunks.db` and `course-index/*/chunks.db`: `SELECT count(*) FROM passages` and the `meta.identity` value → passages that would re-embed per index.
- Download on first start: 1,197,629,632 B + `tokenizer.json`; the time at Josh's measured link speed is his to state.
- Startup cost added: hashing nothing (identity reused from the download), first embedding request after launch waits for the server (≈ 2 s warm, measured once).
- Settings: `embeddingStrategy` key ignored/removed; "late chunking" and sparse options gone from `SearchTab.tsx`.
- Gates that open or close: library search is lexical-only until the GGUF is active; Explorer and course indexing wait for the embedder (they already do when no model is active); chat is unaffected.

With the hand conversion (§7.4) every re-embed number is zero, and the PR says that too.

## 14. Delivery

Three builders at most, each in its own worktree with a private target dir, running only `cargo check`. The coordinator runs the full gates once after the last builder finishes, then the live parity test, the benchmark, the eval and the §13 measurement.

| PR | Builder | Owns (files) | Depends on |
|---|---|---|---|
| **0 Reference vectors** | coordinator (small) | `evals/embedding/reference/**` generated with today's Candle path on main; `evals/embedding/README.md` describing how. No app code. | nothing; **merges first**, before Candle goes. |
| **1 Adapter and server** | A | new `features/embedding/llama_cpp/*` (§4.2); `features/embedding/{loading,di,runtime}.rs`; `features/llm/engine/sidecar_manager/{config,readiness}.rs` (`embedding` field, args branch); `features/llm/engine/gguf_metadata.rs` (two fields); `features/llm/instances.rs` + `instances/admission.rs` (`make_room_for`, `IdleServer`, `with_evictable`); `features/llm/loading.rs` (drop in-app bytes); new `interfaces/di/modules/embedding.rs`; `interfaces/di/container.rs` (wire the starter; constructor at `:215`); `features/embedding/activity.rs` (#142 branch or after its merge); `infrastructure/ml/artifact_identity.rs` (file hash). Port: remove `accelerator_bytes`; shrink `bind_first`. | PR 0. |
| **2 Catalog, download, migration, settings, UI** | B | `crates/lattice-core/.../curated/embedding.rs`, `embedding_defaults.rs`, `checkpoint.rs`; `features/llm/use_cases/download_model/{metadata,multi_file}.rs`; `features/model_management/**` (search results for GGUF embeddings, `set_active_embedding_model.rs`); `features/download/saga.rs`; `features/initialization/use_cases/first_run_setup.rs`, `interfaces/commands/model_setup.rs` (GPU rule via `BackendDevices`); `migrations/20261020120000_llama_cpp_embeddings.sql`; settings contract (`retrieval.rs`), `src/features/settings/components/{SearchTab.tsx, ModelCatalog/*}` and bindings export. | PR 0; builds against PR 1's `EmbeddingModelFacts` and `space_identity` names (fixed in §4.2) without needing its code. |
| **3 Deletion and callers** | C | delete `candle_service/**`, `qwen3_encoder.rs`, `sparse_head.rs`, `late_chunking.rs`, `generator.rs`; `features/embedding/{mod,commands,plugin,dto}.rs`; port methods (`embedding_port.rs`, `sparse_term_store_port.rs`, `value_objects/sparse_embedding.rs`, `mock_embedding_port.rs`) and every fake implementor; `search/engine/sparse_search/**`, `search/engine/vector_search/runtime_index.rs` (`bind_first` signature), `interfaces/di/modules/search.rs` (`index_identity` via `space_identity`, `backfill_model`, strategy removal); `indexing/use_cases/{embedding_input.rs, index_file.rs, index_file/prepare.rs, reindex_document.rs}`; `web/services/ingestion/service.rs`; `examples/retrieval_eval/**`; `infrastructure/ml/README.md`, `ports/mod.rs` doc; `Cargo.toml` (nothing removed: candle stays). | **PR 1 merged** (needs the adapter in `loading.rs` and the search module). |

Order: PR 0 → PR 1 and PR 2 in parallel → PR 3. Shared touch points, and who wins: `features/embedding/loading.rs` and `di.rs` are PR 1's; PR 3 only removes the `strategy` plumbing from them after PR 1 merges. `interfaces/di/container.rs` is PR 1's (one constructor line); PR 2 does not touch it. `interfaces/di/modules/search.rs` is PR 3's; PR 1 provides `space_identity` and PR 3 calls it. `embedding_port.rs` is PR 3's except the two removals PR 1 needs (`accelerator_bytes`, `bind_first`'s argument), which PR 1 makes first; PR 3 rebases on it. `set_active_embedding_model.rs` is PR 2's; it calls `artifact_identity::establish`, whose signature PR 1 keeps.

Gates the coordinator runs once (`.github/workflows/ci.yml`): `cargo check --workspace --all-targets`, `cargo test --workspace --lib --bins --tests`, clippy, `export_bindings --check`, fmt, `rust-architecture-check` (feature-edge zero, file-length ratchet: the new module files stay small), layer boundaries, repository barrier, data-contract check, `npm run type-check`, `contracts:check`. Then: parity test (§12.2), bench (§12.3), eval (§12.4), §13 measurement, and the `binaries/README.md` bump-procedure line.

## 15. Windows test checklist (Josh's PC)

1. **Device:** Settings → Models shows the Vulkan device from `--list-devices`; the default embedding model is Qwen3 (today it is MiniLM there).
2. **First start, fresh profile:** download the GGUF + tokenizer; activate; index one folder in Explorer. Activity shows "Qwen3 Embedding 0.6B" loading, then running with GPU bytes and requests in flight; the folder item shows a pace.
3. **Vulkan parity:** run the ignored parity test with `LATTICE_EMBED_GGUF` set; it must pass. If it fails only on cosine, rerun with `-fa off` (one-line env or arg override in the test) and record which.
4. **Speed:** run the throughput bench on Vulkan; record tok/s and passages/s next to the Mac numbers in `evals/embedding/results/`.
5. **CPU fallback:** make the Vulkan loader fail (e.g. point `VK_ICD_FILENAMES` at an empty file, or disable the GPU in Device Manager), start the app, index again: the log shows the ladder reaching `llama-server-cpu-…`; Activity shows no GPU bytes; record CPU tok/s (F16).
6. **Memory sharing:** with a chat model loaded, start indexing; then open a chat on a larger model than fits beside both: the embedding server is evicted only when idle, the chat loads, the next search restarts the embedding server (log lines name both).
7. **Crash:** kill the embedding `llama-server.exe` in Task Manager during an index build: the build continues after one reload; the log shows one retry, no loop.
8. **Exit:** quit the app; no `llama-server.exe` remains (Job Object).

## 16. Risks

- **CPU-only machines index more slowly.** With MiniLM gone, a machine with no usable GPU runs Qwen3 on the CPU: 720–890 tokens/s on the M3 Max's CPU, slower on a modest laptop (unmeasured). Indexing shows its pace and time left (#140), and search quality is the same everywhere. A user can still pick a smaller embedding model through Hugging Face search.
- **Vulkan behaviour.** FA, long micro-batches and Intel/AMD drivers are unverified until §15. The ladder and `-fa off` are the fallbacks.
- **KV memory of four slots** (≈ 0.9 GB for Qwen3 at 2048) is new GPU cost beside the weights. The slot sweep in the bench (§12.3) records what fewer slots cost; if 2 slots lose little, the default changes in a follow-up with its own measurement.
- **Non-curated models without `tokenizer.json`** cannot chunk (§8). Pre-release limit; the error names it.
- **Feature-edge ratchet.** Any `features::embedding → features::llm` import fails CI; the starter trait in `interfaces/di` is the only bridge (§4.3).
- **File-length ratchet.** New files must stay under the ratchet's limit; `llama_cpp/` is split as in §4.2 for that reason.
- **Another session's servers.** Two other llama-servers were running on this Mac while the §2 checks ran (Josh's `tauri dev` chat model and an eval server on :8089); the Metal numbers in §2 come from the earlier benchmark logs, not from those checks, but any re-run for the record should be done on a quiet machine.

## 17. Decisions

| # | Decision | Reason | Rejected |
|---|---|---|---|
| D1 | Embeddings run in a second `llama-server` process, started through the existing `SidecarManager` with an `embedding` mode on `SidecarConfig`. | One manager, one ladder, one registry for memory, one kill path; the OCR precedent shows a second server works. | A standalone launcher like OCR's (duplicates preflight, ports, Job Object, accounting). |
| D2 | The port stays, minus sparse, late chunking and `accelerator_bytes`. | The eight remaining methods are what every caller uses; the removed ones have no live model behind them. | A new `EmbeddingBackend` trait (nothing gained). |
| D3 | The bridge between embedding and the sidecar manager lives in `interfaces/di/modules/embedding.rs` behind `EmbeddingServerStarter`. | `feature-edge` is held at zero; the composition root already plays this role for `AppEmbedders`. | An `allow` line in `feature-graph.txt`. |
| D4 | Lazy start; unload only on memory pressure when idle, model change, or exit. No timer. | Rule 3; admission already exists; embeddings are wanted on every search. | OCR-style TTL; never unloading. |
| D5 | `-c = 4 × window`, `-np 4`, `-ub = -b = window`, `window = min(n_ctx_train, 2048)`, `-fa auto`, no `--pooling`, default normalisation. | Each value traces to the chunker, the header or the build's own default (§5.1); the machine is never read. | Deriving slots from the 16-passage batch (3.6 GB of KV); machine-sized windows. |
| D6 | F16 only, on every platform. | The only precision equal to today's space (§2); one identity per catalog entry. | Q8_0 on CPU (faster, different space; §18 Q1). |
| D7 | Every platform, including macOS. | One engine everywhere; Metal is faster too; Candle embedding code can be deleted outright. | PC-only (keeps 7,800 lines for one OS). |
| D8 | Identity = SHA-256 of the GGUF file; space key adds `+prefix-<id>` only. | Bytes decide vectors; HF publishes the hash; the CHECK and type are unchanged; honest by the parity test. | Hashing header fields or a catalog label (not tied to bytes); including the server version (Candle's never was). |
| D9 | Chunking keeps `tokenizers` + `tokenizer.json`; the parity test asserts counts equal the server's. | Sync, offline, counts matched on 186 k tokens; the crate stays anyway. | `/tokenize` per passage; a tokenizer rebuilt from the GGUF. |
| D10 | Sparse head and late chunking are deleted, with their table, settings and UI. | §9 evidence; neither can run on llama.cpp as built. | Hidden-but-kept (a shim by another name). |
| D11 | Remote `/v1/embeddings` deferred; `connect` and `EmbeddingModelFacts` keep it a one-adapter change. | Rule 2 makes it a declaration problem; nothing asks for it now. | Shipping it in this build. |
| D12 | One default embedding model (Qwen3) on every machine; the device decides only `-ngl`, through `BackendDevices`. | Candle says "no GPU" on every Windows/Linux machine, which handed them MiniLM; ggml knows the device. | Keeping `gpu_acceleration_available()`; keeping a device-based model choice. |
| D13 | Reference vectors from Candle are committed under `evals/embedding/reference/` before Candle is deleted. | The parity proof must outlive Candle. | Keeping Candle as a test-only dependency for embeddings. |
| D14 | Old safetensors rows are deleted by the migration; files stay. | Rule 5 for the rows; deleting a user's 1.2 GB without asking is not ours to do. | Auto-delete; keeping the rows as unloadable. |
| D15 | all-MiniLM-L6-v2 leaves the catalog. | Vulkan gives almost every PC a GPU; a CPU-only machine runs Qwen3 more slowly but searches as well; no official GGUF exists, so keeping it meant converting and hosting one and an unmeasured parity risk. | Our own MiniLM GGUF as a release asset; a community file pinned by hash. |

## 18. Decisions Josh made (2026-10-20)

- **Q1 Q8_0 as a CPU option?** No. F16 only (D6). Revisit only with a CPU-only user's numbers.
- **Q2 MiniLM?** Dropped from the catalog; Qwen3 for everyone (D12, D15). No model conversion or hosting.
- **Q3 A user's own `/v1/embeddings` server in this build?** No. The port stays ready for it (§10).
- **Q4 Remove the old safetensors folder on first start?** Never automatically. Settings → Models shows its size and a Remove button.
- **Q5 Slots?** Ship `-np 4`. A change to 2 needs the bench's slot sweep and its own follow-up PR.

## 19. Sources

Code and logs as cited. llama.cpp server README (flags, `/v1/embeddings` requires pooling ≠ none, `/embeddings` with `--pooling none` returns unnormalised per-token vectors, `/tokenize`, `/v1/rerank` with `--embedding --pooling rank`): https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md. Non-causal `n_ubatch >= n_tokens` assertion and the server's `n_batch = n_ubatch` clamp (#17912): https://github.com/ggml-org/llama.cpp/issues/21461 and the commit history cited there. Qwen GGUF repo and hashes: https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF (files `Qwen3-Embedding-0.6B-f16.gguf`, `-Q8_0.gguf`). Community MiniLM GGUFs (for Q2's alternative): https://huggingface.co/intextus/all-MiniLM-L6-v2-GGUF, https://huggingface.co/LLukas22/all-MiniLM-L6-v2-GGUF. whisper.cpp Vulkan (third-party): https://github.com/DomoticX/whisper.cpp-windows-vulkan, https://github.com/abdeladim-s/pywhispercpp. Vulkan flash attention reports: https://github.com/ggml-org/llama.cpp/discussions/12629, https://github.com/ggml-org/llama.cpp/issues/18946.
