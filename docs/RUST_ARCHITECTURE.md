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
  engine, repositories, services, commands, plugin, and DI registrar. There
  are no `#[path]` aliases: every module lives at its canonical Rust location
  and has exactly one path. The former `infrastructure::{indexing,llm,qa,
  search,web}` and `infrastructure::services::{conversation_service,
  search_enrichment_service,hyde}` aliases are gone; consumers import
  `crate::features::<name>::engine` and friends directly.
- Domain values that the application layer depends on (search value objects
  and results, conversation aggregates, download sessions and snapshots, QA
  interpretation types, model download events) live in `domain/`, not in a
  feature, so the layer rules above remain checkable.
- `interfaces/di/container.rs` holds only construction and runtime state.
  Each feature exposes its accessors through an `impl Container` registrar
  block in its own `di.rs`, so the accessor surface is owned by the feature.
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
- The renderer sidebar composes conversation lists, references, and a spaces panel.
  Journal selection, space editing, and synthesis have focused hooks. Shared React
  Query reads and mutations own journal/bookmark/note data; component state holds
  drafts, selections, and visibility. View files cannot import the IPC transport.
- CI executes desktop library tests, real audit integration tests, frontend tests,
  a renderer build, and browser smoke tests. The audit suite exercises a rejected
  credential command without accessing the OS keychain, plus sink persistence,
  concurrent delivery, bounded history, and logger enable/disable behavior.

## Background work and error contracts

Conversation vector indexing, maintenance compaction, lesson generation, and
practical runs use `shared/runtime/background`. Shutdown closes admission,
cancels expensive work, and joins final writes before closing SQLite. Queued
lesson jobs remain pending for startup recovery; interrupted running jobs retain
a terminal record and can be retried. Cancellation of a lesson job commits its
status before signalling its registered token. A shared generation slot bounds
concurrent lesson inference.

`features/learning/generation_jobs.rs` receives a pool, model loader, and source
refresh callback instead of the application container. `practical_runs.rs` owns
runtime execution, cancellation tokens, and workspace cleanup; its repository
owns transactions. Practical workspace reads return a saved-data snapshot.
`practical_workspace.rs` probes runtimes and resolves availability from that
snapshot, including operation replays, without starting processes inside a
repository read. Tests inject capability snapshots for unavailable runtimes,
disabled profiles, and activities pinned to an older runtime. Chat streams use a
typed event sink, with the Tauri window adapter confined to `chat/desktop.rs`. The turn workflow still uses the container
for several collaborators and remains a candidate for further decomposition.

The serializable `DomainError`, `ApplicationError`, and `AppError` contracts live
in `shared/error`. Existing domain/application imports re-export those types.
Driver and LLM conversions live in their respective outer modules, preserving
IPC shapes without making the shared error contract import those layers.

Empty placeholder suites and orphaned test scaffolding have been removed.
The active test inventory is in `src-tauri/tests/README.md`. CI explicitly executes
lifecycle, tag repository, and plugin integration tests; the syntax-aware checker
rejects empty integration test bodies, Rust test files outside the Cargo module
graph, and detached spawns in supervised workflows. It also checks the shared
error contract for reverse dependencies.

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
  the single budget owner. `BudgetAllocation::plan` derives every pool from the
  active model's capacity, and it is now shared by the chat path and the QA path;
  the QA path previously carried its own percentage constants. Mandatory-memory
  overflow is an explicit `ActiveMemoryBudgetExceeded` carrying required and
  available counts, never a silent eviction. `plan.rs` returns the typed plan and
  its token accounting; `render.rs` emits typed messages, so memory is never
  spliced into the system prompt as a string.
- `application/services/conversation_memory/` (`mod`, `job`, `selection`,
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
- `features/conversation/chat/memory_context.rs` assembles one turn's bounded
  typed input from memory plus recall, and returns nothing when the setting is
  off or no memory has been extracted, so short conversations pay for none of it.
  `chat/history_tools.rs` owns both shapes of recovery: the two read-only tools a
  continuation model may call, and the same retrieval run automatically for
  providers and QA paths that have no tools.
- `features/conversation/memory_dto.rs` and `memory_details.rs` are the
  details view. `knowledge_dto.rs` and `repository/knowledge.rs` own explicit
  sharing, validity intervals and user corrections. A correction appends an exact
  user-authored source message and supersedes the old item atomically. Quotations are
  resolved from the original messages on every read and never cached beside the
  item, so deleting a message deletes its quotation from this view too.

## Remote chat providers

- Ollama uses its native `/api/tags` and `/api/chat` routes. Its connection test
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
  folder is open, and status reaches the renderer on the
  `explorer-index://status` event (passages, percent, rate and ETA). A
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
- Lab code runs without a host shell: embedded language providers
  (`embedded_runtime.rs`; CPython as a WASI guest in `python_runtime.rs`) or
  fixed container presets (`runtime_catalog.rs`, `lab_runtime.rs`). Portability packs are stored under
  `<data_dir>/learning-packs/`.
- Flashcards are the `study` plugin (`features/study`, `study_*` tables). It
  has no surface of its own; the renderer shows decks inside Learning Studio,
  and Recall schedules its cards through `study_cards` and
  `study::schedule`.

## Command registration and schema invariants

These are enforced by generators, scripts, and the database rather than by the
Rust compiler, so they are the parts that bite a newcomer.

- Registering a Tauri command takes five separate edits: the
  `#[tauri::command]` function (conversation keeps the bodies as `_impl`
  functions in `plugin_impl.rs`), its `generate_handler!` entry in the
  feature's `plugin.rs`, the command name in `src-tauri/build.rs` under
  `InlinedPlugin::commands`, the matching `<feature>:allow-<command>` permission
  in `capabilities/main.json`, and the specta/TypeScript binding. Missing any one
  of the five is not a compile error — the ACL rejects the command at runtime.
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
  and `search:download_reranker` — which is the argument for having it.
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
- The API is still explicitly a scaffold: trusted-header authentication, merged
  conflict application, and outbox delivery need product/security decisions.
  This refactor does not silently implement or change those protocols.

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
