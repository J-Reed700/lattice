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
- Files over ~700 lines are split into directory modules with a thin façade
  file that keeps the public path stable.
- The crate has no blanket `dead_code`, `unused_imports`, `unused_variables`,
  or `deprecated` allowances. Unread fields that must exist (guards,
  `FromRow` columns, serialized DTO fields) carry a targeted `#[allow]` with a
  reason comment.
- Ignored tests always carry a reason string; the remaining ignores are
  environment-gated (OS keyring, network, downloaded models, Ollama).

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
  `repository/workspace/{journals,spaces,memberships,sources,state,bookmarks,explorer}`.
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

## Conversation memory

Bounded conversation memory follows the same layer rules as everything else.
Design and as-built notes: `docs/design/2026-09-19-conversation-memory.md`.

- `domain/conversation_memory.rs` owns the memory value types — item identity and
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
  read-only details view. There is no edit-memory shape, because a free-form
  editor creates requirements with no source behind them. Quotations are
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
- The schema is a single migration, `src-tauri/migrations/20260916000000_init_schema.sql`.
  There is no legacy data to migrate: change that file directly and delete
  local databases when the schema changes.

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

## Command registration and schema invariants

These are enforced by generators, scripts, and the database rather than by the
Rust compiler, so they are the parts that bite a newcomer.

- Registering a Tauri command takes five separate edits: the `_impl` in the
  feature's `plugin_impl.rs`, the handler and `invoke_handler` entry in
  `plugin.rs`, the command name in `src-tauri/build.rs` under
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
  delete. `trg_conversation_memory_invalidate_{au,ad}` and
  `trg_conversation_memory_vectors_invalidate_au` invalidate derived memory when
  source content changes or a message disappears. They are triggers precisely so
  correctness does not depend on a caller remembering: a new write path gets the
  revision bump and the invalidation without knowing they exist.
- Memory concurrency is compare-and-swap on the
  `(transcript_revision, memory_revision)` pair plus operation-id idempotency. A
  commit states the pair it read; a mismatch fails the commit and the run retries
  from a fresh snapshot instead of overwriting a newer ledger. Re-running the
  same operation id does not apply a second time.
- Schema changes go directly into
  `src-tauri/migrations/20260916000000_init_schema.sql`. There is no migration
  chain and no compatibility shim, so a schema change means deleting local
  databases.
- Tests run the real migration through `sqlx::migrate!("./migrations")`. Two
  hand-rolled test schemas existed and had drifted from it, hiding trigger
  behavior and column defaults from exactly the tests that depended on them;
  both were replaced by the migration and the pattern should not return.

## Verification commands

```sh
bash scripts/check-rust-layer-boundaries.sh
bash scripts/check-repository-barrier.sh
python3 scripts/check-tauri-command-inventory.py
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --test security_audit_logging_test
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings -- --check
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
ordinary local unit tests do not require a database server.

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
- The shared desktop error layer remains a coupling point, and ignored tests
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
