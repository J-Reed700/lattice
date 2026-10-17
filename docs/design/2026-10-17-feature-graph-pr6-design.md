# PR 6 design: the last feature edges (2026-10-17)

The base is `arch/feature-graph-4b-5` (01d366d5): main with PRs 4b, 4c and 5. `--list feature-edge` reports 68 refs: 61 baselined and 7 allowlisted (batch→indexing 4, batch→web, compare→search). This design brings the 61 to 0 in four PRs: 6c, 6b, 6a, 6d.

The plan's §2 row 6 had two PRs. 6a alone came to about 65 files, so it splits along its natural seam into the store (6a) and the download manager with its wiring (6d). Vocabulary and IPC leftovers go first, as 6c.

## Decisions

### Q1. Port shape for 6a: two ports plus the download manager

**Choice.** Widen core's existing records trait, add one `ActiveModels` port, and move `DownloadManager` into core unchanged. Don't build one 18-method trait. Four reasons:
- `activation.rs` is already a separate `impl` block (`download/downloaded_model_repository/activation.rs:12-330`), so the split follows the code.
- Readers need only a few methods: the embedding loader needs one, and `ModelLoader` needs one records method and two active methods.
- Active selection is what #81 depends on (`roles.rs:126,149` take the Settings default from `active_local_chat_model()`). Its own port keeps that review small.
- The only test double, `Rows` (`conversation/model_availability/tests.rs:10`), implements the records trait. It gains 5 stubs, where a fat trait would need 14.

**Records: `domain::repositories::DownloadedModelRepository`.** This is the existing 4-method trait; it stays where it is. Production callers, with test modules excluded:

| Method | New? | Callers (edge) |
|---|---|---|
| `find_by_model_id` | has | mm `set_active_chat_model.rs:19`, `set_active_embedding_model.rs:37`, `set_active_utility_model.rs:19`, `commands_extra/active_models.rs:201`, `commands_extra/external_models.rs:228` · llm `loading.rs:183` · transcription `engine/whisper.rs:162` |
| `list` (callers use `list_all` today) | has | mm `download_default_model.rs:119`, `get_models_with_metadata.rs:25`, `external_models.rs:275` |
| `find_by_id` | new | mm `commands_extra.rs:251` |
| `save` | new | mm `track_download.rs:71`, `external_models.rs:269` |
| `is_downloaded` | new | mm `check_is_downloaded.rs:28`, `set_active_chat_model.rs:25`, `set_active_embedding_model.rs:43`, `set_active_utility_model.rs:24` · transcription `whisper.rs:159` |
| `has_any_embedding_model` | new | initialization `first_run_setup.rs:57`. It sits in `activation.rs:311` but reads `status = 'completed'`, not an active flag |
| `delete_if_not_active` | new | mm `delete_downloaded_model.rs:125`, `external_models.rs:298` |
| `delete`, `delete_by_model_id_if_not_active` | has | llm `DeleteModelUseCase`, wired at `llm/di.rs:128` |

**Active: new `application/ports/active_models.rs`, `ActiveModels`.** It has the 9 `activation.rs` methods, with signatures unchanged.

| Method | Callers |
|---|---|
| `get_active_chat_model` | mm `get_active_chat_model.rs:25` · llm `loading.rs:167` (**#81 default resolution**) |
| `get_active_utility_model` | llm `loading.rs:177` (**#81**, `roles.rs:330`) |
| `get_active_embedding_model` | mm `get_active_embedding_model.rs:25` · embedding `loading.rs:31` |
| `set_active_{chat,utility}_model` | mm `set_active_chat_model.rs:34`, `set_active_utility_model.rs:33` |
| `set_active_embedding_model(id, Option<&ArtifactIdentity>)` | mm `set_active_embedding_model.rs:62` |
| `clear_active_{chat,embedding,utility}_model` | mm `clear_active_chat_model.rs:23`, `clear_active_embedding_model.rs:23`, `set_active_utility_model.rs:40` |

**Download manager: `application/ports/download_manager.rs`.** Move the trait, `DownloadRequest`, `DownloadBatchItem` and `DownloadEvent` verbatim from `download/manager/types.rs:14-101`, which needs only core and tokio. Callers: llm `single_file.rs:218,279`, `multi_file.rs:253,312`, `download_model.rs:138` (`start_download`, `start_download_batch`, `get_download_status`, `delete_pending_by_model`); mm `download_default_model.rs:156` (`start_download`) and `plugin/mod.rs:36` (`DownloadEvent`).

`DownloadManagerService`, the engine and `SqliteDownloadRepository` stay in download, which implements the core port.

**Adapter and consumers.**
- **Repository move.** `git mv` the concrete repository to `infrastructure/persistence/repositories/downloaded_models{.rs,/activation.rs,/rows.rs,/ports.rs,/tests.rs}`. It imports only core and its own submodules. It keeps the name `DownloadedModelRepository`, as PR 4c kept `DailyNotesRepository`, with the existing `as …Trait` alias.
- **`artifact_identity.rs`** moves to `infrastructure/ml/`, next to `tokenizer.rs` (PR 4a precedent).
- **One port:** `EmbeddingLoader` (active); whisper and `first_run_setup` (records); the get/clear use cases.
- **Both ports, as two fields with no supertrait:** `ModelLoader` and `SetActive{Chat,Embedding,Utility}ModelUseCase`.
- **Ownership.** Use cases go from holding the repository by value (`(*container.downloaded_model_repository()).clone()`) to `Arc<dyn …>`. The repository holds only a pool (`downloaded_model_repository.rs:20-23`), so no state changes.

### Q2. 6b: conversation

**Into `interfaces/di`.** `git mv` `conversation/di/chat_runtime.rs` (314 lines) to `interfaces/di/chat_runtime.rs`. Move the `mod` line from `conversation/di/mod.rs:135` to `interfaces/di/mod.rs`.

It holds only trait impls for `Container`, `RegisteredTools` (`:131`) and `impl PageReader for WebService` (`:150`). Everything it names is visible from there: `Container.ai`/`core` are `pub` (`container.rs:53,65`), `turn_cancellation`/`research_waiters` are `pub(crate)` (`:130,135`), `compaction::{compact_for_turn, consolidate_after_turn}` are `pub` (`compaction.rs:148,206`), and `handoff` is `pub(crate)`.

**It moves without a behaviour change.** Trait impls are crate-global whichever module holds them, so dispatch is identical. This closes −2 function_calling and −2 web.

**Explorer.** One core port, `application/ports/explorer_turn.rs`. No `FolderExplorer` service port is needed, because resolution already sits behind `ChatStorage::explorer_turn` (`chat/ports.rs:189`).

```rust
#[async_trait]
pub trait ExplorerTurnPort: Send + Sync + std::fmt::Debug {
    fn has_folder_index(&self) -> bool;                                   // prompt.rs:105
    fn add_tools(&self, tools: &mut Vec<ToolDefinition>);                 // tools.rs:149 add_to_turn(.., has_index)
    async fn context_block(&self, max_chars: usize, tools_offered: bool,
                           question: Option<&str>) -> String;             // prompt.rs:135
    async fn execute_tool(&self, call: FunctionCall, max_chars: usize)
        -> Result<FunctionResult>;                                        // prompt.rs:165
}
```

- **Implementation.** Explorer's `ExplorerTurn` implements the port by delegating; `resolve` and `with_index` stay inherent.
- **Alias.** `chat.rs:116` changes from a re-export to `pub(crate) type ExplorerTurn = Arc<dyn ExplorerTurnPort>;`. With it, `chat/turn/prepare.rs:25` (owned), `assemble.rs:535,640`, `calls.rs:30,99,499`, `carry.rs:56-66` and `classify.rs:20` compile unchanged. Only `tool_loop.rs:160` (full path) and `assemble.rs:685` (`explorer.add_tools(&mut turn_tools)`) change.
- **Wrapping.** In `chat_runtime.rs`, `explorer_turn` maps the result to `Arc::new(t) as ExplorerTurn`. Import explorer's struct under another name there to avoid the clash.
- **Tool names are vocabulary, as in PR 3.** The `TOOL_*` constants (`tools.rs:19-22`, `index/tool.rs:15`), `is_explorer_tool` (`tools.rs:39`) and `activity_label` (`tools.rs:195`, `index/tool.rs:64`) go to `contracts::tools::explorer`. So do the arg helpers `arg_str`, `arg_u32` and `folder_path`, which are duplicated today in `tools.rs` and `index/tool.rs:158-187`. They are called from free functions with no runtime in scope (`call_labels.rs:149,176`, `tool_format.rs:50`, `calls.rs:41,84`). This closes −7.

**HyDE.** Add `application/ports/query_interpreter.rs`, `QueryInterpreter`. Its four methods (`interpret_with_context`, `classify_with_context`, `web_search_query_with_context`, `research_followups_with_context`) mirror `hyde_service.rs:168-293`, with `&str` in place of `impl Into<String>`.

`HyDEService` implements it. Add `ChatRetrieval::query_interpreter(&self, llm: Arc<dyn LLMPort>, cancel: CancellationToken) -> Arc<dyn QueryInterpreter>`, which `external_lookup.rs:34-36` calls with the utility LLM and `turn_token(request_id)`. This closes −1.

**`test_runtime.rs` after the shims.** It imports core directly and keeps its own `FakeRuntime` doubles (`:320`, `:407`).

| Line | Today (shim) | After |
|---|---|---|
| 7 | `function_calling::domain::*` (`function_calling/mod.rs:34-40`, cfg(test)) | `contracts::tools` |
| 34, 411 | `search::trait_def::LibrarySearchTrait` (`trait_def.rs:12-14`, cfg(test)) | `ports::library_search` |
| 388 | `explorer::dto::ExplorerFocusDto` (`explorer/dto.rs:9-11`, cfg(test)) | `contracts::explorer` |
| 389 | `explorer::prompt::ExplorerTurn` | the `ExplorerTurn` alias; still returns `None` |
| 419 | `summaries::search::SummarySearchPort` (`summaries/search.rs:16`) | `ports::summary_search` |
| 431-432 | `settings::dto::SettingsDto` | `SettingsDto` via `ports::*` (`chat/ports.rs:10`) |
| 490, 503 | `references::repository::PassageReferenceRecord` (`:7`), `daily_notes::dto::WorkspaceNoteDto` (`:3`) | `contracts::references`, `contracts::notes` |
| new | — | `query_interpreter` returns the real `HyDEService::new(llm).with_cancellation(cancel)`, so tests that script HyDE through the fake LLM behave as before. Test code is outside `feature-edge` (plan §5.6). |

Then delete the three cfg(test) shims. `summaries/search.rs:16`, `references/repository.rs:7` and `daily_notes/dto.rs:3` become private `use`s, because test_runtime was their only outside caller (grepped). Inside daily_notes, `capture_store.rs:5-8` and the `commands.rs:12` glob then import `WorkspaceNoteDto` from core.

### Q3. file→indexing without changing the bindings

**Yes, the paths can change without changing the bindings.** `collect_commands!` names a command by the `#[specta::specta]` fn **identifier**, not its module path; the TS name is `camelCase(ident)`. The `plugin:<p>|` prefix comes from the `<p>:allow-<cmd>` grant in `capabilities/main.json` (`export_bindings/routes.rs:1-10,22-50`), not from Rust. The output follows list order: `indexFile` at `bindings.ts:2207` follows `main.rs:278-297`.

**Move.** The 13 indexing-forwarding commands go from `features/file/plugin/commands.rs` to a new `interfaces/commands/indexing.rs`. That follows `interfaces::commands::document_list::get_document`, which the file plugin already registers (`file/plugin/mod.rs:3,57`). The 13: index_file, index_directory, remove_indexed_file, reindex_file, delete_document, rename_document, get_indexing_status, get_indexing_stats, get_index_progress, cancel_indexing, pause_indexing, resume_indexing and clear_indexing_failure.

**What changes.** `file/plugin/mod.rs:32-50` names them at the new path, and `export_bindings/main.rs:278-297` changes those 13 paths only, in the same order. Bodies and `///` docs (which become JSDoc) move verbatim.

**What doesn't.** `capabilities/main.json`, `build.rs` and `routes.generated.ts` are untouched, and `check-tauri-command-inventory.py:105` keys on the last path segment. This closes −4.

**Rejected:** an `indexing` plugin. It would rename 13 grants and invoke strings.

### Q4. Health: real LLM readiness

**Where it lives.** Readiness lives in `LlmInstances` (`llm/instances.rs:253`, held in `Container.llm_instances` at `container.rs:86,199`). `Container::loaded_models()` reads it (`llm/di.rs:259`, `pub(crate)`), and each instance carries `LoadedInstance.degraded` (`instances.rs:185`).

**Why not at build time.** `health::di::build` runs in `SystemModule` (`modules/system.rs:98`), so wiring the instances at build time would need `container.rs` (owned).

**Smallest honest change.**
1. `health/commands.rs:41` computes `container.loaded_models().iter().any(|m| m.degraded.is_none())`. It names no llm type, so this adds no edge.
2. Pass it into `HealthCheckUseCase::execute(llm_ready)`, and drop `llm_service` (`health_check.rs:49,161`).
3. Delete `DegradedLLMPort` (`ports/mock_llm_port.rs:15`, `ports/mod.rs:164`); `health/di.rs:22` is its only user.

The flag now means "a model is loaded and not degraded"; models load lazily, so it reads `false` until first use. Document that on the use case, not on `HealthStatus`, where a doc change would change `bindings.ts`. **Not fixed:** `MockEmbeddingPort::new_degraded()` (`health/di.rs:21`) keeps `embedding_model` false, so `status` is always "degraded". No non-loading embedding probe exists.

## Split, order and counts

| PR | Scope | Edges closed | Feature edges after | Files (approx.) |
|---|---|---|---|---|
| **6c** | Model vocabulary, IPC adapters, health | llm→embedding 2, mm→embedding 4, settings→llm 2, file→indexing 4 | **49** | ~20 |
| **6b** | Conversation | explorer 7, function_calling 2, search 1, web 2 | **37** | ~22 |
| **6a** | Downloaded-model store | mm→download 15, llm→download 2, embedding→download 2, transcription→download 2, initialization→download 1 | **15** | ~40 |
| **6d** | Download manager and llm DI wiring | mm→download 6, llm→download 7, llm→model_management 2. Last commit makes the rule hard (§5.5) | **0** | ~25 |

`feature-reexport` goes 15 → 14 (6c: `embedding/compatibility.rs:17`) → about 10 (6b: `chat.rs:116` plus the three above). `infrastructure-imports-feature` stays 0.

**6c, in order.**
1. **Embedding compatibility.** `embedding/compatibility.rs` moves whole, with its tests, to core `domain/models/selection.rs` as `EmbeddingCompatibility::from_tags`. Callers: `huggingface_adapter.rs:557-563` and its tests, `download_model.rs:171` and `interfaces/dto/model_catalog_dto.rs:191-195`.
2. **`SPARSE_HEAD_PT`.** It moves from `sparse_head.rs:46` to core `domain/models/checkpoint.rs`. Caller: `metadata.rs:70`.
3. **Activation load.** Embedding's registrar gets `open_embedding_for_activation(dir, identity) -> Result<Box<dyn EmbeddingPort>>`. It keeps the `spawn_blocking` and its error text. `active_models.rs:217-229` calls it.
4. **Connection check.** `llama_cpp::validate_connection` (`llama_cpp/mod.rs:421`) becomes core `LlamaCppSettingsDto::validated_base_url`; core already has `url`. Callers: `validation.rs:219`, `llama_cpp/mod.rs:76,142` and `tests.rs:458,464`.
5. **Test-connection command.** `test_llama_cpp_connection` (`settings/plugin.rs:212`) moves to `interfaces/commands/model_setup.rs`. `settings/plugin.rs:639` and `export_bindings/main.rs:445` get its new path.
6. File commands (Q3).
7. Health (Q4).

**6b, in order:** (1) `git mv` `chat_runtime.rs`, in its own commit; (2) explorer vocabulary into core, then `ExplorerTurnPort`, the alias and the call sites; (3) `QueryInterpreter` in both `ChatRetrieval` implementors; (4) migrate `test_runtime.rs`, then delete the shims.

**6a, in order.**
1. `git mv` the repository and `artifact_identity`, changing paths only, in its own commit. That alone closes all 22.
2. Widen the records trait, add `ActiveModels`, and give `Rows` its 5 new methods.
3. Port the consumers: 11 mm use cases and `commands_extra{,/active_models,/external_models,/warm_up}.rs`; `ModelLoader`, `EmbeddingLoader`, whisper and `first_run_setup`; wiring in `interfaces/di/modules/{ai,search,indexing}.rs`, `interfaces/commands/model_setup.rs` and `bootstrap/{app,startup_reconciliation}.rs`.
4. Split `model_management/di.rs:27`'s accessor into `downloaded_models()` and `active_models()`.

**6d, in order.**
1. Move the `DownloadManager` port into core.
2. `modules/ai.rs` builds the catalog adapters, `SqliteDownloadRepository`, `HttpDownloadEngine` and `DownloadManagerService`, in the same order as `llm/di.rs:41-49` does today. `llm::di::build` takes `LlmPorts { downloaded_models, active_models, download_manager, model_catalog }`, and `LlmDi` loses three fields, which `AIModule` now holds (`ai.rs:173-195`).
3. Update the remaining imports: `download_model.rs:27`, `download_default_model.rs:31`, `mm/di.rs:39`, `plugin/mod.rs:36-38` and `download/manager/*`.

### Files owned by the other session, per sub-PR

| Owned file | 6c | 6b | 6a | 6d |
|---|---|---|---|---|
| `llm/**` | `use_cases/download_model.rs:171`, `download_model/metadata.rs:70`, `llama_cpp/mod.rs:76,142,421`, `llama_cpp/tests.rs:458,464` | — | `di.rs:12`, `loading.rs:8,12,151-183` (field types only), `loading/tests.rs`, `di/roles/tests.rs`, `use_cases/delete_model.rs:266` (test) | `di.rs` (build signature, `LlmDi`), `use_cases/download_model{.rs,/single_file.rs,/multi_file.rs}` |
| `conversation/di/chat_runtime.rs` | — | moved and edited | — | — |
| `chat/turn/prepare.rs` | — | **untouched** (alias) | — | — |
| `chat/test_runtime.rs` | — | imports, `query_interpreter` | — | — |
| `bin/export_bindings/*` | `main.rs`: 13 paths + `:445` | — | — | — |
| `sidecar_manager/*`, `container.rs` + `container/*`, `settings_side_effects.rs`, frontend `modelRoles/*`, `chatModelSelection*`, `ModelPickerPopover.tsx`, `ModelCatalog/*` | — | — | — | — |

Before building, check that no other checkout holds uncommitted edits to `conversation/chat.rs`, `chat/ports.rs`, `tool_loop.rs`, `tool_loop/calls.rs`, `test_runtime.rs`, core `ports/mod.rs`, `contracts/settings/llm.rs` or `domain/models/mod.rs`; every sub-PR touches some of them.

## Risks

**Behaviour.**
- **#81 default chat model.** `roles.rs:126,149` take the default from `get_active_chat_model`. That method, like `get_active_embedding_model`, drops an active row whose files are incomplete (`activation.rs:39-50`). Keep those SQL bodies, don't swap in `repositories/model/tx.rs:77-97` (the second reader over `models`), and keep the SQLite-backed tests; doubles must not stand in for that rule.
- **Utility is not guarded on delete.** Neither delete guard checks `is_active_for_utility` (`downloaded_model_repository.rs:350-351`, `ports.rs`). Preserve this; a fix is its own PR.
- **One download manager.** `bootstrap/app.rs:276-278` needs the container's instance, and `modules/system.rs:91` clones it. 6d must build exactly one, in `AIModule`.
- **Cancellation.** The HyDE interpreter must be built per turn with the turn's token; caching it loses cancellation.
- **Health.** `llm` changes from always `false` to loaded-and-not-degraded. `status` is unchanged, because `determine_status` ignores the flag (`health_check.rs:177-181`).

**Generated bindings.** No `specta::Type` moves: the moved types derive only `Clone` or `FromRow`, and `EmbeddingCompatibility` is already in core. 6c changes command paths only. Run `export_bindings -- --check` before merging, and don't touch `HealthStatus` docs.

**Infrastructure→feature stays 0.** The moved repository imports `super::*` and core; `artifact_identity` imports core and `sha2`; `manager/types.rs` imports core and tokio. Two moves would break it: `HuggingFaceAdapter`/`ModelCacheAdapter` into infrastructure before 6c removes their `embedding::compatibility` import (`huggingface_adapter.rs:557`), which is why 6d keeps them in model_management; and `CandleEmbeddingService::open` or `ModelLoader` into infrastructure.

**Coercions** to check with `cargo check`: `Arc<DownloadedModelRepository>` into the `Arc<dyn ActiveModels>` field (`embedding/di.rs:32`), `Arc<ExplorerTurn>` to `Arc<dyn ExplorerTurnPort>`, and `Arc<HyDEService>` to `Arc<dyn QueryInterpreter>`.

**Later.** Three overlapping traits sit over `models`: `ModelStoragePort` (`ports/model_storage.rs:55`, also implemented at `llm/engine/model_storage_adapter.rs:87`), the records trait, and `ModelRepositoryPort` tx. Merging them is out of scope.

## 6b as built (2026-10-17)

Branch `arch/feature-graph-6b` on `arch/feature-graph-4b-5`, four commits in the planned order. `feature-edge` 61 → 49 (conversation→explorer 7, function_calling 2, search 1, web 2; conversation now has no feature edges). `feature-reexport` 15 → 11 (`chat.rs:116`, `summaries/search.rs:16`, `references/repository.rs:7`, `daily_notes/dto.rs:3`). Every other count is unchanged, `infrastructure-imports-feature` stays 0.

1. **Chat runtime.** `git mv` to `interfaces/di/chat_runtime.rs`; the `mod` line moves to `interfaces/di/mod.rs`. Every path in the file was already absolute, so the move changes no line of it.
2. **Explorer.** Core `contracts::tools::explorer` holds the five `TOOL_*` names, `is_explorer_tool`, `activity_label` (with the `search_folder` label as a private helper), `arg_str`, `arg_u32` and `folder_path`. Core `ports::explorer_turn::ExplorerTurnPort` is the port as designed; explorer's struct implements it by calling its inherent methods, and `add_tools` calls `tools::add_to_turn`. `chat.rs` declares `pub(crate) type ExplorerTurn = Arc<dyn ExplorerTurnPort>`; `chat/turn/prepare.rs` is untouched. Tool definitions, prompt text and the explorer tool loop stay in the explorer feature, unchanged.
3. **HyDE.** Core `ports::query_interpreter::QueryInterpreter`, implemented by `HyDEService` by delegation. `ChatRetrieval::query_interpreter(llm, cancel)` builds a fresh `HyDEService::new(llm).with_cancellation(cancel)` on every call, in `chat_runtime.rs` and in `test_runtime.rs`; `external_lookup.rs` calls it once per turn with the utility LLM and `turn_token(request_id)`, as before.
4. **Test runtime.** Imports core directly, per the table; the three cfg(test) shims are deleted and the three remaining re-exports become private `use`s.

**Deviations.**
- The port's `add_tools` takes `ports::llm_port::ToolDefinition` (what `add_to_turn` and the turn's tool list use), not `contracts::tools::ToolDefinition`; they are different types.
- `index/tool.rs`'s own `arg_str` was not a duplicate: it also reads "/", "." and "./" as absent. Core keeps that reading as `index_arg_str` (`arg_str` then that filter, the same result), used by the `search_folder` label and by `index/tool.rs`, which imports it under its old name.
- `arg_bool` stays private in explorer's `tools.rs`: no label reads it.
- Commit 2 already changes `test_runtime.rs:389` to the alias, because the trait's return type changes in that commit and the tests would not compile otherwise. Commit 3 adds `query_interpreter` there for the same reason.
- `WorkspaceNoteDto` had two more outside readers than the grep found: `daily_notes/plugin.rs` and `daily_notes/tests.rs` named it through `commands.rs`'s `pub use super::dto::*` glob. Both now import it from core.
- Explorer's own tests that called `tools::activity_label`/`is_explorer_tool` now call the core functions.

**Behaviour.** No intended change. The labels and `is_explorer_tool` were moved byte for byte, and the tool definitions and prompt text were not touched. The explorer turn now sits behind an `Arc`; the chat only borrows it, so nothing else differs. The query interpreter is built per turn with that turn's token and is never cached.

## 6c as built

Four commits on `arch/feature-graph-6acd`, steps 1–7 in order. `feature-edge` 61 → 49 (llm→embedding 2 → 0, model_management→embedding 4 → 0, settings→llm 2 → 0, file→indexing 4 → 0), `feature-reexport` 15 → 14, `infrastructure-imports-feature` 0. The generated TypeScript is unchanged (`export_bindings -- --check`), and `capabilities/main.json`, `build.rs` and `routes.generated.ts` are untouched.

**Deviations.**
- **Step 1.** `from_tags` and its tag tables and tests live in a private child module, `domain/models/selection/embedding_compatibility.rs`, as an inherent `impl EmbeddingCompatibility`. The type path is still `selection::EmbeddingCompatibility::from_tags`, with no re-export; it keeps `selection.rs` at its size. The file was `git mv`'d, so its history follows it.
- **Step 3.** `open_embedding_for_activation` is a `pub(crate)` method on the embedding registrar (`embedding/di.rs`) taking `(dir: PathBuf, identity: ArtifactIdentity)`. `active_models.rs` passes `model.as_ref()` to `generation::prepare`.
- **Step 5.** `TestOllamaConnectionResponse` stays in `settings/plugin.rs`, where the moved command imports it from; moving a `specta::Type` risks the bindings' type order. The settings plugin names the command by its full `crate::interfaces::commands::model_setup::` path in `generate_handler!`.
- **Step 6.** `IndexingStatus` (a `specta::Type`) stays in `file/plugin/commands.rs` for the same reason; `interfaces/commands/indexing.rs` imports it. The private `unwrap_api_result` helper moved with the 13 commands, its only users. `file/plugin/mod.rs` imports `interfaces::commands::indexing` beside `document_list`, so the plugin, not feature logic, is what names the interfaces layer.
- **Step 7.** `ports/mock_llm_port.rs` held only `DegradedLLMPort` and its test, so the whole module is gone. The use case's tests pass the flag instead of a mock `LLMPort`.

**Behaviour.** The health check's `llm` flag now reads "a model is loaded and not degraded" instead of always `false`; `status` is unchanged. Nothing else changes: error texts, the `spawn_blocking` in the activation load, command names, invoke strings and JSDoc are the same.
