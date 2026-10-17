# Feature coupling plan, 2026-10-16

**Status:** PR 1 (the checker's allowlist, `--list` and `feature-reexport` rule, plus the 42 (a0) import rewrites) is the change that commits this plan. PR 2 (`6faac15d`): feature edges 248 → 187 (−61, plan −62): the three `daily_notes` row/argument types (`WorkspaceNoteRecord`, `NoteTimestampRecord`, `VaultNoteUpsert`) stay with `DailyNotesRepository` for PR4's notes port, because `application` may not use `sqlx`; `select_opening_documents` moved with `SummarySearchPort`. Feature re-exports 18 → 21: the `SearchResultPortDto` alias is gone, and four temporary `pub(crate) use` shims keep `test_runtime.rs` unchanged. PR 3: this change. Feature edges 187 → 147 (−40, plan −40): `function_calling::{domain, dto}` moved to `contracts::tools`, `tool_specs` and the invalid-call marker to `ports::llm_port`, `ExplorerFocusDto`/`ExplorerLineRangeDto` to `contracts::explorer` and `contains_line_reference` to `shared::text`; conversation→function_calling 34 → 2 (the executor/registry traits in `chat_runtime.rs`, PR6b), conversation→llm 2 → 0, explorer→function_calling 4 → 0, conversation→explorer 9 → 7. The two `ToolDefinition`s stay two types with a `From` between them, because their JSON differs: the frontend lists the registry's with `input_schema`, and research jobs persist the model's with `parameters`. Feature re-exports 21 → 17: the `chat/ports.rs` and `chat/dto.rs` re-exports are gone, and the two shims that keep `test_runtime.rs` unchanged are `#[cfg(test)]`, which the checker does not count. PR 4a (design in `2026-10-16-library-ports-design.md`): feature edges 147 → 93 (−54, design −54). The pairs that changed are learning→web 13 → 0, web→indexing 7 → 0, web→embedding 4 → 0, function_calling→search/embedding/tags/web 7 → 0, search→embedding 12 → 5, indexing→embedding 6 → 2, file→tags and file→web 2 → 0 each, stats→tags 3 → 1, stats→health 1 → 0, vault→settings 2 → 0, backup→vault 2 → 0, and model_management→embedding 5 → 4. Nine items moved into core: `EmbeddingIndexBinding`, `SearchServiceTrait`/`BM25SearchTrait`/`SparseSearchTrait`, `WebServiceTrait`, `TagServiceTrait`/`DocumentLockGuard`, `IngestedDocumentStorePort`, `SettingsReaderPort`, `SpanEmbeddingGroup`, `services::live_index` and `resolve_vault_root`. The contracts `search::engine`, `tags`, `system_stats` and `REFERENCE_TEXT_EXTRACTION_VERSION` moved with them, as did `Tag`. `generation.rs` and `web_article.rs` went to infrastructure, and so did `ml/tokenizer.rs`. `EmbeddingServiceTrait`, `DynamicEmbeddingService` and `RemoteEmbeddingService` were deleted. Feature re-exports 17 → 15: `embedding/entity.rs` is gone, and the `LibrarySearchTrait` shim is now `#[cfg(test)]`. The vault workers take `Arc<impl SettingsReaderPort>` because the planned `Arc<dyn>` coercion does not compile at `container.rs`. PRs 4b, 4c, 5 and 6 follow.

**How the counts were made:** the totals come from `baseline.txt`: 297 references across 54 pairs (re-summed with awk). The checker reads its baseline from a path fixed at build time (`main.rs:583`, `env!("CARGO_MANIFEST_DIR")`), and read-only mode rules out a temp copy. So a grep/perl script copied how `ratchet.rs` collects references (test-only code excluded, as at `ratchet.rs:218`). It matched the baseline to within ±2 per pair (303 against 297; the extra hits sit in test-only files the script couldn't detect). The per-pair counts were sorted into kinds by hand, so expect ±5 movement between kinds.

**Earlier estimates vs the baseline:** the baseline gives **conversation 116 outgoing (not 119) and embedding 31 incoming (not 43)**. Search 42, function_calling 38, download 36, settings 24 and qa 23 match.

## 1. Recommendation

**Target: a small allowlisted DAG (3 edges, 7 references today). Every other edge goes to zero.**

- Most of the 297 are mechanical. 159 are vocabulary types and 57 are wiring code, together 216 (73%). Fixing them means moving types into core or moving wiring into `interfaces/di`, with no design debate. 42 of the vocabulary references point at types that **already live in core** and are only re-exported by a feature.
- Most of the 74 direct service/repository calls already have a port somewhere. Core has ports for downloaded models, settings, embedding, vector search and note indexing. Conversation already defines its own ports in `chat/ports.rs`; their signatures just name other features' types.
- A few edges are genuinely one feature built on another: batch drives indexing and web use cases, and compare is a thin layer over search. A port there adds indirection without decoupling anything. An allowlist with a reason per line, checked to stay acyclic, records that intent.
- A strict zero would force single-call ports. An open "allowed graph" would make today's 54 pairs legitimate. The small DAG sits between them.

## 2. PR plan

| PR | Scope | Refs (running total) | Size | Main risk | Unblocks | Collides with the model-selection session? |
|---|---|---|---|---|---|---|
| **1** | Checker: allowlist file, `--list`, re-export ratchet (§5). Allowlist batch→indexing, batch→web, compare→search. Also rewrite the **42 references that reach core types through feature re-exports** to import from core directly. | −49 (248) | 3 checker files + ~23 source files, imports only | Allowlist growth; re-export modules kept alive by test callers | `--list` turns every later PR's site list into one command | **No.** Don't touch `test_runtime.rs`; keep both re-exports until it migrates |
| **2** | Retrieval vocabulary into core: `qa::dto::{SourceDto, WebSnapshotDto}` → `contracts::sources`; search DTOs plus `RerankOptions`/`RankedBranch`/`BranchKind`/`QueryBranches`/`Reranked` → `contracts::search`; `LibrarySearchTrait` and `SummarySearchPort` (with `SummaryHit`) → `application/ports`; `select_informative_terms` → `shared::text`; note and reference records | −62 (186) | ~4 moved files (qa/dto.rs is 333 lines, search/dto.rs 285) + ~35 callers | Generated TS binding names drift (diff `bin/export_bindings` output); `impl SearchRequestDto` moves with it | PR4 (ports can name core types). qa fan-in 23→0 | **Low.** Edits `chat/ports.rs`; a temporary `pub use` keeps `test_runtime.rs` unchanged |
| **3** | Tool vocabulary into core: `function_calling::domain` (`ToolDefinition`, `FunctionCall`, `FunctionResult`) and the tool input/output DTOs → `contracts::tools`. Merge the two `ToolDefinition` structs (`function_calling/domain.rs:43` and core `llm_port.rs:98`). Move `llama_cpp::{tool_specs, invalid_tool_call_problem}` next to the core one. Also `ExplorerFocusDto` and `contains_line_reference` | −40 (146) | 2 moved files (~880 lines) + ~25 callers | Merging `ToolDefinition` may change its serialized shape | PR6b explorer port; function_calling fan-in 38→2 | **Low.** Temporary `pub use` for `test_runtime.rs:7` |
| **4** | Library ports into core: `WebServiceTrait`, `TagServiceTrait`, `IndexStorageTrait` (each depends only on core already; verified), the Search/BM25 traits. Swap `EmbeddingServiceTrait` for core `EmbeddingPort` (they look like duplicates; inferred). vault/backup read settings through `settings_port`. A notes port for `DailyNotesRepository`. Explorer's vector index goes behind a port. Remaining library vocabulary: constants, `Embedding`, `Tag`, `SystemStatsDto`, `SpanEmbeddingGroup`, `PublishedChunk` | −52 (94) | ~40 files | search↔embedding index-generation lifecycle (5 refs) needs real design; where `USearchVectorIndex` lands overlaps the infrastructure→feature branch | PR5 (each feature's `di::build` can take `Arc<dyn Port>`) | **No.** Coordinate with the infrastructure branch |
| **5** | Wiring that doesn't collide moves into `interfaces/di/modules`: `search/di`, `web/di`, `indexing/di`, `file/{di, plugin/commands}`, `stats/di`, `summaries/di`, `health/di`, `corpus_shape/commands`, `learning/plugin/memory` | −33 (61) | ~15 files | Startup order and lazy loading in DI; file commands that forward to indexing and recent | Nothing new | **Low.** Stay out of `modules/ai.rs` |
| **6** | **Waits for the model-selection session to land.** **6a (46):** the downloaded-model store. Core has a 4-method `DownloadedModelRepository` port; the concrete repository has ~18 methods, including active-model selection. Either widen the port or add an active-models port, plus a download-manager port. Covers `llm/di.rs`, `llm/loading.rs`, model_management use cases, plugin and commands, transcription, initialization, embedding, the model vocabulary, and settings→llm. **6b (15):** `conversation/di/chat_runtime.rs` → `interfaces/di`; `ExplorerTurn` becomes a port; HyDE comes through `ChatRetrieval`. Then delete the temporary re-exports and migrate `test_runtime.rs` | −61 (0, plus 7 allowlisted) | 6a ~30 files, 6b ~10 | Behaviour change in active-model selection; test doubles churn when port signatures change | The rule becomes a hard zero | **High.** Touches every file on the other session's list |

The order matters. PR1's `--list` saves work in every later PR. PRs 2–3 put the vocabulary in core, which the ports in PRs 4 and 6 need for their signatures. PR4's ports are what PR5's `di::build` functions take as parameters. **15 references cannot move until the other session lands:** 7 in `di/chat_runtime.rs`, 7 in `llm/di.rs` and 1 in `llm/loading.rs`. 6a also overlaps their active-model work (inferred).

## 3. Classification of the 297

| Kind | Refs | Representative sites (relative to `src-tauri/src/features/`) |
|---|---|---|
| **(a0) Vocabulary already in core**, reached through a feature re-export. `settings/dto/mod.rs:3` and `model_management/domain.rs:5` are both `pub use lattice_core::…::*` | **42** | `conversation/chat/retrieval/kb_retrieval.rs:13`, `llm/use_cases/download_model/single_file.rs:22`, `backup/use_cases/start_auto.rs:1` |
| **(a) Vocabulary still defined in a feature** | **117** | `conversation/chat/attachments.rs:31` (qa `SourceDto`, 23 refs overall), `conversation/chat/history_tools.rs:42` (`FunctionCall`/`FunctionResult`), `conversation/chat/prior_evidence.rs:10` (`select_informative_terms`, a pure function) |
| **(b) Business code calling another feature's service, repository or trait** | **74** | `model_management/use_cases/get_active_chat_model.rs:5` (concrete `DownloadedModelRepository`), `learning/planning/outline_draft.rs:7` (`WebServiceTrait`, defined in web but depends only on core), `vault/watcher.rs:16` (`GetSettingsUseCase` although core has `settings_port.rs`), `conversation/chat/retrieval/pipeline/external_lookup.rs:35` (`HyDEService::new`) |
| **(c) Wiring in a feature's di/plugin/commands files** | **57** | `conversation/di/chat_runtime.rs:131` (`impl PageReader for web::WebService`), `search/di.rs:10-12`, `llm/di.rs:10-13`, `file/plugin/commands.rs:6` (re-exposes indexing commands), `health/di.rs:8` (production code uses llm's `MockLLMPort`) |
| **(d) Test-only** | **0 counted** | The checker already skips `cfg(test)`, `test-support` and `*tests.rs`. Test-only coupling exists but isn't measured: `conversation/chat.rs:293` (`test_runtime`), `learning/lessons/generation_jobs.rs:526` |
| **(e) Fine: one feature built on a lower-level one** | **7** | `batch/file_job.rs:8`, `batch/url_job.rs:6`, `compare/scoped_search.rs:16` |

**(a) by group (117):**
- Tools: 38
- Search: 29
- qa sources: 23
- Model/embedding (`EmbeddingCompatibility`, `artifact_identity`, `SPARSE_HEAD_PT`): 7
- Note and reference records: 6
- `REFERENCE_TEXT_EXTRACTION_VERSION` in learning: 4
- indexing→embedding (`MODEL_NAME`, `Embedding`): 4
- explorer: 2
- web→indexing: 2
- stats: 2

**(b) by provider (74):**
- Downloaded-model store and download manager: 21 (model_management 14, llm 4, plus embedding, transcription and initialization)
- Web port: 12
- Search traits and use cases: 9
- Embedding (trait and index-generation lifecycle): 8
- Notes, vault and settings: 8
- Explorer as a chat tool provider: 7
- Indexing: 4
- Summaries: 2
- Tags: 2
- settings→llm: 1

**(c) by file (57):**
- `file/*` (di + commands): 10
- `search/di.rs`: 8
- model_management di/plugin/commands: 8
- `conversation/di/chat_runtime.rs`: 7
- `llm/di.rs`: 7
- `web/di.rs`: 4
- `indexing/di.rs`: 4
- `stats/di.rs`: 2
- `learning/plugin/memory.rs`: 2
- five files with 1 each

**Not allowlisted, and why:**
- model_management→download: six features use the same store, so it should be a port.
- web→indexing: core's `note_index_sink` port is the pattern to follow.
- function_calling→*: its executor is an adapter layer, and core ports are enough.
- explorer→search: it uses a vector-index engine that doesn't belong in the search feature.

## 4. Conversation's outgoing edges (116)

Conversation already defines its own ports (`chat/ports.rs`). The leak is that their signatures and the code around them name other features' types. After PRs 1–3, conversation's remaining edges are almost all wiring.

| Target | Refs | Kinds | What it becomes | PR |
|---|---|---|---|---|
| function_calling | 34 | a 32, c 2 | Tool vocabulary moves to `contracts::tools`, with one `ToolDefinition`. The executor/registry traits are used only in `chat_runtime.rs`, which moves to the composition root | 3, 6b |
| search | 30 | a 26, b 3, c 1 | DTOs and rerank/branch types to `contracts::search`; `select_informative_terms` to `shared::text`; `LibrarySearchTrait` becomes a core port (`ports.rs:327`, `corpus_plan.rs:11`); HyDE is supplied by `ChatRetrieval` | 2, 6b |
| qa | 17 | a 17 | `SourceDto`/`WebSnapshotDto` to `contracts::sources`. qa keeps only commands | 2 |
| settings | 16 | a0 16 | Import `lattice_core::application::contracts::settings` directly | 1 |
| explorer | 9 | a 2, b 7 | A `FolderExplorer` port in `chat/ports.rs` replaces `ExplorerTurn` and the tool functions (`is_explorer_tool`, `activity_label`, `add_to_turn`). Explorer implements it and it's wired at the root. `ExplorerTurn` has behaviour (`explorer/prompt.rs:58-165`), so it can't move as a plain type | 3, 6b |
| summaries | 4 | b 2, c 2 | `SummarySearchPort` moves to core; `select_opening_documents` goes behind it or into core (it's a pure ranking function) | 2, 6b |
| web | 2 | c 2 | The `PageReader` adapter (`chat_runtime.rs:131`) moves to `interfaces/di` | 6b |
| llm | 2 | a 2 | `tool_specs` and `invalid_tool_call_problem` sit next to core `ToolDefinition` | 3 |
| references, daily_notes | 1 + 1 | a 2 | The records named in `ports.rs:295,307` move to core contracts | 2 |

## 5. Checker changes

1. **Allowlist file** `scripts/rust-architecture-check/feature-graph.txt`, one line per edge: `allow features::batch features::indexing  # batch runs indexing use cases`.
   - The reason comment is required.
   - Allowed references are left out of `feature-edge` but still feed `cycle_counts`.
   - The checker fails if the allowlist itself contains a cycle, and reports STALE when an allowed edge has no references left.
2. **`--list <rule> [subject]`** prints every finding with its location. Today locations print only for violations (`ratchet.rs:1203-1205`), and the baseline path is fixed at build time, so the only way to see sites is to edit the baseline.
3. **`feature-reexport` ratchet** counts `pub use lattice_core::…` and `pub use crate::features::<other>::…` inside features. Today there are two (`settings/dto/mod.rs:3`, `model_management/domain.rs:5`). They are how 42 core-type references became feature edges, and PRs 2–3 add temporary ones for test callers. The ratchet makes sure they get removed.
   - Don't teach `feature-edge` to see through re-exports instead; that hides the indirection.
4. **One rule plus the allowlist, not separate rules per kind.** Kinds are guessed from module paths and would be gamed by renaming modules. A cheap alternative is a hint on each violation, also from the path:
   - target `dto`/`domain`/`entity`: "move to `lattice_core::application::contracts`"
   - source `di`/`plugin`/`commands`: "wire in `interfaces/di`"
5. **At zero:** delete the `feature-edge` baseline lines and make any new edge outside the allowlist a hard failure, as `feature-cycle` is now.
6. **Test code:** keep it excluded. Optionally print an informational `feature-edge-test` count so coupling in test doubles (`test_runtime.rs`, `mocks`) is visible.

## 6. Risks and items not verified

- **search↔embedding index generation** (`search/engine/vector_search/runtime_index.rs:4,105,108`, `persistence.rs:165`, `qwen3_reranker.rs:82`) is the only group of direct calls without an obvious port. My guess is that index generation belongs in search or core (inferred), but it needs a short design before PR4.
- **`USearchVectorIndex`** (`explorer/index/{build,run,search}.rs`, `summaries/di.rs:28`): choose between a core `VectorSearchPort` adapter and infrastructure together with the infrastructure→feature branch.
- **Generated TS bindings:** core already depends on specta and its settings contracts derive it (core `Cargo.toml:36`), so the DTO moves should work. Check that the `export_bindings` output is byte-identical (not verified).
- **`EmbeddingServiceTrait` vs core `EmbeddingPort`:** the method lists overlap (`embed_single`, `embed_query`, `embed_batch`, `split_text`). Treating them as equivalent is inferred; check the adapter before deleting the trait.
- **`health/di.rs:20`** builds a production `HealthCheckUseCase` with llm's `MockLLMPort`. Move a degraded LLM port into core next to `MockEmbeddingPort::new_degraded` (PR5).
- **Model-role UI in settings:** whether it touches `settings/plugin.rs` or `repository/validation.rs` (the settings→llm edge) is unknown, so I put those 2 references in PR6.

### Critical files for implementation
- scripts/rust-architecture-check/src/ratchet.rs
- src-tauri/src/features/conversation/chat/ports.rs
- src-tauri/src/features/conversation/di/chat_runtime.rs
- src-tauri/crates/lattice-core/src/application/contracts/mod.rs
- src-tauri/src/features/download/downloaded_model_repository.rs