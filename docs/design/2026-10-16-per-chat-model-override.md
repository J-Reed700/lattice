# Per-chat model override

**Date:** 2026-10-16
**Status:** design, decision-complete (Josh's four answers folded in, §14); no code changed
**Base:** PR #66, branch `fix/overnight-wave-2026-10-16` at commit **df6029b8**. Every `file:line` below was re-verified with `git -C /Users/josh/Code/lattice-wave show df6029b8:<path>`. The worktree HEAD is `dad410df` (df6029b8 + main merged, which brings PR #61 `ae85a7d9`); line numbers in the #61-touched files (`chat/ports.rs`, `chat_runtime.rs`, `compaction.rs`, `finalize.rs`, `turn/mod.rs`, `test_runtime.rs`) are df6029b8's and may shift by a few lines on dad410df. Migrations must be dated after `20261016100000`.

## 0. The rule, in one paragraph

A chat's model picker sets the model for **that chat only**. The app-wide default is set in **Settings** and nowhere else. Learning work runs on its own **Learning model** role, never on a chat's choice. A turn resolves its model once, at the start, and nothing that happens in the picker afterwards touches the turn that is running. Every assistant message records the model that actually produced it. When a chosen model cannot be used, the turn or job fails with the true cause and the fix, never with a substitute model.

Today's incident, mechanically: the Explorer picker called the global `set_active_chat_model` plus `updateSettings({provider})` (`src/features/chat/hooks/useChatSwitchActions.ts:97-111`), which invalidated the single-slot chat cache (`src-tauri/src/features/model_management/commands_extra/active_models.rs:26`). The lesson job loads "the chat model" through the same single slot (`src-tauri/src/features/learning/plugin/generation.rs:81-84` → `Container::get_or_load_llm`, `src-tauri/src/features/llm/di.rs:182-190`) and retries that load through `patiently` after any transient failure (`src-tauri/src/features/learning/plugin/program.rs:79-85`; the job kind is `RecoveryPolicy::Requeue`, `src-tauri/src/features/learning/lessons/generation_jobs.rs:343`). The retry got the 9B, launched with its saved per-model `context_length` of 8,704 (`src-tauri/src/features/llm/loading.rs:535-536` → `LocalServerOptions::with_model_config`, `src-tauri/src/features/llm/engine/factory.rs:114-128`), and the lesson's grounded request did not fit (`src-tauri/src/features/learning/model_call.rs:1-8`).

## 1. Verified current state and what is worse than the brief

| # | Finding | Where |
|---|---|---|
| 1 | Chat model cache is **one slot keyed by `()`**; any picker change evicts whatever every consumer was using. | `src-tauri/src/interfaces/di/container.rs:41-44`, `di.rs:182-190`, `src-tauri/src/infrastructure/ml/model_cache.rs:60-70` (`state.entry` is a single `Option`) |
| 2 | The router and utility caches are *also* single-slot despite `String` keys: the key is compared, not indexed. Two utility keys alternate → thrash. | `model_cache.rs:29-33, 60-70` |
| 3 | **Explicit Ollama rows in the picker are a lie.** Selecting one sets the row active and `provider=ollama`, but the chat loader talks to `settings.model`, not the row's tag. | `useChatSwitchActions.ts:101-106`; `loading.rs:590` (`let model = llm_settings.model.clone()`) |
| 4 | "Try with another model" flips the **global** model and provider, regenerates, and only offers "Switch back" in a toast. Anything running in the gap uses the wrong model; an unclicked toast leaves the global changed for good. | `src/features/chat/components/message/useMessageActions.ts:99-188` |
| 5 | Warmup failure **deactivates the global chat model** (`ClearActiveChatModelUseCase`) on `ModelLoadFailed`. | `src-tauri/src/features/model_management/commands_extra/warm_up.rs:16-38` |
| 6 | Utility failures "fall back to chat LLM" in four places. With per-chat overrides "the chat LLM" is ambiguous. | `di.rs:254-259, 314-322`; `src-tauri/src/features/conversation/compaction.rs:39-45`; `src-tauri/src/features/conversation/starters.rs:480-482` |
| 7 | `conversations.model_name` has three meanings: the frontend's guess at creation (`src/features/chat/controller/useConversationActions.ts:56-70`), a copy on fork (`src-tauri/src/features/conversation/repository/fork.rs:67, 172, 283`), the learning program's recorded model on tangents (`src-tauri/src/features/learning/repository/tangents.rs:208-218`). The turn never reads it (`src-tauri/src/features/conversation/chat/turn/prepare.rs:130`). |
| 8 | `conversation_spaces.default_model_name` / `journals.default_model_name` is a dead third tier: only the frontend reads it, to seed `model_name` (`useConversationActions.ts:62-63`). The backend never resolves a model from it. | `init_schema.sql:315, 869`; §3.1 lists every reader and writer |
| 9 | Sidecar sharing is keyed by the **whole `SidecarConfig`** (`derive Eq, Hash`), and `resident_gpu_bytes` *excludes* servers on the same path, so a second window of the same GGUF is under-budgeted. | `src-tauri/src/features/llm/engine/sidecar_manager/config.rs:36-76`; `startup.rs:33-62`; `registry.rs:261-272` |
| 10 | Memory is not admitted up front: a load that does not fit the GPU retries with `n_gpu_layers=0` and is reported as "degraded" in a log line only. | `factory.rs:317-325`; `startup.rs:64-92` |
| 11 | Provenance is thin: the turn record stores `model {id, name}` from `llm.model_name()`; the UI prints the name. | `src-tauri/src/features/conversation/chat/turn/finalize.rs:67-71`; `turn_record.rs:131-134`; `src/features/chat/components/turn/TurnRecord.tsx:239` |
| 12 | A remote llama.cpp client reports `settings.context_window` without asking the server (`src-tauri/src/features/llm/llama_cpp/mod.rs:88`); three window knobs exist (`crates/lattice-core/src/application/contracts/settings/llm.rs:77-94`). |
| 13 | No learning job declares a minimum window; generators divide whatever window they are handed (`model_call.rs:1-8`, `lessons/reference_collection.rs:651`, `planning/outline_evidence.rs:75`). |
| 14 | The Settings roles grid is row-flag based (`is_active_for_*` on `models`, `src/features/settings/components/modelRoles/roleConfig.ts:28, 42, 52, 62`), so a settings-backed role needs a second activity source (§6.3). |

## 2. Domain model

### 2.1 `ModelRef` — the identity of a model, independent of Settings

`crates/lattice-core/src/domain/models/model_ref.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelRef {
    /// A downloaded GGUF row in `models` (`DownloadedModel::model_id`).
    Local { model_id: String },
    /// A tag served by the Ollama endpoint configured in Settings.
    Ollama { model: String },
    /// A model name served by the llama.cpp endpoint configured in Settings.
    LlamaCpp { model: String },
    /// A cloud provider's model; the key comes from the keyring.
    Cloud { provider: CloudProvider, model: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider { Openai, Anthropic, Google }

impl ModelRef {
    /// Stable, log-friendly form: "local:ornith-1.5-9b", "ollama:qwen3:8b", "cloud:openai/gpt-x".
    pub fn to_string(&self) -> String;
}
```

- **Endpoints are connection configuration, not identity.** `Ollama`/`LlamaCpp` carry the tag only; the URL and auth header come from Settings at resolve time (`llm.rs:102-125`). Move the server, every chat follows.
- **Cloud carries provider + model** because the keyring entry and endpoint are per provider (`loading.rs:157-183`, `src-tauri/src/features/llm/cloud.rs:44-71`).
- **Local carries `model_id`, not a path** (`loading.rs:299`, `find_by_model_id`).
- Picker ids map 1:1: `__llamacpp_server__` → `LlamaCpp{settings.llamaCpp.model}`, `__ollama_server__` → `Ollama{settings.model}`, `__cloud_provider__` → `Cloud{settings.provider, settings.model}`, a local row → `Local{model_id}`, an explicit Ollama row (`backend === 'ollama'`) → `Ollama{row.model_id}`. Finding #3 is fixed by construction.

### 2.2 `ChatModelChoice` — what a chat asked for

`crates/lattice-core/src/domain/conversation/model_choice.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatModelChoice {
    #[default]
    Default,
    Explicit { model: ModelRef },
}
```

**`Default` follows Settings live; it is not a snapshot.** (a) Settings is the one global switch, so a chat that says Default must move when Settings moves; (b) a snapshot would make every chat an implicit override and pin old chats to replaced or deleted models; (c) a snapshot needs its own repair path on every model deletion. Snapshots belong in provenance (§7), per message.

### 2.3 `ResolvedModel`, `ModelRole`, `ModelRequest`

```rust
pub struct ResolvedModel { pub model: ModelRef, pub origin: ModelOrigin }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelOrigin { Default, ChatOverride, RequestOverride, LearningRole }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole { Chat, Utility, Router, Learning, Maintenance }

pub struct ModelRequest {
    pub role: ModelRole,
    pub selection: ModelSelection,       // Default | Chat(ChatModelChoice, Option<ModelRef> /*request override*/) | Explicit(ModelRef)
    pub requirements: ModelRequirements,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelRequirements {
    /// The smallest window this work can run in, measured (never a constant). `None` = any.
    pub min_context_tokens: Option<usize>,
    pub tools: ToolSupport,              // Required | NotNeeded (default)
}
```

### 2.4 Reconciling `conversations.model_name`

Dropped, replaced by `model_choice` (§3). The aggregate field (`crates/lattice-core/src/domain/conversation/aggregate.rs:449`), the DTO field (`src-tauri/src/features/conversation/dto.rs:22`), `CreateConversationRequestDto.model_name`, the `create_conversation(title, model_name, system_prompt)` command (`src-tauri/src/features/conversation/commands.rs:139-166`), and the frontend `createConversation(title, modelName)` all lose the parameter. A chat can exist before a model does; the first turn reports the real problem instead of creation throwing "No chat model available" (`useConversationActions.ts:64-67`).

## 3. Persistence, migration, DTOs, commands, bindings

### 3.1 Migration `src-tauri/migrations/20261016120000_model_choice_and_learning_role.sql`

```sql
-- A chat's own model choice. NULL means "the Settings default, live".
-- Otherwise a ModelRef as JSON: {"kind":"local","model_id":"..."} etc.
ALTER TABLE conversations ADD COLUMN model_choice TEXT;

-- The old column recorded the frontend's guess at creation and was never
-- read by a turn. Provenance now lives on each assistant message.
ALTER TABLE conversations DROP COLUMN model_name;

-- The per-space default model was a third tier nothing resolved from.
-- Resolution is: request override > chat choice > Settings default.
ALTER TABLE conversation_spaces DROP COLUMN default_model_name;
ALTER TABLE journals DROP COLUMN default_model_name;
```

Legality: bundled SQLite is 3.46 (`libsqlite3-sys 0.30.1`, `src-tauri/Cargo.lock`; `DROP COLUMN` needs ≥ 3.35). None of the three columns is in an index, trigger, view or FK (`init_schema.sql:474-490, 722-745, 1325-1335` reference `space_id`, `title`, `updated_at`, `is_saved`; no `INDEX … default_model_name` in any migration). No table rebuild, so the sqlx `PRAGMA foreign_keys=OFF` trap does not apply.

**Every reader and writer of `default_model_name` to remove (Track B backend, Track C frontend):**

| File | Lines (df6029b8) |
|---|---|
| `src-tauri/src/features/conversation/space_dto.rs` | 14, 31, 47, 59, 72, 87 (six DTOs) |
| `src-tauri/src/features/conversation/space_repository.rs` | 41, 56, 66, 120 |
| `src-tauri/src/features/conversation/repository/workspace/spaces.rs` | 62, 96, 109, 129 |
| `src-tauri/src/features/conversation/repository/workspace/journals.rs` | 34, 52, 62, 73, 91, 120, 133, 153 |
| `src-tauri/src/features/conversation/repository/workspace/tests.rs` | 24 |
| `src-tauri/src/features/learning/repository/tangents.rs` | 139-148 (space INSERT) |
| `src/types/api/conversation.ts` | 143, 153, 164, 177 (replaced by bindings types anyway) |
| `src/features/chat/controller/useConversationActions.ts` | 62-63 |
| `src/features/journal/hooks/useJournalSpaces.ts` | 102 |
| `src/features/chat/components/sidebar/useJournalSelection.ts` | 171 |
| `src/features/chat/components/sidebar/useNewSpaceForm.ts` | 48 |
| `src/features/spaces/hooks/useSpaceEditor.ts` | 35, 55, 81, 101, 128 (`spaceModelDraft` state) |
| `src/features/chat/components/sidebar/SpacesPanel.tsx` | 70-71, 367-368 (the "Model" input in the space editor — remove the field) |
| `e2e/app-shell.spec.ts` | 845, 892 (fixture objects) |
| `src/lib/bindings.ts` | regenerated (`ConversationJournalDto`, `ConversationSpaceDto`, `Create*/Update*RequestDto`) |

### 3.2 Repository

`src-tauri/src/features/conversation/repository/conversations.rs`: every `SELECT … model_name …` (127, 202) and the INSERT (98-115) swap to `model_choice`; `persistence_mapper.rs:29, 92, 129` map `Option<String>` ↔ `ChatModelChoice` (JSON). A row whose JSON fails to parse is `AppError::Database("conversation <id> has an unreadable model_choice: <err>")`, never silently Default.

New methods on `ConversationRepository`, exposed through the `ChatRecords` port (`chat/ports.rs`):

```rust
async fn model_choice(&self, conversation_id: &str) -> Result<ChatModelChoice>;
async fn set_model_choice(&self, conversation_id: &str, choice: &ChatModelChoice) -> Result<()>; // bumps updated_at
```

Fork (`fork.rs:67, 162-172, 280-283`) copies `model_choice`. Chat tangents (`repository/tangents.rs`) inherit the parent's. Learning tangents (`learning/repository/tangents.rs:208-218`) are created with `model_choice = NULL`; the program's own `model_name` stays as the program's provenance (`learning/service.rs:182`).

### 3.3 Settings field for the Learning role (§6.3)

`crates/lattice-core/src/application/contracts/settings/llm.rs`, on `LLMSettingsDto` (which already has `#[serde(default)]` fields, e.g. `router` at 150-152):

```rust
/// The model every learning job runs on: lesson preparation, outlines,
/// practice, study decks, assessments, diagnostics, learning memory.
/// `None` means not set, and learning work refuses until it is; it never
/// borrows the chat default.
#[serde(default)]
pub learning_model: Option<ModelRef>,
```

Written through the existing `update_settings` category path (`src-tauri/src/features/settings/use_cases/update.rs:33-98`; the LLM category merges JSON keys, so the frontend sends `{category: 'llm', updates: {learningModel: {...} | null}}`). Validation in `validate_security_constraints` (`update.rs:102-121`): when `learningModel` is present and non-null, deserialize it as `ModelRef` and reject malformed JSON with `InvalidInput("learningModel is not a model reference")`. No existence check at write time beyond shape: the roles UI only offers rows that exist, and the refusal at use time names the real state (§6.3).

### 3.4 DTOs

- `ConversationDto`: remove `model_name`; add `model_choice: ChatModelChoice`. Mapper `mapper.rs:27` follows.
- `CreateConversationRequestDto`: remove `model_name`.
- `TurnModelDto` (`turn_record.rs:131-134`):
  ```rust
  pub struct TurnModelDto {
      pub id: String,                      // llm.model_name(), as today
      pub name: String,
      pub model: Option<ModelRef>,         // None only for rows written before this change
      pub origin: Option<ModelOrigin>,
      pub provider: Option<String>,        // llm.provider_name()
      pub context_tokens: Option<u32>,     // llm.max_context_tokens()
      pub degraded: Option<String>,        // §5.4: the sidecar's reason when it ran on CPU
  }
  ```
- `ChatStreamEventDto` (`chat/dto.rs`, `pub struct ChatStreamEventDto`): add `#[serde(skip_serializing_if = "Option::is_none")] pub model: Option<TurnModelDto>`, sent once right after the turn pins its model (§7).
- `DownloadedModelResponse` (`src-tauri/src/features/model_management/commands_extra.rs:50-68`): add `supports_tool_calling: Option<bool>` (from `GgufModelInfo::supports_tool_calling`, `engine/gguf_metadata.rs:136`, via the existing `ModelFileInspector`) and `configured_context_length: Option<u32>` (from `settings.llm.local_model_configs[model_id].context_length`).
- `ActiveModels` (`model_management/plugin/commands.rs:298-301`) is unchanged; the learning model is read from settings.
- `LoadedModelDto { model: ModelRef, provider: String, context_tokens: u32, in_flight: u32, pinned_by: Vec<String>, loaded_at: String, last_used_at: String, gpu_bytes: Option<u64>, degraded: Option<String> }`.

### 3.5 Commands

| Command (plugin) | Signature | Behaviour |
|---|---|---|
| `set_conversation_model_choice` (conversation) | `(conversation_id: String, choice: ChatModelChoice) -> Result<ConversationDto, ApiError>` | Validates the ref exists now (§3.6), persists, returns the DTO. Never loads, never touches Settings/`models`/caches. |
| `regenerate_response` (conversation, `plugin.rs:650-664`) | add `model: Option<ModelRef>` after `request_id` | Per-request override for this regeneration only (§4.3). Not persisted. |
| `create_conversation` (conversation, `commands.rs:159-166`) | drop `model_name` | |
| `warm_up_learning_model` (model) | `() -> Result<(), ApiError>` | Loads the Learning model via `llm_for(Learning)`; emits `model:warmup-status {role:'learning', …}`; a failure reports and **changes no setting**. |
| `list_loaded_models` (model) | `() -> Result<Vec<LoadedModelDto>, ApiError>` | Diagnostics (§11) and the manual script. |

Registered in `src-tauri/src/bin/export_bindings/main.rs` (conversation block ~248-255; model block ~220), regenerated into `src/lib/bindings.ts`; `npm run type-check:e2e` then fails until `e2e/` fixtures drop `model_name`/`defaultModelName` and add `learningModel` to the settings fixture (`e2e/helpers/learningStudioBackend.ts:1085`, `get_settings`).

### 3.6 Validation at set time (`set_conversation_model_choice`)

| Ref | Check | Error |
|---|---|---|
| `Local` | row exists (`find_by_model_id`), chat type, `loadable_path()` exists on disk (`crates/lattice-core/src/domain/models/downloaded.rs:41-47`) | `InvalidInput`: "Ornith-1.5-9B is not downloaded any more. Download it in Settings → Models or pick another model." / "…its file is missing from `<path>`." |
| `Ollama` / `LlamaCpp` | the endpoint URL in Settings is non-empty | `InvalidConfig`: "No Ollama connection is configured. Add one in Settings → Chat." |
| `Cloud` | key present in env/keyring (same lookup as `loading.rs:165-180`) | `InvalidConfig`: "No OpenAI API key is saved. Add it in Settings → Chat." |

## 4. Resolution and precedence

### 4.1 Pure resolver (lattice-core)

`crates/lattice-core/src/application/services/chat_model_resolution.rs`:

```rust
pub struct DefaultChatModel(pub Result<ModelRef>);   // computed from settings + active row, §4.1 table
pub fn default_chat_model(settings: &LLMSettingsDto, active_local: Option<&str /*model_id*/>) -> DefaultChatModel;
pub fn resolve(request_override: Option<&ModelRef>, chat: &ChatModelChoice, default: &DefaultChatModel) -> Result<ResolvedModel>;
pub fn resolve_learning(settings: &LLMSettingsDto) -> Result<ResolvedModel>;   // Err(InvalidConfig) when unset
```

Precedence for chats: **request override > chat override > Settings default.**

| `provider` | Default |
|---|---|
| `local` | `Local{active row}` or `Err(AiModelsNotInstalled("No chat model is selected in Settings → Models"))` |
| `ollama` | `Ollama{settings.model}` |
| `llamacpp` | `LlamaCpp{settings.llama_cpp.model}` |
| `openai/anthropic/google` | `Cloud{provider, settings.model}` |
| `auto` | first of: active local row, configured llama.cpp, configured Ollama (`crates/lattice-core/src/application/services/model_selection.rs:35-57`) |

`Auto`'s *load-time* fallback (local broken → llama.cpp → Ollama) is kept **only for `Default`**; an explicit ref never falls through to another backend.

### 4.2 Container API (`src-tauri/src/features/llm/di.rs`, and the `ChatModels` port)

```rust
pub struct PinnedModel {
    pub llm: Arc<dyn LLMPort>,
    pub resolved: ResolvedModel,
    pub instance_id: u64,
    pub degraded: Option<String>,
    // Drop decrements the instance's in_flight counter.
}

impl Container {
    pub async fn chat_llm_for(&self, conversation_id: &str, request_override: Option<&ModelRef>, requirements: ModelRequirements) -> Result<PinnedModel>;
    pub async fn default_chat_llm(&self) -> Result<Arc<dyn LLMPort>>;         // today's get_or_load_llm body, Maintenance role
    pub async fn learning_llm(&self, requirements: ModelRequirements) -> Result<PinnedModel>;
    pub async fn llm_for(&self, request: &ModelRequest) -> Result<PinnedModel>; // the general form the three above call
    pub async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>>; // unchanged signature
    pub async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>>;         // unchanged signature
    pub fn invalidate_llm_instances(&self, cause: InvalidationCause);           // replaces the three invalidate_* fns
    pub fn loaded_models(&self) -> Vec<LoadedModelDto>;
}
```

`get_or_load_llm()` is **removed**. Every call site (§6) must say which of the four it means.

### 4.3 Per-request override ("Try with another model")

`regenerate_response(…, model: Some(ref))` → `chat_llm_for(id, Some(&ref), …)`, `origin = RequestOverride`. **Nothing is persisted.** The toast: "Answered with X" with one action "Use X for this chat" → `set_conversation_model_choice`. No "Switch back", because nothing was switched.

## 5. LLM instance management

### 5.1 `LlmInstances` replaces the three single-slot caches

Generic `InstancePool<K, V>` in `src-tauri/src/infrastructure/ml/instance_pool.rs`; `src-tauri/src/features/llm/instances.rs` wraps it as `LlmInstances`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelKey {
    pub model: ModelRef,
    /// Sidecar launch fields only — exactly the fields that make SidecarConfig's Eq/Hash
    /// (config.rs:36-76): context_window, kv_cache_quantization, seed, parallel_slots, speculative.
    /// Empty for remote and cloud refs. Roles are NOT part of the key.
    pub launch: LaunchProfile,
}
pub struct InstanceEntry { llm: Arc<dyn LLMPort>, instance_id: u64, loaded_at, last_used_at, in_flight: AtomicU32, pinned_by: Mutex<Vec<(ModelRole, String)>>, degraded: Option<String>, generation: u64 }

impl InstancePool<K, V> {
    pub async fn get_or_load<E, F, Fut>(&self, key: K, load: F) -> Result<Arc<V>, E>;   // single flight PER KEY
    pub fn invalidate(&self, pred: impl Fn(&K) -> bool) -> Vec<K>;                     // holders keep their Arc
    pub fn evict_idle(&self, select: impl Fn(&K, &EntryStats) -> Option<u64 /*bytes*/>, needed: u64) -> Vec<(K, u64)>; // LRU among in_flight == 0
    pub fn snapshot(&self) -> Vec<(K, EntryStats)>;
}
```

Dead instances (`is_alive()` false, `llm_port.rs:228`) are evicted before a load, as `model_cache.rs:75-82` does. The utility and router roles become ordinary keys in the same pool (`Local{utility row}` + launch, `Local{router id}` + launch): if a role and a chat override resolve to the same key they share one `LLMPort` and one sidecar.

### 5.2 Sidecar sharing across roles, and the 8,704 incident

The pool key equals the sidecar key (`SidecarConfig` derives `Eq`/`Hash`, `config.rs:36-44`; `start_shared` reuses on equality, `startup.rs:42-59`, log "Reusing the running llama-server for this model; no second load" at 57). Same model + same launch profile → one process for chat, utility **and learning**. Same model, different window → two processes of the same weights; on 36 GB next to a 27B that is not affordable, so it is prevented: requirements are checked before launch (§5.4, §6.3) and the user is told what collides.

Fix `SidecarRegistry::resident_gpu_bytes` (`registry.rs:261-272`): skip "same `SidecarConfig`", not "same path". One-line change plus the comment.

### 5.3 Launch profile

`LocalServerOptions::from_settings(settings).with_model_config(settings.local_model_configs.get(model_id))` exactly as today (`loading.rs:535-536`, `di.rs:298-303`, `factory.rs:114-128`). A per-model `context_length` is honoured as written (`config.rs:412-459`). A role does not get its own window: the per-model config is the model's setting wherever it is used.

### 5.4 Admission control and degraded starts — **decided: run on CPU and say so**

Before launching a sidecar, `LlmInstances` computes `needed = resident_bytes_of(path, window)` (`config.rs:193-200`) and `spare = usable_gpu_bytes − resident_gpu_bytes(other servers)` with the auto-sizer's arithmetic (`config.rs:247-285`). Then:

1. `needed ≤ spare` → launch.
2. Else `evict_idle(needed − spare)`; re-check; launch if it fits. Each eviction is logged with bytes.
3. Else **refuse** with `AppError::ServiceNotAvailable`:
   > "Can't load Ornith-1.5-9B for this chat: it needs 6.7 GB (5.5 GB weights + 1.2 GB context at 8,704 tokens) and 3.1 GB is free. Qwen3.5-27B (15.9 GB) is answering in 'Research notes' and preparing 'Lesson 4'. Wait for them to finish, stop them, or use Qwen3.5-27B for this chat."

   Names come from `pinned_by`. Nothing is unloaded from under a running turn or job.

When a launch passes admission but the sidecar still falls back (`startup.rs:64-92`: GPU/Metal/driver fault → `n_gpu_layers=0`, reported by `handle.degraded()`, `factory.rs:317-325`), the instance **runs and says so** everywhere the model is named:

| Surface | Exact wording |
|---|---|
| Composer model button (chat header state) | label suffix "· on CPU"; `title`/`aria-label`: "Ornith-1.5-9B is running on the CPU: <sidecar reason>. Answers will be slow." The suffix comes from `PinnedModel.degraded` on the stream event's `model` field while streaming and from `list_loaded_models` otherwise. |
| Streaming line above the answer | "Answering with Ornith-1.5-9B · on CPU" |
| Turn record (`TurnRecord.tsx:239`) | "Ornith-1.5-9B · chosen for this chat · ran on CPU: <reason>" (`TurnModelDto.degraded`) |
| Lesson progress (`lesson_progress::stage`, `generation_jobs.rs:196-198`) | "Preparing with Qwen3.5-27B (on CPU: <reason>; this will be slow)" |
| Settings → Models row | warm-up status "Ready (on CPU: <reason>)" |
| Log | `model.degraded { model, instance_id, reason }` at `warn` |

Reason: the ladder exists for driver faults where CPU is the only way the model runs; memory shortfalls are already caught by admission; refusing would turn a GPU hiccup into a dead chat. Admission is skipped (and logged as skipped) when the GPU reports no memory (`capabilities.rs:56-58`).

### 5.5 Eviction, unload, invalidation

- **On demand** (§5.4 step 2) is the policy. No idle timer.
- **Model deleted** (`model_management/plugin/commands.rs:108` `delete_model`): `invalidate_llm_instances(ModelDeleted(model_id))` — running holders finish on the mmap'd file; the process dies on the last drop. **No setting is changed**: a chat override or the Learning role pointing at it stays and is shown as unavailable (§6.3, §9).
- **Chat override cleared/changed**: unloads nothing.
- **Settings changes** (`src-tauri/src/interfaces/settings_side_effects.rs:97-102` currently invalidates all three caches on any `Llm` write) → `invalidate_llm_instances(SettingsChanged{..})` with a predicate: `local_model_configs[id]` changed → keys on `Local{id}`; `ollama_url`/auth → Ollama keys; `llama_cpp.*` → LlamaCpp keys; sampling/thinking/`max_tokens`/`timeout` → all keys (`GenerationConfig` is baked into each client, `factory.rs:337-343`); `provider`/`model`/active row/`learning_model` → **nothing** (they change what resolution returns, and resolution is per request). The side-effect impl diffs the previous and new `LLMSettingsDto` to decide.
- **App exit**: `registry.rs:225` `kill_all`, unchanged.

### 5.6 Concurrency

Two chats on two local models: two processes, two schedulers (`BackendKey` per endpoint, `scheduler/registry.rs:29`). Same model: one process, `--parallel` slots via the scheduler (`llama_server_capacity`, `registry.rs:125`). Loads of different keys run in parallel; same key coalesces.

### 5.7 Invalidation while a turn is in flight

A turn holds its `PinnedModel` from `prepare` to `finalize`; the `Arc` survives any invalidation (`crates/lattice-core/src/application/ports/loaded_chat_model.rs:7-9`). Compaction for the turn uses the **same pinned instance**: `ChatPolicy::compact_for_turn(id, cancel, continuation: Arc<dyn LLMPort>)`, and `compaction::build_job` stops re-resolving (`compaction.rs:52`).

## 6. Consumer inventory

| Consumer | File:line | Classification | Notes |
|---|---|---|---|
| Chat turn | `chat/turn/prepare.rs:130` | **`chat_llm_for`** (+ request override) | Resolves once, pins. For a brand-new chat the conversation is created first (§8.1). |
| Resumed research turn | `prepare.rs:293` | **`chat_llm_for`**, `tools: Required` | Re-resolves the chat's choice after restart. |
| Research job open-conversation | `chat/research_job.rs:131` | removed | Creation needs no model. |
| Compaction for the turn | `compaction.rs:52` | pinned instance passed in | §5.7. Utility summariser unchanged (`compaction.rs:39`). |
| Consolidation after the turn | `chat_runtime.rs:194-196` | **`chat_llm_for`** (re-resolved) | |
| Handoff | `handoff.rs:359` | **`chat_llm_for(source)`**; new chat inherits `model_choice` | |
| Journal synthesis | `synthesis.rs:263, 316` | `default_chat_llm` | Scratch chat created with Default. |
| Chat starters | `starters.rs:480-482` | Utility, else **`default_chat_llm`** (explicit, logged) | |
| Router | `routing.rs:266`, `di.rs:195-230` | Router | Same settings key, now a pool entry. |
| Intent / planner / HyDE / kb retrieval | `routing.rs:94`, `retrieval/pipeline.rs:89`, `kb_retrieval.rs:182, 613` | Utility | "fall back to chat" → "fall back to Default", in the log line. |
| Grounding judge | `verification/background.rs:58, 99` (`finalize.rs:151`) | Utility | |
| Lesson generation worker | `learning/plugin/generation.rs:81-84` → `generation_jobs.rs:196-198` | **`learning_llm(requirements)`** | §6.3. |
| Outline generate / repair | `learning/plugin/program.rs:81, 128` | **`learning_llm`** | `patiently` retries transient errors only; `InvalidConfig`/`ServiceNotAvailable` refusals fail once. |
| Practice, study, practical, assessment, diagnostics, learning memory | `plugin/practice.rs:171,260,312; study.rs:34,50; practical.rs:228,344,406; assessment.rs:93,195; diagnostics.rs:44,109; memory.rs:130` | **`learning_llm`** | Requirements where a floor is measurable (e.g. `recall/study_generation.rs:318`), else `default()`. |
| Learning tangent chats | `learning/repository/tangents.rs:208` | chat (Default choice) | A tangent is a chat; it follows its own choice. |
| QA health | `qa/commands.rs:19` | `default_chat_llm` | DTO gains `model: ModelRef`. |
| Tags, compare | `tags/commands/generation.rs:149, 321`, `compare/plugin.rs:42` | `default_chat_llm` | |
| Corpus shape, summaries | `corpus_shape/commands.rs:106`, `summaries/di.rs:89` | Utility | |
| Boot prewarm | `di.rs:361` (`["chat","utility","embedding"]`) | Default + Utility + Embedding | Learning is **not** prewarmed at boot (a long job absorbs a load; a third resident model at boot is memory for nothing). |
| Warm-up commands | `warm_up.rs:16, 81` + new `warm_up_learning_model` | | **Stop clearing the active row on failure** (`warm_up.rs:21-38`). |
| Explorer folder index | `explorer/plugin.rs:137` | embedding only | nothing |

### 6.1 Why learning never follows a chat

Lessons belong to a program, not a chat. The one chat-shaped thing nearby — a learning tangent — is a chat and follows its own choice.

### 6.2 Requirements derived from measured sizes

`src-tauri/src/features/learning/model_requirements.rs` (new): one function per job kind returning `ModelRequirements` from the job's own inputs — the token count of the fixed prompt parts the generator will send, plus the answer reservation, plus the smallest evidence room the planner accepts, using the existing primitives (`model_call::fits`/`source_room`, `model_call.rs:105-120`; `grounded_generation::evidence_room`). Token counts need a counter before the model is loaded: use `estimate_tokens` (`llm_port.rs:56`) for the floor, and re-check with the loaded model's exact counter after load (the loaded model may be refused at that point too, with the same message and exact numbers). No constants.

### 6.3 The Learning model role — **decided: yes, a dedicated role**

**Persistence.** `settings.llm.learning_model: Option<ModelRef>` (§3.3). Not a `models` row flag: a row flag cannot name an Ollama tag or a llama.cpp model, and the chat role's split brain between `is_active_for_chat` and `provider` (`roleConfig.ts:82-91`) is exactly what a settings-backed ref avoids.

**Unset means refuse.** `resolve_learning` returns `InvalidConfig("No Learning model is set. Choose one in Settings → Models under Learning.")`. Shown: the learning job/command fails immediately with that text (Learning Studio surfaces command errors as it does today, e.g. the outline progress `api_error`, `program.rs:98`); Settings → Models shows a persistent line above the roles grid: "Learning: not set — lessons, outlines and study decks will not run until you choose one", with a button **"Use Qwen3.5-27B (your chat model)"** that writes the setting (explicit, one click, nothing silent). First-run setup (`src/features/model/components/FirstRun/ModelSetupModal.tsx`, PR #63 territory) is not changed in this PR; the banner covers it.

**Settings roles UI** (Track C): `roleConfig.ts` gains a fourth descriptor:

```ts
{ id: 'learning', label: 'Learning', assignLabel: 'Set as Learning', pendingLabel: 'Setting…', activeLabel: 'Active (Learning)',
  allowedTypes: ['language_model', 'chat'],
  hint: 'Prepares lessons, outlines, practice and study decks. Needs a large context window.' }
```

`RoleDescriptor.activeKey` is replaced by `isActive(model: DownloadedModel, settings: LLMSettings | undefined): boolean` (chat already special-cases, `RoleButton.tsx:40`; the learning role compares `settings.llm.learningModel` to `modelRefForRow(model)`). `assignRole(modelId, 'learning')` in `ModelRolesContext.tsx:91` → `updateSettings({category:'llm', updates:{learningModel: ref | null}})`. `OllamaMetaRow.tsx:54` adds `learning` to its roles (`ModelRef::Ollama{tag}`); `LlamaCppMetaRow.tsx` likewise. Cloud models are not offered for Learning in this change (no cloud row exists in the grid). The row that holds the role shows "Active (Learning)" and, when its configured window is below what the last refused job needed, the refusal text under the button. The unavailable case (§5.5): the banner reads "Learning model Ornith-1.5-9B is not downloaded any more" with "Choose another" (focuses the grid) — the setting is never cleared for the user.

**Resolution.** All learning consumers (§6 table) call `learning_llm(requirements)` → `llm_for(ModelRequest{role: Learning, selection: Explicit(settings.llm.learning_model?), requirements})`. The chat default and every chat's choice are invisible to it. Provenance in `learning_programs.model_name` (`learning/service.rs:182`) and `lesson_progress::model` keep recording the model actually used.

**Requirements and refusal.** Identical mechanism to §6.2; the refusal names the role:
> "Preparing 'Transformers from scratch' needs a context window of at least 20,480 tokens. Your Learning model, Qwen3.5-9B, is set to 8,704 in Settings → Models. Set it to Auto (≈32,768 on this Mac right now) or 20,480 or more, or choose a larger Learning model."

Numbers: `local_model_configs[id].context_length`, `auto_context_size` (`config.rs:151-200`), and the measured floor. The job ends `Failed` once (not requeued: `InvalidConfig` is not transient for `patiently`, `patient_model.rs:40-45`).

**Deleted model.** `delete_model` does not touch `learning_model` (§5.5). Learning jobs refuse: "Your Learning model, Ornith-1.5-9B, is not downloaded any more. Choose one in Settings → Models under Learning." The delete confirmation in Settings (`ModelDetailPanel.tsx`, the delete path) says, when the row holds the role: "This is your Learning model. Lessons will stop until you choose another."

**Sharing and pool keying.** The key has no role (§5.1). Learning on the same GGUF and launch profile as chat or utility → the same process and `LLMPort`. A learning requirement larger than that model's configured window is refused (above), never a second process.

**Warm-up.** `warm_up_learning_model` (§3.5) loads through `llm_for(Learning)` and emits `model:warmup-status {role:'learning'}`; `WarmupRole` (`src/features/model/stores/modelWarmupStore.ts:7`) gains `'learning'`; `useModelWarmupListener.ts:27` needs no change. A failure sets the row's status and **never writes a setting** (contrast `warm_up.rs:21-38`, which is also fixed). The composer's warm-up mask (`ChatPanel.tsx:133-135`) ignores the learning role.

**Ownership and tests.** Backend: Track A (settings field, resolver, `learning_llm`, warm-up command, invalidation predicate); Track B (every learning call site, `model_requirements.rs`, refusal texts, `delete_model` confirm data); Track C (roles UI, banner, warm-up store). Tests: §12 "Learning role".

### 6.4 Self-heal — **decided: LATER**

v1 refuses up front with the numbers and the fix. Rationale kept for the later change: a saved `context_length` is an explicit instruction and `with_context_override` exists to honour it as written (`config.rs:405-412`); a larger window is a different `SidecarConfig`, hence a second process of the same weights unless the smaller one is idle and evicted first; the user who set 8,704 set it for a reason the self-heal would un-decide. When `context_length` is Auto there is nothing to heal: Auto already sizes to trained length within free memory. LATER scope: "when the running server is idle *and* the model's context is Auto, relaunch at the needed window".

## 7. Turn pinning and provenance

1. `prepare` calls `chat_llm_for`, pins, and immediately streams a `ChatStreamEventDto { model: Some(TurnModelDto) }` so the UI can say "Answering with X" from the first byte.
2. `finalize.rs:67-71` writes the extended `TurnModelDto` into `metadata.turn_record.model`. Old rows keep `{id, name}`.
3. The pin drops at the end of `finalize` (or via the guard on error/cancel).
4. A picker change mid-turn writes the row; the running turn never re-reads it. The composer shows the new choice; the streaming message keeps "Answering with …" for the pinned one.

## 8. Edge semantics

- **New chat**: selecting in the picker with no active conversation creates one (the empty-state path, `ChatPanel.tsx:191-194`) and sets its choice. No pending state, no new `chat_with_conversation` parameter.
- **Fork/branch**: inherit (`fork.rs`). **Chat tangents**: inherit. **Learning tangents**: Default.
- **Handoff**: summary by the source chat's effective model; the new chat inherits `model_choice`.
- **Explorer chats**: nothing special (`ExplorerChat.tsx:108` embeds `ChatPanel`).
- **Settings default changed**: Default chats move on their next turn; overridden chats do not; nothing is invalidated.
- **Space switching**: not a model concern; spaces have no model tier any more.
- **Restart with a pending research turn**: `Requeue` → `resume_turn` (`prepare.rs:272-293`) re-resolves the chat's choice; an unavailable override fails with its §9 message, never substitutes.
- **Backup/restore onto a machine without the model**: `model_choice` is a ref; first turn fails with "not downloaded"; the picker offers "Use default". The conversation export (`export_conversations.rs:59, 236, 289`) replaces top-level `model_name` with `model_choice` JSON and must carry each assistant message's `turn_record.model` (verify the export includes message metadata; add it if not).

## 9. Failure-mode matrix

| Case | System behaviour | User sees | Fix offered |
|---|---|---|---|
| Overridden local model deleted | set-time refusal (§3.6); turn-time `AiModelsNotInstalled` | "This chat is set to Ornith-1.5-9B, which is no longer downloaded." Composer label: name + "unavailable". | Picker: "Use default (Qwen3.5-27B)"; link to Settings → Models |
| File missing/moved | `ModelLoadFailed` with the path | "…its file is missing from `<path>`." | same |
| Endpoint unreachable | `ServiceNotAvailable` with endpoint + error (`loading.rs:607-616`) | "Can't reach Ollama at `http://…`: connection refused." | "Retry", "Use default" |
| Endpoint removed from Settings | `InvalidConfig` | "This chat uses `qwen3:8b` on Ollama, but no Ollama connection is configured." | "Use default", "Open Settings → Chat" |
| Cloud key removed | `InvalidConfig` | "This chat uses GPT-x on OpenAI, but no OpenAI key is saved." | same |
| Provider changed in Settings | irrelevant to an override (the ref names its provider); Default follows Settings | — | — |
| Not enough memory | §5.4 evict idle, else refuse naming sizes and pinners | the §5.4 message | use the resident model, wait, or stop those turns |
| GPU fault → CPU fallback | §5.4: runs, says so on every surface | "· on CPU" + reason | none needed; slow but honest |
| GGUF incompatible with the bundled engine | `UnloadableArtifacts` remembers (`loading.rs:25-110`) → `ModelLoadFailed` with llama-server's tail | "Ornith-1.5-9B can't be loaded by Lattice's local engine: `<tail>`." | "Use default"; not retried until the file changes |
| Bundled binary unusable | `SidecarBinaryUnusable` → `ServiceNotAvailable` (`loading.rs:16-23`) | install message, as today | reinstall |
| Model changed while a turn streams | §7.4 | streaming keeps A; composer shows B | — |
| Learning model unset | `InvalidConfig` (§6.3) | "No Learning model is set…" + Settings banner | "Use <chat model>" button / choose a row |
| Learning model deleted | refusal (§6.3) | "Your Learning model … is not downloaded any more." | choose another |
| Window too small for the job | refusal before launch (§6.2/§6.3) | the §6.3 message with numbers | set Auto / larger / other model |
| Chat needs tools, model has none | assembler runs without tools (`assemble.rs:605`) | Generate step detail "X cannot call tools; answering without search" | pick a tool-capable model (badge in picker) |
| History + memory exceed the new window | compaction against the pinned model (§5.7); message alone too long → `InvalidInput` (`turn/mod.rs:62-63`) | "Your message is longer than Ornith-1.5-9B's 8,704-token window." | shorten / larger model |
| Tokenizer differences | counts from the pinned instance (`assemble.rs:128, 402, 540`) | — | — |
| KV prefix cache | per process; cold prompt after a switch | slower first token | — |
| Thinking support | per instance via `thinking_policy_for` (`factory.rs:336-337`) | model's own behaviour | — |
| Default missing (Default chat) | `AiModelsNotInstalled` | "No chat model is selected in Settings → Models." | open Settings |
| Set-time race (deleted between open and click) | refusal | toast with cause | pick again |
| Restart with pending research turn | §8 | that case's message | that case's fix |
| Restore elsewhere | §8 | "not downloaded" | download / use default |

## 10. Frontend

- **Composer model button** (`Composer.tsx:221-243`): effective model. Default → "Default · Qwen3.5-27B"; override → "Ornith-1.5-9B" with caption "for this chat"; unavailable → name + "unavailable"; degraded → "· on CPU" (§5.4). `aria-label`: "Model for this chat: Ornith-1.5-9B (chosen for this chat)" / "(app default)".
- **`ModelPickerPopover`** (`ModelPickerPopover.tsx:18-24`): new props `choice: ChatModelChoice`, `defaultOption: ChatModelOption | null`, `onUseDefault: () => void`; first row "Use default (Qwen3.5-27B)"; rows are `role="menuitemradio"` with `aria-checked`; local rows show "8,704 ctx · set by you" / "32K ctx" and a "no tools" badge from the new `DownloadedModelResponse` fields. PR #64's `open`/`onOpenChange`/`anchorOnly` props are kept as-is.
- **`useChatSwitchActions`** → `handleChooseChatModel(modelId, label)` and `handleUseDefault()`; both call only `VaultAPI.setConversationModelChoice`; `updateProvider`/`setActiveChatModel` are deleted from the hook (`useChatSwitchActions.ts:40-50, 79-153`). Toasts: "This chat now uses X" / "This chat now uses the app default (Y)".
- **`useMessageActions.handleTryWithModel`** → `regenerateResponse(conversationId, { model })`; toast "Answered with X" + "Use X for this chat". Lines 61-67, 99-166 go.
- **Settings is the only global switch**: `ModelRolesContext.tsx:105`, `ModelDetailPanel.tsx:214`. New ESLint rule (in `eslint.config.js`, a new block after the `no-restricted-syntax` block at 365, **not** inside `VIEW_BACKEND_ALLOWLIST` which #63 edits): `no-restricted-syntax` selectors forbidding `MemberExpression[property.name=/^(setActiveChatModel|clearActiveChatModel)$/]` and object literals with `updates` containing a `provider` key, for files outside `src/features/settings/**` and `src/features/model/**`.
- **Effective option**: `resolveEffectiveChatOption(choice, settings, downloadedModels)` in `chatModelSelection.ts`; `resolveChatModel` (26-39) deleted with `createConversation(title, modelName)`.
- **Turn record**: "Qwen3.5-27B · chosen for this chat" / "· app default" / "· one-off" / "· Learning model", plus "· ran on CPU: <reason>".
- **Warm-up mask** (`ChatPanel.tsx:133-135`, `modelWarmupStore.ts:84-87`): masks only chats whose effective model is Default.
- **State**: conversations live in the zustand store; after `set_conversation_model_choice` replace the entry from the returned DTO and `invalidateQueries(conversationKeys.detail(id))` (`src/shared/conversations/conversationKeys.ts`). Never touch `DOWNLOADED_MODELS_QUERY_KEY` or `['settings']` from chat code.
- **Types**: `Conversation.modelName` → `modelChoice: ChatModelChoice` (`src/types/conversation.ts:242-253`, `src/types/api/conversation.ts:26`); `ModelRef`/`ChatModelChoice`/`ModelOrigin`/`TurnModelDto`/`LoadedModelDto` imported from `src/lib/bindings.ts`.

## 11. Observability

| Event | Fields |
|---|---|
| `model.resolved` | `role`, `conversation_id?`, `request_id?`, `job_id?`, `model`, `origin`, `provider`, `context_tokens`, `instance_id`, `reused`, `load_ms` |
| `model.refused` (`warn`) | `role`, `model`, `reason_code` ∈ {`not_downloaded`,`file_missing`,`endpoint_unreachable`,`no_key`,`no_memory`,`window_too_small`,`no_tools`,`incompatible_gguf`,`learning_unset`}, the numbers used |
| `model.admission` | `model`, `needed_bytes`, `spare_bytes`, `evicted: [(model, bytes)]`, `outcome` |
| `model.degraded` (`warn`) | `model`, `instance_id`, `reason` |
| `model.evicted` | `model`, `instance_id`, `idle_for_ms`, `reason` |
| `model.invalidated` | `matched: [keys]`, `cause` |
| `chat.model_choice_set` | `conversation_id`, `from`, `to` |
| `settings.learning_model_set` | `from`, `to` |

`list_loaded_models` answers "what is loaded, who pinned it" live; the message's `turn_record.model` answers "which model ran this message and why" after the fact.

## 12. Test plan

**lattice-core unit**: resolver precedence and every `provider` row; explicit refs never fall through; `resolve_learning` unset → `InvalidConfig`; `ModelRef`/`ChatModelChoice` JSON round-trips; unreadable JSON is an error; `LLMSettingsDto` with and without `learningModel` deserialises.

**src-tauri unit**: `InstancePool` per-key single flight, parallel loads of different keys, dead eviction, `invalidate(pred)` keeps holders, `evict_idle` skips `in_flight > 0`, LRU order; admission fits / evict-then-fits / refuse-with-pinners / no-GPU skip; `resident_gpu_bytes` counts a same-path different-window server; invalidation predicate (`provider`/`learning_model` → none; `local_model_configs[x]` → x only); warm-up failure (chat and learning) leaves settings and rows untouched; `model_requirements` floors equal measured parts on fixtures.

**src-tauri integration** (`chat/tests.rs`; `test_runtime.rs` fake implements `chat_llm_for`/`learning_llm`): override chat runs on B while Default is A, provenance `ChatOverride`; global change moves a Default chat, not an overridden one; `regenerate(model: C)` persists nothing; **Learning role**: a lesson job runs on `learning_model` regardless of any chat's choice and of the chat default; unset → fails once with the text; below the floor → fails once, not requeued; deleted → refusal text, setting intact; same GGUF/profile as chat → one instance id in `list_loaded_models`. Fork/tangent/handoff inherit; learning tangent is Default. Migration applies on a populated DB (three columns dropped, FTS triggers still fire, space CRUD round-trips without `default_model_name`).

**Frontend (Vitest)**: `modelRefForOption` for all five id shapes; `resolveEffectiveChatOption`; `useChatSwitchActions` calls only `setConversationModelChoice` (global mocks uncalled; creates a conversation first when none); `useMessageActions` regenerate carries `model`, toast action persists; `ModelPickerPopover` "Use default" row, `aria-checked`, badges, degraded suffix; roles: fourth `RoleButton` renders for chat-type rows and the Ollama/llama.cpp meta rows, `assignRole('learning')` sends `{learningModel}`; banner states (unset / unavailable / set); ESLint rule fixture fails lint.

**e2e**: fixtures updated; journeys: pick a model in a folder chat → recorded command is `set_conversation_model_choice` with that id and `get_settings`/`set_active_chat_model` unchanged; Settings → Models: assign Learning → `update_settings` with `learningModel`.

**Live manual script (Josh)** — superseded by the runnable checklist in §18, which follows what was built: 1) default chat 27B, Learning 27B; chat A (main), chat B (Explorer). 2) In B pick Ornith-9B → toast; Settings unchanged; A still "Default · 27B"; send in B → "Answering with Ornith-1.5-9B"; `list_loaded_models` shows both, B pinned. 3) While B streams, pick 27B in B → streaming keeps 9B; next message uses 27B. 4) Start a lesson prep with B on 9B → progress names 27B. Set Learning to Ornith-9B (window 8,704), start another → fails immediately with the §6.3 text; no relaunch in the log. Set 9B's window to Auto → runs, shares the process with B if B is on 9B (one instance id). 5) In A "Try with Ornith-9B" → answered with 9B, A still Default; "Use for this chat" → A shows 9B. 6) Delete Ornith-9B → A, B show unavailable; Learning banner "not downloaded any more"; sending in A fails with the text + "Use default"; lesson refuses with the Learning text; nothing in Settings changed by itself. 7) Change the Ollama URL → an Ollama-override chat follows it. 8) Force a GPU fallback (e.g. `n_gpu_layers=0` via a debug env the sidecar already honours, or a model that exceeds Metal's buffer limit) → "· on CPU" on the composer, the stream line, the turn record. 9) Quit with a research turn running in B; relaunch → resumes on B's model.

## 13. Implementation plan (executable by Opus agents with no other context)

Rules: at most 3 agents at once; every cargo invocation as `lockf -k /Users/josh/Code/.lattice-cargo.lock cargo <args>`; `cargo check -p lattice-core -p lattice` and lib tests only until integration; grep before read; `npm run type-check`, `npm run lint`, `npm run test:unit` for frontend. Base: `fix/overnight-wave-2026-10-16` at or after `dad410df`.

### Order and dependencies

```
A0 (contract commit) ─┬─► A1 (instances, admission, loaders)
                      ├─► B  (conversation + learning consumers + migration)
                      └─► C  (frontend)
A1 + B + C ─► I (integration: bindings regen, full cargo test, e2e, manual script)
```

A0 is one agent, first, alone (small, ~2 h). Then A1, B, C concurrently. Then I (one agent).

### A0 — contract commit (Track A, first)

Owned files: `crates/lattice-core/src/domain/models/model_ref.rs` (new) + `mod.rs` export; `crates/lattice-core/src/domain/conversation/model_choice.rs` (new) + export; `crates/lattice-core/src/application/services/chat_model_resolution.rs` (new) + `services/mod.rs`; `crates/lattice-core/src/application/ports/llm_port.rs` (add `ModelRole`, `ModelRequirements`, `ToolSupport`, `ModelOrigin`, `ResolvedModel`); `crates/lattice-core/src/application/contracts/settings/llm.rs` (`learning_model`); `src-tauri/src/features/llm/di.rs` (new Container methods, **implemented as thin delegations to today's loader** so everything compiles: in A0 `chat_llm_for` and `learning_llm` ignore their inputs and return today's `load_llm_with_fallback` result wrapped in a `PinnedModel` with `origin: Default`, marked `// A1 replaces`; A0 touches no conversation or learning file); `src-tauri/src/features/conversation/chat/ports.rs` (`ChatModels` gains `chat_llm_for`, `learning_llm`, `default_chat_llm`; `get_or_load_llm` stays in A0 and is removed by B); `src-tauri/src/features/conversation/chat/turn_record.rs` (`TurnModelDto` fields, all `Option`); `src-tauri/src/features/conversation/chat/dto.rs` (`ChatStreamEventDto.model`); `src-tauri/src/features/model_management/commands_extra.rs` (`DownloadedModelResponse` fields, `LoadedModelDto`); `src-tauri/src/bin/export_bindings/main.rs` (register `warm_up_learning_model`, `list_loaded_models` as stubs returning `Ok(vec![])`/`Ok(())`); regenerate `src/lib/bindings.ts`.

Exposes (exact):
```rust
// lattice-core
pub enum ModelRef { Local{model_id}, Ollama{model}, LlamaCpp{model}, Cloud{provider: CloudProvider, model} }
pub enum ChatModelChoice { Default, Explicit{model: ModelRef} }
pub enum ModelRole { Chat, Utility, Router, Learning, Maintenance }
pub enum ModelOrigin { Default, ChatOverride, RequestOverride, LearningRole }
pub struct ModelRequirements { pub min_context_tokens: Option<usize>, pub tools: ToolSupport }
pub struct ResolvedModel { pub model: ModelRef, pub origin: ModelOrigin }
pub fn default_chat_model(&LLMSettingsDto, Option<&str>) -> DefaultChatModel;
pub fn resolve(Option<&ModelRef>, &ChatModelChoice, &DefaultChatModel) -> Result<ResolvedModel>;
pub fn resolve_learning(&LLMSettingsDto) -> Result<ResolvedModel>;
// LLMSettingsDto
pub learning_model: Option<ModelRef>,
// src-tauri Container + ChatModels port
pub struct PinnedModel { pub llm: Arc<dyn LLMPort>, pub resolved: ResolvedModel, pub instance_id: u64, pub degraded: Option<String> }
async fn chat_llm_for(&self, conversation_id: &str, request_override: Option<&ModelRef>, requirements: ModelRequirements) -> Result<PinnedModel>;
async fn learning_llm(&self, requirements: ModelRequirements) -> Result<PinnedModel>;
async fn default_chat_llm(&self) -> Result<Arc<dyn LLMPort>>;
```
Tests: resolver unit tests; settings DTO deserialisation with/without the field. Done when: `cargo check` + `cargo test -p lattice-core` green; bindings regenerated and `npm run type-check` green (e2e type-check may fail on the new settings field — C fixes fixtures).

### A1 — instances, admission, loaders (Track A, after A0)

Owned files: `src-tauri/src/infrastructure/ml/instance_pool.rs` (new), `src-tauri/src/features/llm/instances.rs` (new), `src-tauri/src/features/llm/di.rs`, `src-tauri/src/features/llm/loading.rs` (add `load_ref(&ModelRef, &LLMSettingsDto, GenerationConfig) -> Result<Arc<dyn LLMPort>>`, `launch_profile_for(&ModelRef, &LLMSettingsDto) -> LaunchProfile`; keep `load(settings)` for Default), `src-tauri/src/features/llm/engine/sidecar_manager/registry.rs` (resident bytes fix; `pub fn admission(&self, path, window) -> Admission`), `src-tauri/src/features/llm/engine/sidecar_manager/config.rs` (make `auto_context_size`, `resident_bytes_of` `pub(crate)`), `src-tauri/src/interfaces/di/container.rs` (replace the three caches with one `LlmInstances`), `src-tauri/src/interfaces/settings_side_effects.rs` (predicate invalidation, diff old/new settings), `src-tauri/src/features/model_management/commands_extra/{active_models.rs, warm_up.rs}` (stop clearing; `warm_up_learning_model_impl`), `src-tauri/src/features/model_management/plugin/commands.rs` (the two new commands; `delete_model` → `invalidate_llm_instances(ModelDeleted)`), `src-tauri/src/features/llm/loading/tests.rs`, new `instances/tests.rs`.

Consumes A0's types. Exposes: the Container methods for real; `invalidate_llm_instances(InvalidationCause)`; `loaded_models()`. Keeps `get_or_load_utility_llm`/`get_or_load_router_llm`/`utility_llm_loader` signatures (they now go through the pool). `chat_llm_for` reads the choice through `self.conversation_records().model_choice(id)` — the `ChatRecords` method B adds; until B lands, A1 calls `ConversationRepository::model_choice` directly on `db_pool()` with the SQL `SELECT model_choice FROM conversations WHERE id = ?` (the column exists once B's migration is applied; A1's tests use an in-memory DB with B's migration file, which B commits first thing — see B order).

Tests: §12 src-tauri unit list. Done when: `cargo check` green; `cargo test -p lattice --lib llm::` and `infrastructure::ml::` green; `list_loaded_models` returns the sidecar(s) in a manual run.

### B — conversation, migration, learning consumers (Track B, after A0)

Order inside B: (1) commit the migration file alone first (A1 depends on it); (2) repository + DTOs + mapper; (3) commands; (4) turn/pin/provenance; (5) consumers; (6) learning requirements; (7) tests.

Owned files: `src-tauri/migrations/20261016120000_model_choice_and_learning_role.sql`; `src-tauri/src/features/conversation/{dto.rs, mapper.rs, persistence_mapper.rs, commands.rs, plugin.rs, plugin_impl.rs, service.rs, trait_def.rs, mocks.rs, handoff.rs, synthesis.rs, starters.rs, compaction.rs, space_dto.rs, space_repository.rs}`; `conversation/repository/{conversations.rs, fork.rs, tangents.rs, workspace/spaces.rs, workspace/journals.rs, workspace/tests.rs}`; `conversation/chat/{ports.rs (remove get_or_load_llm; ChatRecords::model_choice/set_model_choice; ChatPolicy::compact_for_turn signature), research_job.rs, test_runtime.rs, tests.rs}`; `conversation/chat/turn/{prepare.rs, finalize.rs, mod.rs}`; `conversation/di/chat_runtime.rs`; `crates/lattice-core/src/domain/conversation/aggregate.rs` (drop `model_name`); `learning/{model_requirements.rs (new), plugin/generation.rs, plugin/program.rs, plugin/practice.rs, plugin/study.rs, plugin/practical.rs, plugin/assessment.rs, plugin/diagnostics.rs, plugin/memory.rs, lessons/generation_jobs.rs, repository/tangents.rs}`; `qa/commands.rs`, `qa/dto.rs`; `tags/commands/generation.rs`; `compare/plugin.rs`; `backup/{export_repository.rs, use_cases/export_conversations.rs}`; `src-tauri/src/bin/export_bindings/main.rs` (register `set_conversation_model_choice`, update `regenerate_response`, `create_conversation`).

Consumes: A0 types and Container methods (A1's real implementations arrive at integration; B's tests use `test_runtime.rs` fakes). Exposes: the commands in §3.5 with exact signatures; `ChatRecords::{model_choice, set_model_choice}`; `learning::model_requirements::{lesson_preparation, outline, study_deck, practice, assessment, diagnostics}(…) -> ModelRequirements`.

Tests: §12 src-tauri integration list; migration test on a populated DB. Done when: `cargo check` green; `cargo test -p lattice --lib conversation::` and `learning::` green; `grep -rn "get_or_load_llm" src-tauri/src` returns nothing; `grep -rn "default_model_name\|model_name" src-tauri/src/features/conversation` returns nothing.

### C — frontend (Track C, after A0's bindings)

Owned files: `src/utils/chatModelSelection.ts` + `src/utils/__tests__/chatModelSelection.test.ts`; `src/features/chat/hooks/useChatSwitchActions.ts`; `src/features/chat/components/message/useMessageActions.ts`; `src/features/chat/components/{ModelPickerPopover.tsx, ChatPanel.tsx, Message.tsx, MessageActions.tsx}`; `src/features/chat/components/composer/Composer.tsx`; `src/features/chat/components/turn/TurnRecord.tsx`; `src/features/chat/components/sidebar/{SpacesPanel.tsx, useNewSpaceForm.ts, useJournalSelection.ts}`; `src/features/chat/controller/useConversationActions.ts`; `src/features/spaces/hooks/useSpaceEditor.ts`; `src/features/journal/hooks/useJournalSpaces.ts`; `src/shared/conversations/{conversationsStore.ts, conversationsStore.types.ts}`; `src/types/{conversation.ts, api/conversation.ts}`; `src/features/model/stores/modelWarmupStore.ts`; `src/features/model/hooks/useDownloadedModels.ts` (add `warmUpLearningModel`); `src/features/settings/components/modelRoles/{roleConfig.ts, RoleButton.tsx, ModelRolesContext.tsx, LocalModelRow.tsx, OllamaMetaRow.tsx, LlamaCppMetaRow.tsx}` + their tests; `src/features/settings/components/ModelCatalog/ModelDetailPanel.tsx` (delete confirm text); `src/lib/api.ts`/`apiClients.ts` wrappers; `eslint.config.js` (new block only); `e2e/fixtures/**`, `e2e/helpers/**`, `e2e/app-shell.spec.ts`, `e2e/explorer.spec.ts`.

Consumes from bindings: `ModelRef`, `ChatModelChoice`, `ModelOrigin`, `TurnModelDto`, `LoadedModelDto`, `LLMSettingsDto.learningModel`, `commands.setConversationModelChoice(conversationId, choice)`, `commands.regenerateResponse(conversationId, toolPreferences, requestId, model)`, `commands.createConversation(title, systemPrompt)`, `commands.warmUpLearningModel()`, `commands.listLoadedModels()`. Exposes: `modelRefForOption(option, settings): ModelRef`, `modelRefForRow(model: DownloadedModel): ModelRef`, `resolveEffectiveChatOption(choice, settings, models): ChatModelOption | { unavailable: true, label } | null`, store actions `setConversationModelChoice(id, choice)`, `regenerateResponse(id, { model? })`.

Tests: §12 frontend + e2e lists. Done when: `npm run type-check`, `npm run type-check:e2e`, `npm run lint`, `npm run test:unit` green; `grep -rn "setActiveChatModel\|updateProvider" src/features/chat` returns nothing; `grep -rn "defaultModelName\|modelName" src e2e` returns only bindings-derived learning program fields.

### I — integration (one agent, after A1 + B + C)

Regenerate bindings (`cargo run --features bindings-export --bin export_bindings -- --check` then without `--check`); `cargo test` for the workspace once; `npm run test:all` minus desktop; run the manual script §12; update `docs/design` with the as-built notes; open the PR against `main` after #66 merges (or stacked on #66 if Josh prefers; note the stacked-PR stranding rule).

### Conflict surface (updated)

- **#61** (`llm/stable-summary-window`): **merged** into main (`ae85a7d9`) and into the wave branch at `dad410df`. Build on `dad410df` or later; B's edits to `ports.rs`, `chat_runtime.rs`, `compaction.rs`, `finalize.rs`, `turn/mod.rs`, `test_runtime.rs` apply on top of #61's.
- **#64** (`ui/polish-2026-10-16`): adds optional `open`/`onOpenChange`/`anchorOnly` to `ModelPickerPopover.tsx` and edits `MessageActions.tsx`. C adds `choice`/`defaultOption`/`onUseDefault` to the same props interface → textual conflict in the props block and the `Popover.Root` line. Land #64 first; C keeps #64's props.
- **#63** (`arch/views-through-clients`): edits `eslint.config.js` `VIEW_BACKEND_ALLOWLIST` (line 67 block) and moves the chat sidebar `workspaceQueries` to `src/features/chat/hooks/workspaceQueries.ts`; also touches `ChatView.tsx`, `KnowledgePanel.tsx`, `useNewSpaceForm.ts`, `useJournalSelection.ts`, `ModelSetupModal.tsx`, `ModelsTab.tsx`. C's ESLint change is a **separate new block** (not the allowlist), and C's edits to `useNewSpaceForm.ts:48` / `useJournalSelection.ts:171` are one-line field removals → small conflicts; land #63 first and C re-applies the one-liners. If #63 moved `workspaceQueries`, C's space field removals follow it to the new path.
- **#65** (`arch/infra-no-features`): touches `sidecar_manager.rs`, `registry.rs`, `persistence.rs`, `model_management/mod.rs`. A1 edits `registry.rs` (resident bytes + admission) → land #65 first.
- **#66** (the wave itself): the base.

## 14. Decisions taken (nothing open)

1. **Low memory / GPU fault for a chat's model**: run on CPU and say so — wording and surfaces in §5.4.
2. **Learning model role**: yes — full design in §6.3 (settings field `llm.learning_model`, unset = refuse with a one-click explicit fix, roles UI, resolution, requirements, deletion, sharing, warm-up, ownership, tests).
3. **`default_model_name`**: dropped from `conversation_spaces` and `journals` in the migration; every reader and writer listed in §3.1.
4. **Self-heal**: LATER, rationale kept in §6.4.

## 15. Non-goals

No per-space model tier. No per-message persisted model. No idle unload timer. No cloud option for the Learning role in this change. No change to first-run setup beyond the Settings banner.

## 16. Citation index

Chat cache `di.rs:182-190`; aliases `container.rs:41-44`; cache internals `model_cache.rs:29-33, 60-82, 104-133`; loader `loading.rs:143-192, 495-569, 575-620 (590)`; cloud `loading.rs:157-183`, `cloud.rs:44-71`; sidecar factory `factory.rs:83-128, 229-346 (317-325 degraded, 337-343 config)`; sidecar key `config.rs:36-76`; context override `config.rs:412-459`; auto size `config.rs:151-200, 247-285`; share `startup.rs:33-62`; resident bytes `registry.rs:261-272`; kill `registry.rs:225`; handle drop `process.rs:157-165`; LLMPort `llm_port.rs:112-230`; prepare `prepare.rs:11-39, 125-134, 272-293`; finalize `finalize.rs:67-71, 151`; turn DTOs `turn_record.rs:131-134`; stream DTO `chat/dto.rs` (`ChatStreamEventDto`); request `turn/mod.rs:44-51`; ports `chat/ports.rs:25-36, 386-396`; runtime impl `chat_runtime.rs:14-30, 187-196`; compaction `compaction.rs:36-60`; learning loaders `learning/plugin/generation.rs:81-84`, `program.rs:81, 128`, `generation_jobs.rs:196-198, 343`; learning window contract `model_call.rs:1-8, 105-120`; warm-up clears row `warm_up.rs:16-38`; set active `active_models.rs:9-35`; settings invalidation `settings_side_effects.rs:97-102`, `settings/plugin.rs:614-616`; settings update `use_cases/update.rs:33-98, 102-121`; schema `init_schema.sql:308-315, 345-367, 380-396, 862-869`; fork copy `fork.rs:67, 162-172, 280-283`; roles UI `roleConfig.ts:16-91`, `RoleButton.tsx:26-60`, `ModelRolesContext.tsx:91-112`, `OllamaMetaRow.tsx:54`, `LocalModelRow.tsx:174-175`; warm-up store `modelWarmupStore.ts:7, 84-87`, `useModelWarmupListener.ts:27`; frontend switch `useChatSwitchActions.ts:40-50, 79-153 (126)`; try-with `useMessageActions.ts:61-67, 99-188`; selection utils `chatModelSelection.ts:6-10, 26-39, 72-109, 111-230`; picker `ModelPickerPopover.tsx:18-24, 95-100`; composer `Composer.tsx:221-243`; panel `ChatPanel.tsx:48, 133-151, 183-188, 191-194, 393-399`; create `useConversationActions.ts:56-70`; models hook `useDownloadedModels.ts:23, 43-54, 68, 89, 120-133`; settings callers `ModelRolesContext.tsx:105`, `ModelDetailPanel.tsx:214`; record UI `TurnRecord.tsx:239`; explorer `ExplorerChat.tsx:108`; bindings `bindings.ts:1090-1092, 1146, 1818`; eslint `eslint.config.js:67, 365, 449-461`; e2e settings fixture `e2e/helpers/learningStudioBackend.ts:1085`.

## 17. As built

Built by tracks A0 (`bf661de2`), B (`b9c892ad`…`faec7fad`), C (`71c52a83`…`0275b85a`), A1 (`91342d8a`, `904bce71`, `eab0a15f`, `9faaad9a`, `f39f83bc`, `97137902`) and integration I: merge `d138dc64` (main at `bba563ec`), `27589047` (memory refusal fails once), `ab5da4d0` (the window shortfall beside the Learning role), `6292f697` (clippy), `dc7ffcf4` (e2e label), `73f287fb` (structured-call window refusal), merge `a1e7d767` (main at `32c379c4`). What differs from §1–§16, and why:

**Contracts and resolution**
- `PinnedModel` lives in lattice-core (`application/ports/llm_port.rs`), not in `src-tauri`: the `ChatModels` port in core names it in its signatures.
- `ChatModelChoice::to_stored` / `from_stored` exist: the repository stores Default as `NULL` and a choice as `ModelRef` JSON, and the backup export writes the same shape.
- `createConversation` keeps its request object (`CreateConversationRequestDto` without `model_name`), not positional `(title, systemPrompt)`.
- `chat_llm_for` reads `model_choice` with its own `SELECT model_choice FROM conversations WHERE id = ?` in the llm feature instead of `conversation_records().model_choice(id)`, so llm does not import conversation (no new feature edge).
- QA health resolves through `pinned_default_chat_llm`; its DTO's `model` is the `ModelRef` the default resolved to, plus a new `model_name` (what the server reports). `default_chat_llm` stays for callers that only need the client.

**Instance pool and admission (§5)**
- The pool key is `ModelKey { model, launch, tuning }`. `Tuning` (Standard / Utility / Router) is new: the utility and router clients carry their own output ceiling and temperature, so they are separate clients — but they share the one server process, because the sidecar is keyed by the launch profile alone. Chat, learning and maintenance share `Standard`, hence one client.
- Admission prices a new bundled server against every running server except its own (another window of the same GGUF counts).
- `load_ref` returns the client with its server facts (`CreatedLlm`), and `load(settings)` was removed: the Settings default also loads through `load_ref` after resolution.
- **The memory refusal is `ModelLoadFailed`, not `ServiceNotAvailable`** (§5.4, §9; integration commit "Fail learning work once when its model does not fit in GPU memory"). `ServiceNotAvailable` is transient to the job runtime and to `patiently`, so a lesson job was deferred and redelivered without end and an outline waited and retried, with the refusal never shown. `ModelLoadFailed` is what an engine's `InsufficientMemory` already maps to and is not transient: a lesson job fails once with the refusal as written, an outline returns it at once. Side effect: compaction treats a utility model refused for memory as "utility failed to load" and summarises with the turn's pinned chat model. The §6 table's note that `patiently` fails `ServiceNotAvailable` refusals once was wrong for that variant; it holds now. `ModelRejected` (server rejections) is a separate change on another branch.

**Learning role (§6.2, §6.3)**
- Floors are twice each job's answer reservation (the planner never reserves more than half a window): lessons 20,000 tokens (`LESSON_OUTPUT_RESERVE` 10,000), outlines 8,000, study decks and recall drafts by card count; practice, assessment, diagnostics and conversation decks size their answers to fit and state none. The 20,480 in §6.3's example is illustrative.
- The refusal raised when the Learning model is loaded (or its saved window is checked before launch) is generic — "Learning work needs a context window of at least 20,000 tokens. Your Learning model, X, is set to 8,704 in Settings → Models. Set it to Auto (≈N on this Mac right now) or 20,000 or more, or choose a larger Learning model." — because the instance pool does not know the course. The learning feature's own `ensure_window` check names the work ("Preparing 'X' needs…"), but every learning call passes its requirements to `learning_llm`, so the pool refuses first.
- **The refusal text beside the Learning role (built by I).** `last_learning_window_shortfall` (model plugin) returns `LearningWindowShortfallDto { model, needed_tokens, window_tokens, window_is_saved }`: the last learning work refused because the Learning model's window was too small. `LlmInstances` records it where the pool refuses a Learning window and forgets it once that model serves learning work with a window that covers it. Settings → Models shows "Learning work needs a context window of N tokens; this model has M." on the row that holds the role — local rows, the Ollama row and the llama.cpp row — comparing against the row's saved window, so raising the window far enough clears the line at once and moving it to Auto clears it until work runs again. Two deviations: the line is in the row's status area, beside "Didn't load" and "on CPU", not literally under the button, because the role buttons sit in a wrapping toolbar; and it is held in memory, so it is gone after a restart (the failed job keeps its message in Learning Studio).
- Lesson and outline material that overflows the window (the structured call in `lessons/structured_call.rs`) is refused as that window refusal too — "The requested learning material does not fit in X's N-token context window. Raise your Learning model's context length in Settings → Models, or choose a Learning model with a larger window" — instead of telling the learner to trim their sources, which that step gives them no way to do. `model_call::send` takes `TooLarge::Input` (learner-shortenable, `InvalidInput`) or `TooLarge::Window` (`window_too_small`, `InvalidConfig`); a plain message is `Input`, so the other callers are unchanged.
- The delete warning for the Learning model is on `LocalModelRow`'s delete dialog, where downloaded models are deleted, not `ModelDetailPanel.tsx`.

**Frontend (§10)**
- API wrappers went into the feature clients (`src/features/*/api/client.ts`), which `VaultAPI` composes, not `src/lib/api.ts`.
- The global-chat-model ESLint rule is two blocks: a flat-config block replaces an earlier block's `no-restricted-syntax`, so view files get a second block carrying the native-access selectors and the chat-model selectors together.

**Merges with `origin/main`**
- First merge (`bba563ec`, #69–#75):
  - `eslint.config.js`: `VIEW_BACKEND_ALLOWLIST` is empty — main had emptied it except for `LocalModelRow.tsx`, which this branch had already moved onto hooks.
  - `infrastructure/services/startup_reconciliation.rs` moved to `bootstrap/startup_reconciliation.rs` (#71); its test INSERTs without `conversations.model_name` (`0567ed8a`) moved with it.
  - No other textual conflicts; #74's import paths in `conversation/di/chat_runtime.rs` and `interfaces/di/container/port_bindings.rs` merged cleanly. Main's PDF-import journey looked for the old picker label "llama.cpp · qwen-test.gguf"; it now checks the composer button's accessible name, "Model for this chat: qwen-test.gguf (app default)."
- Second merge (`32c379c4`, #76–#80): `ChatStreamEventDto` carries both the pinned `model` and main's `captured` sources; `model_call::send` takes main's signature without a deadline plus `TooLarge`; practice tutoring and worked solutions run on the Learning model with main's calls, which dropped the tutor pace; `chat_runtime.rs` imports explorer and tool vocabulary from lattice-core; main's new `capture_tests.rs` creates its conversation without a model name. Bindings regenerated.

**§12 corrections**
- There is no debug switch that forces the CPU fallback; the ladder only runs when the GPU start fails (§18 step 9).
- `list_loaded_models` has no screen of its own; sharing shows in the log as `model.resolved` lines with the same `instance_id` and `reused=true` (§18).

## 18. Live checklist (Josh)

Run on the built app with a real model set. Logs: `~/Library/Application Support/lattice/logs/lattice.log.<date>`; `grep -E "model\.(resolved|refused|admission|evicted|invalidated|degraded)|chat\.model_choice_set|settings\.learning_model_set"` shows every decision. Names below use Qwen3.5-27B as the Settings default and Ornith-1.5-9B as the second model; substitute yours. Tick each box only when every "Expect" holds.

**0. Setup**
- [ ] Settings → Models: Qwen3.5-27B holds Chat. Learning not set yet. Ornith-1.5-9B downloaded, its window (Configure model) on Auto.
- [ ] Open chat A in the main chat view and chat B from Explorer (choose a folder, open its chat). Expect both composer buttons to read "Default · Qwen3.5-27B".

**1. Per-chat override in the main chat**
- [ ] In A open the model picker. Expect a "Use default (Qwen3.5-27B)" row first, rows as radio items, window badges ("32K ctx" / "8,704 ctx · set by you") and "no tools" where it applies.
- [ ] Pick Ornith-1.5-9B. Expect the toast "This chat now uses Ornith-1.5-9B", the button "Ornith-1.5-9B for this chat", a `chat.model_choice_set` log line, and Settings → Models unchanged (Qwen3.5-27B still holds Chat).
- [ ] Send a message. Expect "Answering with Ornith-1.5-9B" above the streaming answer and, once saved, the turn record "Ornith-1.5-9B · chosen for this chat".
- [ ] While an answer streams in A, pick Qwen3.5-27B. Expect the streaming answer to finish on Ornith-1.5-9B; the next message answers with Qwen3.5-27B.

**2. Per-chat override in an Explorer chat**
- [ ] In B pick Ornith-1.5-9B and send. Expect "Ornith-1.5-9B for this chat" in B only; A shows its own choice.
- [ ] With A on Qwen3.5-27B and B on Ornith-1.5-9B, send in both. Expect two `model.resolved` lines with different `instance_id`s, both models loaded at once, and neither unloaded by the other.
- [ ] Pick "Use default" in B. Expect "This chat now uses the app default (Qwen3.5-27B)".

**3. A global change does not move an overridden chat**
- [ ] Set A to Ornith-1.5-9B; leave a third chat C on Default.
- [ ] In Settings → Models make another model hold Chat (or change the chat provider). Expect C's button to follow ("Default · <new model>") and A to stay "Ornith-1.5-9B for this chat"; send in both and check the turn records ("· app default" vs "· chosen for this chat").
- [ ] Put Qwen3.5-27B back on Chat.

**4. "Try with another model" changes nothing global**
- [ ] In C (Default), on an answer choose Try with another model → Ornith-1.5-9B. Expect the toast "Answered with Ornith-1.5-9B" with "Use Ornith-1.5-9B for this chat", the new answer's record "· one-off", C's button still "Default · Qwen3.5-27B", and Settings → Models unchanged.
- [ ] Click "Use Ornith-1.5-9B for this chat". Expect "This chat now uses Ornith-1.5-9B" and C's button to switch; Settings still unchanged.

**5. Learning role unset**
- [ ] With Learning unset, Settings → Models shows "Learning: not set — lessons, outlines and study decks will not run until you choose one" with "Use Qwen3.5-27B (your chat model)".
- [ ] In Learning Studio prepare a lesson. Expect it to fail at once with "No Learning model is set. Choose one in Settings → Models under Learning." and not to retry (the job stays Failed).
- [ ] Click "Use Qwen3.5-27B (your chat model)". Expect a `settings.learning_model_set` line, the banner gone, and Qwen3.5-27B's Learning button pressed.

**6. A lesson job ignores every chat's picker**
- [ ] With A and B on Ornith-1.5-9B, prepare a lesson. Expect lesson progress "Model: Qwen3.5-27B" and a `model.resolved role=Learning model=…qwen…` line; no chat's choice or the chat default appears.
- [ ] While it runs, change A's model. Expect the lesson to keep its model to the end.

**7. Too small a window is refused, once, with the numbers**
- [ ] Set Learning to Ornith-1.5-9B and its window (Configure model) to 8,704. Prepare a lesson. Expect an immediate failure reading "Learning work needs a context window of at least 20,000 tokens. Your Learning model, Ornith-1.5-9B, is set to 8,704 in Settings → Models. Set it to Auto (≈N on this Mac right now) or 20,000 or more, or choose a larger Learning model." (the Auto clause appears only when Auto would be large enough), a `model.refused … reason_code="window_too_small"` line, no new llama-server launch in the log, and no retry.
- [ ] Open Settings → Models. Expect under Ornith-1.5-9B: "Learning work needs a context window of 20,000 tokens; this model has 8,704."
- [ ] Set its window to Auto. Expect that line to disappear; prepare the lesson again and expect it to run. If B is on Ornith-1.5-9B with the same window, expect the same `instance_id` for B's turn and the lesson.

**8. Deleting the Learning model**
- [ ] Delete Ornith-1.5-9B from Settings → Models. Expect the dialog to warn "This is your Learning model. Lessons will stop until you choose another."
- [ ] After deleting: the banner reads "Learning model Ornith-1.5-9B is not downloaded any more" with "Choose another"; chats set to it show "Ornith-1.5-9B unavailable" and the notice "This chat is set to Ornith-1.5-9B, which is no longer downloaded." with "Use default"; a message sent there anyway fails with the same sentence (the backend names the model by its id); a lesson fails once with "Your Learning model, <id>, is not downloaded any more. Choose one in Settings → Models under Learning."
- [ ] Expect nothing in Settings to have changed by itself: the Learning setting still names Ornith-1.5-9B until you choose another.

**9. Degraded CPU start wording**
- [ ] There is no switch that forces it; it happens only when the GPU start of a llama-server fails and the sidecar retries on the CPU (a `model.degraded` warn line and "running in a degraded fallback configuration"). If you see one: expect "· on CPU" on the composer button with the reason in its tooltip, "Answering with X · on CPU" while streaming, the turn record "X · <origin> · ran on CPU: <reason>", and "Ready (on CPU: <reason>)" / "Loaded (on CPU: <reason>)" on the Settings row. If none occurs in this run, mark N/A: the wording is covered by `LocalModelRow.test.tsx`, `ChatModelButton`/`TurnRecord` tests and `instances/tests.rs`.

**10. Not enough memory**
- [ ] With Qwen3.5-27B answering in one chat, start a second chat on a model too large to fit beside it. Expect the refusal "Can't load X for this chat: it needs … and … is free. Qwen3.5-27B (… GB) is answering in '<chat>'. Wait for them to finish, stop them, or use Qwen3.5-27B for this chat.", a `model.admission … outcome` line, and the busy model still loaded.
- [ ] Do the same for a lesson (Learning set to the large model while a chat answers). Expect the lesson to fail once with that message, not to sit "retrying automatically".

**11. Restart**
- [ ] Quit with a research turn running in a chat set to a non-default model; relaunch. Expect it to resume on that chat's model (`model.resolved … origin=ChatOverride`).
