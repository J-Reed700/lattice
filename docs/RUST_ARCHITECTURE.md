# Rust architecture and verification

## Dependency direction

The desktop application owns persistence and runtime ports under
`src-tauri/src/application/ports`. Domain values do not own SQL drivers,
transport adapters, or application orchestration. Composition remains in the
DI modules; adapters implement ports in infrastructure or existing feature
repository modules.

The sync API follows the same direction: HTTP and PostgreSQL depend on the
sync service/repository contracts. HTTP error rendering and SQL row decoding
belong to their respective adapters, not the sync contracts.

## Module layout

- Each vertical feature under `src-tauri/src/features/<name>/` owns its
  engine, repositories, services, commands and plugin. Two production
  modules still load through `#[path]`:
  `learning/planning/curriculum_repository.rs` (`generation_recovery.rs`) and
  the macOS half of `infrastructure/setup/renderer_shutdown.rs`; the other
  `#[path]` uses are test modules. The former `infrastructure::{indexing,llm,
  qa,search,web}` aliases are gone, and infrastructure no longer re-exports
  feature types (one exception:
  `infrastructure/persistence/repositories/sqlite_model_repository.rs`
  re-exports `model_management`'s `SqliteModelRepository`). Callers import
  `crate::features::<name>::...` directly.
- Modules used by one feature live in it: conversation memory
  (`features/conversation/memory/`), the chat intent classifier and router
  (`conversation/chat/{intent,router}.rs`), the article extractor and stealth
  fetching (`features/web/services/{article_extractor,stealth}.rs`), and the
  model manager (`features/model_management/model_manager`).
- Domain values that the application layer depends on (search value objects
  and results, conversation aggregates, download sessions and snapshots, QA
  interpretation types, model download events) live in `domain/`, not in a
  feature, so the layer rules above remain checkable.
- `interfaces/di/container.rs` holds construction and runtime state. 25
  features expose accessors through an `impl Container` block in their own
  `di.rs` (conversation's is `di/`). batch, cache, compare, corpus_shape,
  daily_notes, download, explorer, huggingface, jobs, learning, qa and
  references have no registrar. `AppContainer` is gone.
- Large modules are split by responsibility with façades that preserve public
  paths. This is an incremental convention, not an enforced line-count limit.
  Chat separates wire contracts, routing, tool selection, prompt migration,
  turn execution, and tests. The sidecar manager separates configuration,
  process ownership, registry cleanup, startup policy, failure classification,
  preflight, readiness, and tests. Some workflow and repository files remain
  large and still need further decomposition.
- The crate has no blanket `dead_code`, `unused_imports`, `unused_variables`,
  or `deprecated` allowances. Unread fields that must exist (guards,
  `FromRow` columns, serialized DTO fields) carry a targeted `#[allow]` with a
  reason comment.
- Ignored tests always carry a reason string; the remaining ignores are
  environment-gated (OS keyring, network, downloaded models, Ollama, Docker).

## Runtime ownership

- `ModelCache` owns generation-based invalidation and single-flight loading.
  Invalidated work can finish for its original caller but cannot publish into
  a newer cache generation. Cancellation releases the loader lock.
- `EmbeddingRuntime` owns readiness checks and retry cooldown. Unready degraded
  providers are returned without being cached; stale work cannot evict a newer
  provider or install a failure cooldown for it.
- Consumers receive read-only loaded-model ports rather than mutable cache locks.
- Model loaders own artifact opening and compatibility checks. The container
  supplies dependencies and settings instead of implementing model factories.
- Embedding artifact identity is established when a model is activated and
  stored on its `models` row. Launch and model load read it; nothing on the
  launch path hashes model files.

## Persistence boundaries

- Conversation services receive a repository port. Pruning deletes messages and
  bookmarks and recomputes totals in one transaction, scoped to the conversation.
- Turn completion changes the pending user message and inserts the assistant
  response in one transaction. A late failure marker only affects a still-pending
  user message; it cannot undo an already committed response.
- Prompt assembly is an application service using read-only history and
  supplemental-context ports. History and document references use one aggregate
  snapshot. Supplemental data is a separate read, not a globally consistent
  database snapshot.
- Favorites and recent-document commands share repositories with tool execution.
  Recent access tracking and bounded-history eviction are transactional. Favorite
  insertion and metadata retrieval also commit together.
- File-library operations use the normalized documents/files schema. Folder
  removal atomically removes watch entries and matching documents, without
  deleting disk files. Path validation remains at the command boundary; escaped
  subtree matching and deletion counts belong to persistence.
- Web/file import share a document-scope port; batch memberships are atomic and
  idempotent.
- PostgreSQL pushes serialize per user before allocating operation sequences or
  checking document heads. This protects absent-head optimistic checks and
  same-user commit order. Different users remain independent; the tradeoff is
  that concurrent pushes from one user queue behind each other.
## Conversation workspace ownership

- `features/conversation/plugin_impl.rs` is the command facade. Existing IPC names,
  request shapes, error mapping, and generated binding imports are preserved.
- The existing `ConversationRepository` owns workspace persistence through
  `repository/workspace/{journals,spaces,memberships,sources,state,bookmarks,explorer,retrieval}`.
  These modules receive repository data rather than a DI container. Multi-statement
  operations retain their transactions, and missing/foreign IDs retain their errors.
- Branching and synthesis orchestration are separate workflow modules. Public
  workspace/synthesis DTOs live in `workspace_dto.rs`, re-exported at the old path.
- Journal synthesis is a durable `journal.synthesis` job
  (`conversation/synthesis.rs`) whose subject is its destination. It saves its
  plan and each part's notes as it goes; after a restart it resumes at the first
  unsaved part, and one that finished while the app was closed waits, staged,
  until the renderer saves it. The renderer (`features/journal/synthesis/`)
  follows it through `jobs://status`; `RootLayout` mounts its progress panel, and
  open editor saves are flushed before the result is appended.
- The renderer sidebar composes conversation lists, references, and a spaces panel.
  Journal selection, space editing, and synthesis have focused hooks. Shared React
  Query reads and mutations own journal/bookmark/note data; component state holds
  drafts, selections, and visibility. ESLint keeps view files from value-importing
  `VaultAPI`, a feature client, the transport or a Tauri plugin; files that
  predate the rule sit in allowlists in `eslint.config.js` that can only shrink.
- CI executes desktop library tests, real audit integration tests, frontend tests,
  a renderer build, and browser smoke tests. The audit suite exercises a rejected
  credential command without accessing the OS keychain, plus sink persistence,
  concurrent delivery, bounded history, and logger enable/disable behavior.

## Background work and error contracts

Detached tasks go through `shared/runtime/background` (`background::spawn`).
Shutdown closes admission, cancels expensive work, and joins final writes before
closing SQLite. The ratchet allows one raw spawn (`infrastructure/setup/app.rs`);
any other deliberate detached task carries a `// raw-spawn: <reason>` marker
(today: file plugin setup and the Learning lab and container child processes).

Durable jobs use `shared/runtime/jobs`. Its `JobStore` is the only code that
reads or writes the `jobs`, `job_events` and `job_checkpoints` tables; a feature
keeps its own facts about a job in a side table keyed by `job_id` (Learning's is
`learning_jobs`). The `JobRuntime` is owned by the container: a feature
registers a handler per kind during plugin setup, which first settles that
kind's jobs left running by the last process (requeue or mark interrupted, per
kind), then delivers saved pending work. Submission is idempotent on the
operation ID; a reused ID with a different payload hash is rejected. A kind may
bound its concurrency and run one job per subject at a time (Learning: one
preparation per course). Cancellation commits the status before signalling the
worker's token, and every worker transition is conditional on the job still
running, so a late result never overwrites it. Temporary outages defer a job
with a growing retry delay instead of failing it; a retry is a new attempt
linked by `retry_of_job_id` that inherits the staged result, activity and
checkpoints. Each transition is published to the renderer as `jobs://status`.
Shutdown closes the runtime and drains it before the database closes.

Kinds today: `learning.lesson_preparation`; `batch.file_import` and
`batch.url_import` (items in `batch_import_items` keyed by job id);
`explorer.folder_index` (keyed by the index root, one build at a time);
`chat.deep_research` (one per conversation, resuming from its last finished
round); and `journal.synthesis`. The renderer reads all of them through one
jobs module (`src/features/jobs`: `list_jobs` plus one `jobs://status`
listener).

`features/learning/lessons/generation_jobs.rs` is Learning's lesson-preparation
handler; it receives a pool, model loader, and source refresh callback instead
of the application container. `practical_runs.rs` owns
runtime execution, cancellation tokens, and workspace cleanup; its repository
owns transactions. Practical workspace reads return a saved-data snapshot.
`practical_workspace.rs` probes runtimes and resolves availability from that
snapshot, including operation replays, without starting processes inside a
repository read. Tests inject capability snapshots for unavailable runtimes,
disabled profiles, and activities pinned to an older runtime. Chat streams use a
typed event sink, with the Tauri window adapter confined to `chat/desktop.rs`. Chat workflows depend on the feature's model, storage, retrieval, and policy
contracts, which the desktop composition adapter implements. The ratchet counts
`Container` imports outside plugin, command, DI, desktop, setup and interfaces
files; the baseline still lists 16 (in compare, conversation branching,
compaction, handoff and synthesis, Explorer folders, Learning packs and sources,
QA starters, reranker setup and vault writeback).

The serializable `DomainError`, `ApplicationError`, and `AppError` contracts live
in `shared/error`. Existing domain/application imports re-export those types.
Driver and LLM conversions live in their respective outer modules, preserving
IPC shapes without making the shared error contract import those layers.

Empty placeholder suites and orphaned test scaffolding have been removed.
The active test inventory is in `src-tauri/tests/README.md`. CI explicitly executes
lifecycle, tag repository, and plugin integration tests; the syntax-aware checker
(`scripts/rust-architecture-check`) rejects empty integration test bodies and Rust
test files outside the Cargo module graph, and checks the shared error contract
for reverse dependencies.

It also ratchets four outer-ring rules against
`scripts/rust-architecture-check/baseline.txt`: infrastructure importing
features, feature-to-feature edges, `Container` imports outside adapter files,
and raw spawns outside `shared/runtime`. A new or rising count fails; so does a
falling one, until `bash scripts/check-rust-layer-boundaries.sh --write-baseline`
records it. That command also rewrites `scripts/tauri-command-orphans.baseline`.
Commit the smaller file; never add lines to get a change through.

## Conversation memory

Bounded conversation memory follows the same layer rules as everything else.
Design and as-built notes: `docs/design/2026-09-19-conversation-memory.md`.

- `domain/conversation/memory.rs` owns the memory value types — item identity and
  kind, validity, evidence spans, proposed changes, allowed transitions — and all
  deterministic validation. An untrusted model proposal becomes a committable
  candidate only by passing rules that live here. The module is pure: no sqlx,
  Tauri, or axum, and no imports from `application`, `features`, or
  `infrastructure`. Calling a model and paging a transcript belong to the
  application layer; SQL belongs to the repository.
- `application/ports/conversation_memory.rs` declares `ConversationMemoryPort`
  (snapshot load, commit, source reads) and `ConversationMemoryReadPort`
  (recall). Application services depend on these traits only, so their tests use
  fakes while the SQL stays behind the repository barrier.
- `application/services/context_assembler/{mod,budget,plan,render,tests}.rs` is
  the single budget owner. `BudgetAllocation::plan` takes the request's fixed
  parts (system policy, tool schemas, current input) and how its history is paid
  for (`HistoryCharge`: reserved memory pools, measured history, or history the
  caller carries whole), and divides the active model's capacity into the output
  reservation, safety margin, memory pools and the evidence pool. Fixed parts
  that do not fit are a typed `PromptBudgetExceeded`, never a clipped
  instruction; mandatory-memory overflow is an explicit
  `ActiveMemoryBudgetExceeded`, never a silent eviction. `EvidenceBudget` hands
  the evidence pool out claim by claim (`EvidenceShare` for attachments, an
  Explorer folder, recalled evidence and web pages) and packs ranked passages
  into what is left. `ContextAssembler::assemble` selects and renders memory,
  history and recall against the same allocation; `plan.rs` returns the typed
  plan and its accounting, and `render.rs` emits typed messages, so memory is
  never spliced into the system prompt as a string.
- `application/services/grounded_generation/` is the one path from gathered
  evidence to an answer for every generator outside chat's tool loop: handoff
  (continue in a new chat), compare, document summaries and every Learning
  generator (through `learning/model_call.rs`) use it today. A
  request carries instructions, a task, evidence passages under stable ids,
  optional history, an output reservation, priority/cancel/cache key and an
  optional `claim_verification` policy. It plans with `BudgetAllocation`,
  selects evidence (`All`, `BestFirst` or `Prefix`), sends one typed call and
  returns the text, the ids it carried, the accounting and any verdicts. Evidence
  the window cannot hold is reported by id; a request whose fixed parts or
  required evidence overflow is refused before any model call.
- `features/conversation/memory/` (`mod`, `job`, `selection`,
  `prompts`, `extract`, `verify`, `summarize`, `segment`) is the only compaction
  implementation, with `mod.rs` as the façade. `CompactionJob` in `job.rs` must be a singleton: it
  holds the per-conversation single-flight slots, so two instances each hold
  their own map and the mutual exclusion means nothing. `CompactionJob::with_slots`
  exists so a caller can share process-wide slots deliberately.
- `features/conversation/repository/{memory,memory_recall,memory_port}.rs` are
  the only places memory SQL lives. `load_memory_snapshot` reads state, items,
  evidence, and summary in one transaction, so no caller can pair revision `N`'s
  summary with revision `N+1`'s ledger. `commit_memory` is one atomic
  transaction: it checks both revisions, re-resolves every evidence span against
  live message content, then applies items, evidence, summary, watermark, and
  revision together. It is the only writer of `conversation_summaries`.
  `memory_recall.rs` scopes retrieval to one conversation — ownership is in the
  `WHERE` clause of every query, and only `source = 'message'` rows are returned,
  so a title or bookmark note cannot be presented as something the user said.
  `memory_port.rs` is thin delegation to those inherent methods.
- `features/conversation/chat/turn/` runs one chat turn as stages with typed
  hand-offs: `prepare` (validate, load, open, read history), `budget` (plan the
  `TurnBudget`), `classify` (intent, then router), `carry` (attachments and the
  Explorer folder, charged to the evidence budget), `retrieve` (recalled
  evidence, then the retrieval pipeline), `assemble` (fit, number and render the
  evidence; choose tools; plan the typed request), `generate` (the tool loop)
  and `finalize` (persist, then start the background grounding check). The turn
  has no budget arithmetic of its own: `TurnBudget` is one `BudgetAllocation`
  planned before retrieval, and the tool loop grows its request only up to that
  allocation's input budget. The stages reach infrastructure only through the
  narrowed `chat/ports.rs` traits (`ChatRecords` for the conversation's own
  rows, `LibrarySearchTrait`, `PageReader`, `ChatTools`), so `turn/tests.rs`
  drives them with a fake runtime. `turn/research.rs` saves a deep-research
  turn's progress so its `chat.deep_research` job resumes after a restart.
- `features/conversation/chat/memory_context.rs` assembles one turn's bounded
  typed input from memory plus recall when the setting is on, and returns
  nothing when it is off; the string history then goes out whole, already charged
  whole by the turn's budget. Even a conversation with no extracted ledger is
  planned, because its raw history may already exceed the window. A plan that
  would drop unprocessed history compacts once, inline, and is planned again;
  if that still does not fit, the turn fails with the user's message saved.
  `chat/history_tools.rs` owns both shapes of recovery: the two read-only tools a
  continuation model may call, and the same retrieval run automatically when the
  plan is assembled.
- `features/conversation/memory_dto.rs` and `memory_details.rs` are the
  details view. `knowledge_dto.rs` and `repository/knowledge.rs` own explicit
  sharing, validity intervals and user corrections. A correction appends an exact
  user-authored source message and supersedes the old item atomically. Quotations are
  resolved from the original messages on every read and never cached beside the
  item, so deleting a message deletes its quotation from this view too.

## Model access and retrieval

- `LLMPort` (`application/ports/llm_port.rs`) is the one model API and takes
  typed requests only (`complete*` with `CompletionRequest`). The string
  `generate*` methods, the engine `LLMClient` trait and the separate sidecar
  client are gone. Responses carry replay items in each provider's own shape.
- The bundled llama-server sidecar and a remote llama.cpp server share one
  client, `LlamaCppLlm` (`features/llm/llama_cpp/`): messages, streaming, retry,
  `/props`, `/tokenize` and slot pinning (`id_slot` with `cache_prompt`). The
  cloud provider (`features/llm/cloud.rs`) adapts OpenAI Responses and Anthropic
  Messages and is used only when selected explicitly.
- Every model call is admitted by one `InferenceScheduler` per backend
  (`features/llm/scheduler/`) by `InferencePriority` (interactive,
  verification, maintenance, background), with one slot kept for interactive
  work and estimated tokens reserved against the window. The per-feature
  limiters (Ollama semaphore, compaction and Learning gates, judge fan-out cap)
  are gone; Learning keeps a per-course lock.
- `HybridSearchUseCase` (`features/search/use_cases/hybrid_search.rs`) is the
  one retrieval orchestrator: branch execution, weighted fusion, the attachment
  exclusion and the cross-encoder rerank (`RerankOptions`). Chat, the tool
  executor, the search page, compare, Learning's `LibraryPassages` and the
  retrieval eval (`examples/retrieval_eval`) all run through it;
  `HybridSearchService` is gone.
- Ollama uses its native `/api/tags` and typed `/api/chat` routes. Its connection test
  no longer accepts a llama.cpp model list as evidence of an Ollama connection.
- The explicit `llamacpp` provider keeps its own URL, model, and authentication
  under `llm.llamaCpp`. Its adapter uses `/v1/chat/completions`, preserves native
  tool-call IDs and replies, and parses streamed UTF-8 without losing split bytes.
- The llama.cpp connection test verifies model discovery and a small generation.
  Chat availability and conversation creation share the renderer's provider
  selection helper so a remote server does not require a local model download.

## Backup archives

- `features/backup/archive` writes encrypted `.lattice-backup` files to a
  user-chosen folder. Design and as-built notes:
  `docs/design/2026-09-16-encrypted-backup-archive.md`.
- Paths for archive destinations and restore sources come from Rust-side
  native dialogs, never from the webview.
- Archives omit derivable tables (embeddings, sparse terms, clusters, chat
  starters). Restore leaves `shared::constants::REEMBED_MARKER_FILE` in the app
  data directory; search startup clears it once vector coverage is complete.
- The schema starts with `20260916000000_init_schema.sql` and evolves through
  dated migrations beside it (see the migration rules below).

## Library blobs

- Imported files are copied into a content-addressed library,
  `~/.lattice/files/{sha256}/{filename}`, and `documents.checksum` is that same
  digest. Design: `docs/design/2026-09-16-library-blob-lifecycle.md`.
- `shared/fs/roots.rs` resolves the two roots kept outside the app data
  directory, `~/.lattice/files` and `~/.lattice/web-archive`; both hold user
  content a backup must carry.
- Ownership rule: Lattice deletes a file from disk only when it owns the file.
  Library blobs are removed by hash through `ContentAddressedStoragePort`,
  never by arbitrary path; web archive articles go through
  `WebArchiveServiceTrait::delete_article`. Anything else — a vault note, a
  user file indexed in place — is left alone and logged.
- `LibraryGc` owns the reference check. `release(hash)` runs after a document
  that referenced the hash is deleted (or after an import fails to commit) and
  removes the blob only when no document references it; `sweep()` runs at
  startup and does the same for every hash on disk.
- An import holds a `BlobLease` from the moment it copies a blob until the
  document row commits. A leased hash is never removed, so an import and a
  concurrent delete of the previous document cannot race into a missing file.
- Backups pack only the blobs the archived database references. The set comes
  from `SELECT DISTINCT checksum FROM documents` read out of the snapshot
  itself, so the tar and the manifest always agree, and orphans never travel.

## Explorer and folder indexes

- `features/explorer` is the `explorer` plugin: a folder on disk beside a
  chat. The folder is read live and never imported into the library. Every
  path resolves through `scope::Scope`, which confines it to the chosen root;
  `fs` lists, reads, searches and finds with size bounds and `.gitignore`
  awareness. Design: `docs/design/2026-10-02-explorer.md`.
- An Explorer thread is an ordinary conversation whose `explorer_root` column
  is set (migration `20261002100000_conversation_explorer_root.sql`);
  `explorer/repository.rs` is the only writer of that column. The thread's
  model gets the folder block from `prompt.rs` and read-only tools confined to
  the root (`list_directory`, `read_file`, `search_files`, `find_files`, and
  `search_folder` from the index). Answers point at lines with the
  `path:10-24` grammar parsed in `line_refs.rs`.
- `explorer/index` keeps a semantic index per folder under
  `<data_dir>/folder-index/`: a `chunks.db` and a vectors file keyed by the
  embedding identity. The index lives outside `lattice.db`, so backups do not
  carry it; it is rebuilt from the folder. A watcher follows edits while the
  folder is open. Builds are `explorer.folder_index` jobs keyed by the index
  root, run one at a time with the open folder first and resumed after a
  restart if the folder still exists; status (passages, percent, rate and
  ETA) reaches the renderer on `jobs://status`. The composition root owns the
  index manager. A
  folder uses its own index when it has one, else the deepest enclosing
  index, limited to its sub-tree. Nothing evicts an index; only the user
  deletes one. Design: `docs/design/2026-10-02-folder-index.md`.
- The folder list is the `explorer_folders` table in `lattice.db`
  (migration `20261002110000_explorer_folders.sql`, SQL in
  `explorer/repository.rs`, assembly in `explorer/folders.rs`). Opening an
  index upserts the folder's row, and listing adopts index directories that
  have no row. Removing a folder with its threads deletes them through the
  conversation plugin's normal delete path. Each row also holds the folder's
  system prompt and space (`20261003100000_explorer_folder_settings.sql`):
  its threads live in that space, General by default, and its prompt stands
  in for the space's.

## Learning Studio and flashcards

- `features/learning` is the `learning` plugin behind the Studio surface:
  programs, curriculum, sources, practice, practical labs, assessment
  evidence, canvas, portability packs and Recall. Its tables come from the
  `learning_*` migrations dated `20260930020000` onward. Design:
  `docs/design/2026-09-30-learning-studio.md`; test evidence:
  `docs/development/learning-studio-verification.md`.
- Every Learning generator — lessons, outlines and their reviews, content
  verification, assessments, practice, practical activities, study decks — sends
  its strict-JSON call through `learning/model_call.rs` into
  `application/services/grounded_generation`, so the context assembler's
  `BudgetAllocation` decides whether a prompt fits and what the answer may use.
  Generators state an output reservation and, where they choose sources before
  writing the prompt, ask the planner for the room left (`source_room`); none
  does window arithmetic of its own. Claim judging stays on the shared
  `claim_verification` service.
- Lab code runs without a host shell and never in the app's process: built-in
  JavaScript (QuickJS) and Python (CPython as a WASI guest on Wasmtime) run in
  the lab runner (`runtime/lab_runner.rs`), a child copy of the app executable
  started with `--lattice-lab-runner`, which `main.rs` hands off before Tauri
  starts. The child gets an empty environment and the run's scratch directory,
  speaks length-prefixed JSON over stdin/stdout, and is killed on cancellation
  or when it outlives its limits; the engines' own limits (fuel, epochs, store
  and QuickJS memory, read-only mounts) still apply inside it. The engines
  live in `runtime/guest/` behind the default-on `learning-labs` cargo feature;
  without it the built-in runtimes report themselves unavailable. The compiled
  interpreter is cached under the OS cache directory
  (`lattice/learning-python/`). Container presets (`runtime_catalog.rs`,
  `lab_runtime.rs`) run fixed argv templates under Docker or Podman.
  Portability packs are stored under `<data_dir>/learning-packs/`.
- Flashcards are part of Learning's Recall (`learning/recall/`); there is no
  `study` plugin. FSRS (`recall/schedule.rs`) is the only scheduler, the
  schedule lives on the `study_cards` row, both review paths share
  `record_review`, and the study commands are on the `learning` plugin.
- Learning's operation replay uses one `learning_operations` ledger.
  Document sources search the library through the `LibraryPassages` port
  (hybrid search scoped to the program's documents); web and pasted sources
  stay keyword-ranked. Learning keeps no vector index of its own.

## Command registration and schema invariants

These are enforced by generators, scripts, and the database rather than by the
Rust compiler, so they are the parts that bite a newcomer.

- Registering a Tauri command takes five separate edits: the
  `#[tauri::command]` function (conversation keeps the bodies as `_impl`
  functions in `plugin_impl.rs`), its `generate_handler!` entry in the
  feature's `plugin.rs`, the command name in `src-tauri/build.rs` under
  `InlinedPlugin::commands`, the matching `<feature>:allow-<command>` permission
  in `capabilities/main.json`, and its entry in `export_bindings`'s
  `collect_commands!` list. Missing any one of the five is not a compile
  error — the ACL rejects the command at runtime. `npm run bindings:generate`
  then writes the TypeScript bindings and `src/shared/ipc/routes.generated.ts`,
  taking each command's plugin from its grant in `capabilities/main.json`; the
  transport accepts only a generated `CommandName`, and hand-copied DTOs in
  `src/types/api` re-export the generated types.
  `compact_conversation` had exactly this defect: registered in the handler and
  present in the generated bindings, absent from `build.rs` and
  `capabilities/main.json`, so `/compact` would have failed in the running app
  while the whole test suite passed. `export_bindings -- --check` and
  `scripts/check-ipc-contracts.mjs` cover the binding side only, which is how
  that omission survived. The ACL side is now covered by
  `scripts/check-tauri-command-inventory.py` (`npm run contracts:commands`),
  which cross-checks every `generate_handler!` entry against `build.rs` and
  `capabilities/main.json` in both directions and runs in CI. It found two more
  live instances of the same defect on its first run — `search:reranker_status`
  and `search:download_reranker` — which is the argument for having it. It
  also ratchets `#[tauri::command]` functions that no handler registers against
  `scripts/tauri-command-orphans.baseline`.
- Two conversation-memory invariants live in SQLite triggers, not in callers.
  `trg_conversation_transcript_revision_{ai,au,ad}` bump
  `conversations.transcript_revision` on every message insert, update, and
  delete. `trg_conversation_memory_invalidate_{au,bd}` and
  `trg_conversation_memory_vectors_invalidate_au` invalidate derived memory when
  source content changes or a message disappears. They are triggers precisely so
  correctness does not depend on a caller remembering: a new write path gets the
  revision bump and the invalidation without knowing they exist.
- Memory concurrency is compare-and-swap on the
  `(transcript_revision, memory_revision)` pair plus operation-id idempotency. A
  commit states the pair it read; a mismatch fails the commit and the run retries
  from a fresh snapshot instead of overwriting a newer ledger. Re-running the
  same operation id does not apply a second time.
- Add each schema change as a new dated migration,
  `src-tauri/migrations/YYYYMMDDHHMMSS_name.sql`. Never edit one that has been
  applied, the squashed init file included: sqlx stores a checksum of every
  applied migration and startup refuses a database whose history changed. A
  column added by `ALTER TABLE` goes only in the new file, never also in the
  init schema, or fresh databases fail. Lattice is pre-release and carries no
  compatibility code for old local data; a development database that predates
  a breaking change is deleted, not repaired. The app never resets a database
  on its own.
- Tests run the real migration through `sqlx::migrate!("./migrations")`. Two
  hand-rolled test schemas existed and had drifted from it, hiding trigger
  behavior and column defaults from exactly the tests that depended on them;
  both were replaced by the migration and the pattern should not return.

## Verification commands

```sh
bash scripts/check-rust-layer-boundaries.sh
bash scripts/check-repository-barrier.sh
python3 scripts/check-tauri-command-inventory.py
python3 scripts/check-sql-contracts.py
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
cargo test --locked --manifest-path scripts/data-contract-check/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --test security_audit_logging_test
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo run --manifest-path src-tauri/Cargo.toml --features bindings-export --bin export_bindings -- --check
cargo test --manifest-path api-rust/Cargo.toml --lib
cargo test --manifest-path api-rust/Cargo.toml --test sync_persistence -- --ignored
cargo test --manifest-path src-tauri/Cargo.toml --test conversation_memory_evals -- --list
```

After removing ratcheted violations, run
`bash scripts/check-rust-layer-boundaries.sh --write-baseline` and commit the
smaller baselines.

The conversation-memory evaluation suite is opt-in and needs a configured model
plus the environment documented at the top of
`src-tauri/tests/conversation_memory_evals.rs`. Authenticated Qwen calibrations
have exercised correction and conflict cases. The corrected-value and
unresolved-conflict cases pass after repairs to same-batch transitions, repair
context, and ambiguous-transition review. A subsequent release run found and
repaired contradictory action probes and a continuation harness that truncated
multi-tool answers after two rounds. The corrected three-repeat corpus-wide
release baseline has not completed and must not be reported as passing.

The `sync_persistence` command requires a disposable PostgreSQL `DATABASE_URL`. SQLx creates
isolated test databases. CI provisions PostgreSQL and runs these tests explicitly;
ordinary local unit tests do not require a database server. Replay tests cover both
accepted operations and conflicts after the document head changes. Reusing an
operation ID with a changed payload rejects the entire transaction.

## Limits of the current architecture

This is not a claim of “10/10” architecture or production readiness.

- Desktop layers are still modules inside one crate. Syntax-aware source checks
  catch direct dependencies, grouped imports, derives, and path aliases, but do
  not expand macros or resolve every indirect re-export. Separate crates would
  provide a stronger compiler-enforced boundary.
- The repository-barrier shell check remains heuristic and intentionally permits
  annotated resource workflows. A separate syntax-aware rule forbids SQL driver
  dependencies in feature plugin entry points and their child modules, including
  grouped and renamed imports. Conversation branching and synthesis use the same
  rule; workspace persistence cannot import the DI container or transport drivers.
  Conversation-memory indexing remains a separate decomposition opportunity.
- Some runtime loaders still use concrete downloaded-model persistence adapters;
  live-model compatibility and performance require model/hardware testing beyond
  unit tests.
- The shared desktop error contract is still broad, and ignored tests
  remain coverage gaps. A green library run does not substitute for live desktop,
  migration, packaging, or inference validation.
- Bounded conversation memory has only limited real-model calibration. The
  deterministic suite covers quote fidelity, commit atomicity, revision
  preconditions, and budget arithmetic. One authenticated Qwen correction case
  and one unresolved-conflict case pass. Extraction quality, recall usefulness,
  multi-cycle drift, and corpus-wide reliability remain unestablished until the
  full repeated baseline completes.
- The sync API is still explicitly a scaffold: one server-configured bearer
  token maps to one tenant. Multi-tenant identity-provider authentication,
  merged conflict application, and outbox delivery still need product and
  security decisions. The desktop application does not call this service yet.

Keep structural refactors distinct from changes to authentication, sync conflict
semantics, delivery guarantees, and user-visible retention policy. Document and
test those decisions before calling the backend production-ready.

## Shared memory lifecycle

Bounded memory defaults on; a saved explicit opt-out remains respected. This
product default is independent of model-quality evaluation results. After an
answer commits, a serialized background maintenance job extracts and reviews
completed turns and updates the existing ledger and working summary. It uses
the utility model when configured, otherwise the conversation model. Failure
leaves the answer and the previous committed memory intact.

`conversation_memory_attributes` attaches explicit conversation, space, or
personal scope and validity/verification dates to existing item IDs. Nothing is
shared automatically. Corrections inherit scope unless the user explicitly
changes it. Deleted source evidence is never replaced with cached quotations.
Forgetting resolves the saved item and records source-span suppression so a
rebuild cannot recreate the same saved assertion. Original transcript messages
remain available through conversation history.

The repository resolves mandatory evidence beyond the recent-message window.
Shared requirements enter the same mandatory token budget; optional facts are
ranked by lexical overlap and compatible source-message embeddings. Transcript
recall fuses lexical, exact-identifier and semantic candidates with reciprocal
rank fusion. Embedding model identity, dimension and live source bytes must
match. Bounded scans report truncation; no retrieval miss proves absence.

`search_saved_knowledge` reads explicitly available memories and optional
correction history within the turn's shared tool budget. The inspector shows
scope, provenance, valid dates and the IDs included in the latest answer's
initial context. User controls use React Query; their writes invalidate both
memory views and conversation messages. The original extraction, repeated
compaction and continuation evaluation remains the model-quality gate, separate
from deterministic repository and budget tests.

## Feature ownership and mutation contracts

Learning Studio groups planning, lessons, references, practice, assessment,
recall, canvas, runtime, and portability under their owning modules. Its public
feature facade remains stable; command adapters are grouped under `plugin/`.
Web-source validation, replay handling, and persistence orchestration live in the
source workflow, with guarded fetching supplied by the command adapter.

The document pool and transaction adapters share SQL operations. Aggregate reads
include chunks and tags; explicit metadata reads omit children. Batch saves have
the same semantics as individual saves, with one transaction around the batch.
Document tag replacement is also one transaction, including missing-tag creation.

Daily-note capture holds a write transaction while selecting or creating a target
and merging the capture. Full edits compare a persisted revision. A database
trigger advances that revision for every writer, including vault imports and
content-only updates. Model path resolution and symlink confinement belong to
the filesystem adapter, and domain checks reject direct environment or filesystem
access outside tests.
