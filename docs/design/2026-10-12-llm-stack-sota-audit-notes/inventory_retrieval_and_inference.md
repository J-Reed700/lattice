# Lattice retrieval + inference inventory (as of 2026-10-12, main + uncommitted tree)

Paths are relative to `src-tauri/src/` unless they start with `evals/`, `scripts/`, `src-tauri/` or `docs/`. Descriptive only; no judgments.

## 1. Chunking & parsing

- **Effective chunker is the embedder's token splitter, not the "semantic" chunker.** Indexing first builds chunks via `ChunkingStrategy::Semantic{max_tokens}` (`domain/value_objects/chunking_strategy.rs:170-240`, sentence-packing by *word* count, no overlap), then `prepare_structured_with_spans` discards them and rebuilds (`features/indexing/use_cases/embedding_input.rs:35-112`; caller `features/indexing/use_cases/index_file.rs:460`, comment at :1211).
- Settings values `chunk_size 800` / `chunk_overlap 120` (`application/contracts/settings.rs:968-969`) and the adaptive floor 600-1600 by word count (`chunking_strategy.rs:44-63`) only shape those discarded first-pass chunks. `features/indexing/engine/chunker.rs` (`SemanticChunker`, 800/120, `[Document: … | Page | Section]` prefix at :291-300) has no production constructor (grep found only tests/docs).
- **Real policy** (`features/embedding/input_policy.rs`):
  - Target passage size `RETRIEVAL_CHUNK_TARGET_TOKENS = 480` real-tokenizer tokens, plus prefix tokens, capped by the model window (`:14`, `:62-66`).
  - Binary search for the longest fitting prefix (`:84-104`). Then the cut prefers a paragraph break, then a sentence end, then a word boundary, all in the second half of the window (`cut_candidates`, `:189+`). Code fences and markdown tables are never cut (test `:503`).
  - **Zero overlap, on purpose** (`:128-134`). A trailing fragment under 15% is merged into the previous chunk (`:18`, `:142`).
- **Structure spans**: markdown-heading sections (with a `CodeFence` tracker), intersected with PDF page ranges. Code MIME types are never split by `#` headings (`embedding_input.rs:53-60`, `:200-330`). Each chunk stores `section` (breadcrumb "Parent > Child"), byte offsets and page (`:95-103`).
- **Contextual header on every embedded chunk**: `heading leaf | source group title | edition | file name | group description`, each truncated (100/64/24/48/64 chars), followed by `\n\n` (`embedding_input.rs:169-189`). The header is shrunk to ≤64 tokens (`:68-79`). The header is embedded but is not part of the stored chunk text (provenance field).
- **PDF**: `lopdf 0.44` (`src-tauri/Cargo.toml:115`) plus a custom content-stream layout walker (`features/indexing/engine/extraction/pdf_layout.rs:1-25`):
  - A line at ≥1.2× body font size becomes a `##`/`###`/`####` heading (`HEADING_RATIO`, `:33`, max level 4 `:38`, ≤140 chars `:43`).
  - ≥3 aligned columns × ≥2 rows become a markdown table (`:46-49`).
  - Fallback is lopdf `extract_text` (`pdf.rs:1-9`).
- **OCR: not active.** Image-only pages are flagged `needs_ocr` (`pdf.rs:7-9`). The bound `OcrPort` is `NoopOcr` (`interfaces/di/modules.rs:486-519`, :550). Curated OCR models (Qwen2.5-VL-7B Q4_K_M, Florence-2-large) exist in the catalog only (`domain/models/curated.rs:454`, `:477`).
- **Other formats**: docx, rtf, odt, xlsx, pptx, csv, html, markup, archives (`features/indexing/engine/extraction/*.rs`); plus a MIME map covering about 40 code/text extensions (`extraction/mime.rs:17-60`). HTML/web uses vendored `readability-js` plus `scraper` (`Cargo.toml:166,173`).
- **Re-chunk policy**: reindex infers `Semantic{max_tokens = p75(observed)*1.2 clamped 256-2048}` (`chunking_strategy.rs:70-104`, used at `indexing/use_cases/reindex_document.rs:192`), and the structured rebuild then re-applies the 480-token policy.
  - A change of embedding model, strategy or compression opens a new vector "generation" (separate index files) rebuilt from the SQLite vectors (`features/search/di.rs:100-130`).

## 2. Embeddings

- **Runtime**: in-process Candle `=0.10.2`, with the `metal` feature on macOS (`Cargo.toml:153-155,212-214`) and HF `tokenizers 0.15` (`:91`). No ONNX, fastembed or llama.cpp for embeddings.
  - Architectures: BERT, JinaBert, Qwen3, and others (`features/embedding/candle_service.rs:36,90`; `embedding/compatibility.rs:30-31`).
- **Curated models** (`domain/models/curated.rs:337-415`):
  - `qwen3-embedding-0.6b`: 1024-d, context 2048, safetensors (`:351-371`). Default where a GPU backend exists.
  - `all-minilm-l6-v2`: 384-d, context 512 (`:376-413`). Default on CPU-only machines (doc `:337-346`).
  - BGE-M3 is recognized by name (prefix table, sparse head) but has no curated entry (`model_manager.rs:189-228,740` references `bge-m3`).
- **Pooling**:
  - Qwen3: last-token pooling, then L2-normalize, in F16 on GPU and F32 on CPU (`candle_service.rs:1370-1371`, `:1505-1517`).
  - BERT family: mean or CLS, read from `1_Pooling/config.json`, default CLS (`:1314-1351`).
  - Late-chunk path: mean-pool per chunk range, then L2 (`:623-634`).
- **Instruction prefixes** (`features/embedding/prefixes.rs`):
  - Qwen3: query-only `"Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:"` (`:66-68`).
  - E5: `query: ` / `passage: `. Nomic: `search_query: ` / `search_document: `. BGE English: query instruction only (`:74-92`).
  - BGE-M3 and MiniLM: none (`:58`, `:122`).
  - A document-side prefix joins the vector identity (doc `:16-20`).
- **Batching**:
  - `MAX_BATCH_ROWS = 64`.
  - Padded-token budget 16×512 for encoders and 4×1024 for Qwen3 (`candle_service.rs:146,155,160`).
  - Query embedding LRU cache of 256 entries (`:165`).
- **Late chunking** is implemented (`embedding/late_chunking.rs:29-41`; `candle_service.rs:595-770`) but the default strategy is `ChunkFirst` (`application/contracts/settings.rs:164-172`).
- **Matryoshka / quantization**: the default *settings* value is `vector_index_compression = Truncated, dims 256, quantization I8` (`settings.rs:1002-1013`). It is honoured only for models listed as Matryoshka-capable, which today means only `qwen3-embedding-0.6b` (`curated.rs:433`, `:439-442`; gate in `interfaces/di/modules.rs:386-420`).
  - So a Qwen3 vault's HNSW index stores 256-d int8 vectors, with rescoring against the full-precision vectors (`search/engine/vector_search/rescore_store.rs`; `RESCORE_ORDER_MARGIN 0.05`, `usearch_index.rs:1019`).
  - No multi-vector/ColBERT.
- **Storage**: f32 vectors live in SQLite. The ANN index is USearch 2.23 (`Cargo.toml:97`), one generation per identity × compression, with a persisted manifest (`search/engine/vector_search/manifest.rs`, `persistence.rs`).
- **Learned sparse**: a BGE-M3 sparse head exists (`embedding/sparse_head.rs`) and `SparseSearchService` is wired, but `SPARSE_BRANCH_ENABLED = false` (`search/di.rs:34-39`, `:290-308`).

## 3. Index & first-pass retrieval

- **HNSW (USearch)**: cosine metric, `M (connectivity) = 32`, `ef_construction = 256`, `ef_search = 256`, all fixed constants (`search/engine/vector_search/usearch_index.rs:57-59`, `:289-293`). Scalar kind F32 or I8 (`:259-262`).
- **Exact-scan fallback**:
  - Brute force when the index holds ≤ `EXACT_SEARCH_MAX_VECTORS = 20_000` vectors, or when the scoped eligible set is ≤ 20k (`:65`, `:432-443`).
  - Scoped candidate cap `MAX_SCOPED_CANDIDATES = 8_192` (`:67`).
- **Lexical**: SQLite FTS5 `chunks_fts` with `tokenize='porter unicode61 remove_diacritics 2'`, plus a `chunks_trigram` FTS5 table (`src-tauri/migrations/20260916000000_init_schema.sql:233-250`).
  - Ranking uses FTS5 `bm25()` with default column weights (`search/engine/text_search/sqlite_text_search.rs:260-274`).
  - Query terms are OR-joined after normalization (`search/engine/fts_query.rs:95-111`).
  - A separate in-Rust `BM25Search` is also constructed (`search/di.rs:280`). Its use in the chat path was not traced.
- **Fusion**: weighted RRF, `score += w/(k + rank + 1)` (`search/engine/fusion.rs:66-80`; moved to `lattice-core` `application/contracts/search/fusion.rs` on 2026-10-16), with `k = DEFAULT_RRF_K = 10`, vector weight 0.7 and keyword weight 0.3 (`shared/constants.rs:60-64`).
  - When sparse is on, the lexical weight is split between BM25 and sparse (`hybrid_search.rs:427`).
- **Per-branch depth**: `limit × BRANCH_DEPTH(3)` (`search/use_cases/hybrid_search.rs:78`, `:396`).
- **Floors and caps**:
  - Vector cosine floor 0.15 in hybrid mode (`:83`).
  - `PER_DOCUMENT_CAP = 4` passages per document before others get a turn (`:88`).
  - The fused list is deduped by chunk id.
- **Chat k values**:
  - `kb_search_limit = clamp(tool_output.max_results=12, 16, 48)` gives **16** (`conversation/chat/retrieval/mod.rs:464-473`; `settings.rs:1143`, `kb_search_min_limit 16 / max 48` in `RetrievalTuningSettingsDto::default`).
  - With rerank on, the candidate pool is `min(limit×3, 64)` = 48 (`search/use_cases/hybrid_search/rerank.rs:12-36`), so each branch fetches 48 (or 144 with rerank).
  - All of this runs per planner query (1-3 queries) and is fused (`chat/retrieval/corpus_plan.rs:733-745`).
- **Truncation and expansion**:
  - The list is truncated to `kb_search_limit`, or 16 when start-at-beginning (`kb_retrieval.rs:386-388`).
  - Then **neighbour expansion**: for the top 8 anchors, add chunk_index ±1 *within the same section* (at most 3 rows), up to `limit+8` (`corpus_plan.rs:874-893`; SQL in `conversation/repository/workspace/retrieval.rs:186-204`).
- **Openings / section lookups**:
  - Planner-chosen `opening_document_ids` contribute up to 4 opening chunks each, round-robin (`corpus_plan.rs:746-768`).
  - Explicit section identifiers in the question pull `section_lookup` passages (`:769-777`).
- **Scoping**: an allow-list of space document ids plus this chat's attachments (`corpus_plan.rs:733-737`). Explorer turns skip library search unless forced (`chat/routing.rs:171-179`). Recency: `search/engine/recency.rs` exists (267 lines); its use in the chat path was **not determined**.

## 4. Query side

- **Turn intent classifier**: a utility-LLM JSON call, enable-only (it can turn on KB/web/follow-up and never off). Confidence threshold 0.5, timeout 15 s (`chat/intent.rs:1-3,25-26`; merged in `chat/routing.rs:141-151`).
- **Vault search gate**: `should_search_kb = force_kb || (router_may_search && action ∈ {NewSearch, UseLastDocument})` (`chat/retrieval/mod.rs:279-284`). So non-Explorer, non-closed-book turns search by default. Web and wiki run only when forced or enabled by intent (`:285-286`).
- **Router** (use last doc / new search / clarify): disabled by default; its model would be `phi-3.5-mini-instruct-q4_k_m`, with a 350 ms timeout (`settings.rs:947-953`).
- **Corpus planner (conversational query rewriting)**: a utility LLM reads a document catalog of filenames, openings and headings (up to ~16k tokens).
  - It returns JSON with 1-3 queries of ≤24 words, 0-4 opening doc ids, and `start_at_beginning`, under a `json_schema` constraint (`corpus_plan.rs:132`, `:646`).
  - Timeout 20 s, then a 10-minute cooldown after failure (`:36,44`). Output reserve 512 tokens (`:73`).
  - Follow-up history is passed in. Short follow-ups skip the planner using anchor terms (`kb_retrieval.rs:148-158`; `corpus_plan.rs:429-500`).
  - The fallback is a local plan (`:420`).
- **HyDE**: the **vault path does not use HyDE**; the KB interpretation is `HyDEInterpretation::raw_only` (`kb_retrieval.rs:198`). `HyDEService` (`search/hyde/`) is used only to generate web/wiki queries (`chat/retrieval/pipeline.rs:491-600`, `choose_web_query` `:631-710`).
- **Decomposition / multi-query**: only via the planner's ≤3 queries. No separate sub-question decomposition was found.
- **Sufficiency check and corrective retry**:
  - The verdict uses min top score 0.35 and min term coverage 0.4; retry is on by default (`settings.rs:899-913`; `kb_retrieval.rs:277-300`).
  - When insufficient, the planner is re-asked with `CORRECTION_INSTRUCTION` and the tried queries (`corpus_plan.rs:83`; `kb_retrieval.rs:301-356`).
  - The second pass is RRF-fused with the first (`corpus_plan.rs:902-913`) and re-reranked if rerank is on.

## 5. Reranking

- **Default off**: `enable_reranking: false` (`settings.rs:994`).
- **Downloadable model**: `cross-encoder/ms-marco-MiniLM-L-6-v2`, Candle `BertModel`, max length 512 (`search/reranker_setup.rs:47`; `model_management/model_manager.rs:55-59`; `search/engine/reranker.rs:388`).
- **Qwen3-Reranker** is also supported when the artifacts declare `Qwen3ForCausalLM`: yes/no logit scoring, max length 2048, publisher prompt (`reranker.rs:193-236`; `qwen3_reranker.rs:25-28`). No catalog or download entry was found for it.
- **k in / k out**: `rerank_max_candidates = 48` scored, query cut to 6000 chars (`settings.rs:1052-1053`; `chat/retrieval/rerank.rs`). The result is then truncated to `kb_search_limit` (16).
- **Rerank query**: message + HyDE text (None on the vault path) + ≤10 follow-up anchors (`chat/retrieval/rerank.rs`).
- **Blend**: `0.35 × (fused/max_fused) + 0.65 × clamp(rerank, 0, 1)` (`reranker.rs:304-316`).
- **When skipped**: whenever the planner selected opening documents (`kb_retrieval.rs:258`).
- **Lazy load**: init retry delay of 5 minutes after a failure (`reranker.rs:104`). No per-call latency timeout was found.
- **Measured latency**: 533 ms median with ms-marco on v3, versus 24.9 ms without (`evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md:14,17`).

## 6. Context assembly

- **Planner**: `BudgetAllocation::plan` (`application/services/context_assembler/budget.rs:205-260`).
  - Output reservation = `min(capacity/4, 32_768)`, at most capacity/2 (`:91-101`, `:218-221`).
  - Safety margin = `max(5% of capacity, 256)` (`:103-107`).
- **History pools** (bounded memory, default on, `settings.rs` `bounded_conversation_memory: true`):
  - Mandatory 25%, capped at 4096.
  - Summary 10%, capped at 1536.
  - Recall 15%, capped at 2048.
  - Recent 35% (`budget.rs:77-86`).
- **Evidence shares** (of the remaining room, in claim order):
  - Attachments 0.6.
  - Explorer folder 0.25 (floor 500, cap 8000).
  - Prior cited evidence 0.25 (cap 4000).
  - Web pages 0.4 (`budget.rs:345-370`).
  - The user's documents take the rest and are "ranked first" in the prompt (comment `:364-365`).
- **Other caps**: web pages are split by equal-share water-filling (`chat/retrieval/page_budget.rs`). Tool output excerpts are 4000 chars, `max_chars` 50 000 (`settings.rs:1141-1143`). Memory recall is 8 KB (`chat/memory_context.rs:41`).
- **Ordering**: openings first, then section lookups, then fused/reranked hits, then section neighbours (`corpus_plan.rs:746-777`, `:874-893`). The exact final passage ordering and eviction inside `chat/turn/assemble.rs` (472 lines) was **not read**.
- **Token counting**:
  - Budgeting uses `count_tokens` = chars ÷ `chars_per_token`. The default is 4.0 (`application/ports/llm_port.rs:39,183`), recalibrated online from server-reported usage by `TokenEstimator` (`features/llm/scheduler/scheduled.rs:59,112,216-229`; `scheduler/estimator.rs`).
  - The real `/tokenize` (`ServerTokenizer`) is only used through `count_tokens_exact`, whose only caller is memory recall (`chat/memory_context.rs:281-287`).
  - The llama.cpp client also clamps `max_tokens` from a chars/4 × 1.5 prompt estimate, with a floor of 1024 (`features/llm/llama_cpp/mod.rs:184-225`).
- **Max context**:
  - Settings `context_window` 131 072 and `max_tokens` 131 072, `local_context_window: None` (`settings.rs:1091-1093`).
  - The bundled sidecar's actual `--ctx-size` comes from §7, i.e. ≤32 768 under auto.

## 7. Inference config

- **Pinned llama.cpp**: `llama_cpp_tag v0.6.0`, release `llama/v0.6.0-r1` in `J-Reed700/lattice`, macOS min 13.3 (`src-tauri/scripts/llama-server.lock`; the file is under `src-tauri/scripts/`, not `scripts/`).
  - CI build flags: `-DGGML_NATIVE=OFF -DGGML_OPENMP=OFF`, and `-DGGML_METAL_USE_BF16=ON` on macOS (`.github/workflows/llama-build.yml:171-188`).
- **Exact sidecar argv**: `-m <gguf> --port <ephemeral> --host 127.0.0.1 --no-webui -ngl <n> --ctx-size <n> --jinja` (`features/llm/engine/sidecar_manager/readiness.rs:49-73`). The API key goes in via the `LLAMA_API_KEY` env var.
  - **Not passed**: `-fa`, `--cache-type-k/v`, `-b`/`-ub`, `--parallel`/`-np`, `-t`, `--reasoning-format`, `--reasoning-budget`, draft/speculative/MTP flags, `--cache-reuse`, `--slot-save-path`, `--mlock`/`--no-mmap`. All of these are left to llama-server defaults.
- **`-ngl`**: 99 when an accelerator is detected, else 0 (`sidecar_manager/config.rs:205`). The factory can override it (`engine/factory.rs:206-213`).
- **`--ctx-size`**:
  - Auto mode sizes from GGUF KV bytes/token and 70% of VRAM (`config.rs:75`, `:119-160`), clamped to 4 096–32 768 (`:85,91`).
  - Fallbacks: 8192 on GPU, 4096 on CPU (`:69,73`), and 2048 if RAM is under 8 GB (`:60-64`, `:224`).
  - A user override is possible, with warnings past the affordable or trained length (`:262-290`).
- **Request body** (`/v1/chat/completions`, `llama_cpp/mod.rs:248-313`):
  - `stream:true`, `stream_options.include_usage`, `return_progress:true`.
  - `temperature`, `top_p`, `top_k`, `repeat_penalty`, `max_tokens`.
  - Optional `reasoning_effort`, duplicated into `chat_template_kwargs`.
  - `tools` + `parallel_tool_calls:true`; `logprobs`/`top_logprobs`.
  - `response_format: json_schema`.
  - `id_slot` + `cache_prompt:true` when the scheduler assigned a slot.
  - **Not sent**: `min_p`, `presence_penalty`, `n_predict`, `enable_thinking`. A retry may set `cache_prompt:false` (`llama_cpp/retry.rs:139`).
- **Slots**: the scheduler reads the slot count from `/props`, falling back to 1. It pins conversations to slots by cache key (`llama_cpp/mod.rs:330-380`; `scheduler/mod.rs:30-133`).
- **Sampling**:
  - Chat defaults: temp 0.7, top_p 0.9, top_k 40, repeat_penalty 1.1 (`settings.rs:1087-1090`).
  - Utility LLM: the same settings with a 8 192 max-output ceiling (`features/llm/di.rs:41,257-266`).
  - Deterministic override (temp 0, top_p 1, top_k 1; `llm_port.rs:270-276`) is used by memory extraction (`conversation/memory/mod.rs:300`), handoff (`handoff.rs:188`) and Ollama JSON (`engine/ollama_client.rs:621`).
  - Claim verification and judge: temp 0 / top_p 1 / top_k 1 (`settings.rs:444-454`), `reasoning_effort:"low"` and a json_schema (`application/services/claim_verification/batch.rs:145-146`), with a 90 s budget and 4096 max output (`chat/verification/judge.rs:50,55`).
  - No sampling override was seen in the corpus planner request (`corpus_plan.rs:630-665`).
  - The utility model is user-set, and falls back to the chat LLM (`chat/retrieval/pipeline.rs:108-125`).
- **Timeouts**:
  - Sidecar stall 30 s (`llama_cpp/mod.rs:30`).
  - First-frame floor 60 s, plus prefill allowance at 20 tok/s per 2048-token batch (`retry.rs:10-20`). Retries up to 5 (`retry.rs`, `MAX_ATTEMPTS`).
  - Remote/user stall: default 30 s, range 15–180 (`settings.rs` `DEFAULT/MIN/MAX_LLM_STALL_TIMEOUT_SECONDS`).
  - Turn budgets: chat 30 min, deep research 2 h (`conversation/chat.rs:196-197`).
  - Health check 2 s; probe 30 s (`llama_cpp/mod.rs:33,139`).
- **Remote llama-server**: the same client (base URL + auth headers; `settings.rs:385-393`). It is probed with a test completion (`mod.rs:139-175`) and scheduled by URL from `/props`, or 1 slot with no pinning (`mod.rs:330-336`).
  - Ollama (`engine/ollama_client.rs`) and cloud providers (`features/llm/cloud.rs`) also exist; they were not inventoried.

## 8. Model catalog

- **Chat entries** (`domain/models/curated.rs:70-300`). All are GGUF Q4_K_M by default. Columns: entry, size, min/recommended RAM, context, tags.

| Entry | Size | Min / rec RAM | Context | Tags |
|---|---|---|---|---|
| tinyllama-1.1b | 0.7 GB | 2 / 4 | 2048 | chat |
| phi-3-mini-4k | 2.3 GB | 4 / 8 | 4096 | chat, code, reasoning |
| phi-3.5-mini | 2.4 GB | 4 / 8 | 128k | chat, code, reasoning |
| mistral-7b-v0.2 | 4.4 GB | 8 / 12 | 32k | chat, code, reasoning |
| llama-3.2-3b | 1.9 GB | 4 / 8 | 131k | chat, code, reasoning |
| **qwen3.5-2b** | 1.2 GB | 4 / 8 | 262k | tool-calling |
| **qwen3.5-4b** | 2.6 GB | 8 / 12 | 262k | tool-calling |
| **qwen3.5-9b** | 5.3 GB | 16 / 24 | 262k | tool-calling |
| llama-3.1-8b | 4.6 GB | 8 / 16 | 131k | tool-calling |
| mixtral-8x7b | 26 GB | 32 / 48 | 32k | chat, code, reasoning |

  - Line numbers for the rows above: `:73,96,119,143,166,189,212,235,259,282`.
  - The `tool-calling` tag gates tools under `--jinja` (`:33-58`).
- **First-run default by effective RAM** (RAM + VRAM, or unified memory) (`curated.rs:316-334`):
  - Under 8 GB: Qwen3.5-2B.
  - 8–16 GB: Qwen3.5-4B.
  - 16 GB or more: Qwen3.5-9B.
- **Embedding entries**: Qwen3-Embedding-0.6B (1024-d, Matryoshka) and all-MiniLM-L6-v2 (384-d) (§2).
- **Reranker**: ms-marco-MiniLM-L-6-v2 is the only downloadable one (§5).
- **Utility/judge model**: there is no dedicated catalog entry. It is user-selected via `set_active_utility_model` (`model_management/use_cases/set_active_utility_model.rs`), or else the chat model.
- **Other entries**: OCR (Qwen2.5-VL-7B, Florence-2) and Whisper transcription (`curated.rs:451-610`).
- **Mock catalog**: `application/ports/model_catalog.rs:574-730` holds a mock with tinyllama, phi-3, llama-3.2, qwen2.5 and mixtral, used in tests.
- **Remote eval model**: named `qwen3.8-27b-remote` in eval folders (`evals/conversation-memory/results/README.md`).

## 9. Evaluation

- **Retrieval harness**: the Rust example `src-tauri/examples/retrieval_eval/{main.rs,production.rs}` writes JSONL, which `scripts/rag_eval.py` scores.
  - Metrics: recall@k, nDCG@k, MRR@k, passage recall@k, per-dimension breakdown, latency. A missing query fails the run.
  - `scripts/compare_rag_eval.py`: paired query-cluster bootstrap.
  - `scripts/validate_rag_evals.py` and `scripts/eval_integrity.py`: integrity checks (schema v2).
  - Python tests exist: `scripts/test_*rag_eval.py`.
- **Datasets**:
  - `evals/retrieval/starter.json`.
  - synthetic-library v1/v2/v3. v3 is 379 docs, 194 queries, 1.61 M chars (`evals/retrieval/SYNTHETIC_LIBRARY_V3.md:12`).
- **Latest numbers** (v3, Qwen3-Embedding-0.6B, Metal, production mode, top-k 10, chunk-first, no compression, sparse off; `evals/retrieval/README.md:93-100`).

| Run | recall@5 | nDCG@5 | MRR@5 | passage recall@5 | median latency |
|---|---|---|---|---|---|
| 2026-10-09, chat orchestrator | 0.7755 | 0.6904 | 0.6978 | 0.7630 | 52 ms, contended machine |

  - No reranked run on 10-09; no checkpoint was present (README ~:108).
  - 2026-09-19 runs (`evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md:10-17`):

| Run | recall@5 |
|---|---|
| Before | 0.6946 |
| Before, ms-marco reranker | 0.7100 |
| After | 0.7755 |
| After, mrl256-i8 | 0.7640 |
| After, late chunking | 0.7110 |
| After, ms-marco reranker | 0.7755 (nDCG 0.6910) |

  - v2 recall@5 is 0.9706 (`:19`). The weakest v3 dimensions are noisy/OCR text (nDCG 0.344) and multi-hop (0.512) (`:41,45`).
  - Reranker study: `evals/retrieval/2026-09-18-RERANKER-RESULTS.md`.
- **Production mode does not exercise** the planner rewrites, HyDE, openings and section lookups, the corrective retry, workspace scoping, page numbers, or answer generation (`evals/retrieval/README.md:230-234`). The defaults for the run above differ from shipped settings, where compression defaults to truncated 256/i8.
- **Answer harness**: `scripts/chat_rag_eval.py` calls an OpenAI-compatible llama.cpp server with the default prompts and records pattern matches, citation structure, abstention signals and latency. Its own docstring says it is not the desktop chat path.
  - Latest output: `evals/retrieval/results/chat/qwen3.5-9b-local/2026-09-25-synthetic-v1-qwen3-local-qwen3.5-9b-chat.jsonl`. No summarized score was found in a doc.
  - Human evidence-bound review: `scripts/review_rag_eval.py`.
- **Memory evals**: `src-tauri/tests/conversation_memory_evals.rs`, where every test is `#[ignore]` and needs `LATTICE_EVAL_UTILITY_MODEL` and similar (`:14`, `:2921-3080`). Results live per model in `evals/conversation-memory/results/{qwen3.5-9b-local,qwen3.8-27b-remote}/` (latest 2026-09-26).
- **Learning / LLM-judge style evals**: `src-tauri/tests/teaching_course_evals.rs` (`:218,321,373,455`) and `learning_studio_evals.rs:330`, all `#[ignore]` and needing a configured provider.
- **Embedding tests needing real Qwen3 weights**: `#[ignore]` (`features/embedding/candle_service.rs:2240,2279`).
- **Runtime grounding check**: claim verification (lexical plus LLM judge) runs per turn (`conversation/chat/verification/`). It is a runtime feature, not an offline eval.

## Not determined (budget)
- Whether `search/engine/recency.rs` or the in-Rust `BM25Search` take part in chat retrieval.
- The final passage ordering and eviction in `chat/turn/assemble.rs`.
- Whether `local_context_window` feeds the sidecar's `with_context_override`.
- `DEFAULT_RESCORE_FACTOR`'s value.
- Summary scores for the 2026-09-25 chat answer run.
- The Ollama and cloud provider settings.
