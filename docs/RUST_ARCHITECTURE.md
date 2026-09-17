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

## Verification commands

```sh
bash scripts/check-rust-layer-boundaries.sh
bash scripts/check-repository-barrier.sh
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --test security_audit_logging_test
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings -- --check
cargo test --manifest-path api-rust/Cargo.toml --lib
cargo test --manifest-path api-rust/Cargo.toml --test sync_persistence -- --ignored
```

The last command requires a disposable PostgreSQL `DATABASE_URL`. SQLx creates
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
- The API is still explicitly a scaffold: trusted-header authentication, merged
  conflict application, and outbox delivery need product/security decisions.
  This refactor does not silently implement or change those protocols.

Keep structural refactors distinct from changes to authentication, sync conflict
semantics, delivery guarantees, and user-visible retention policy. Document and
test those decisions before calling the backend production-ready.
