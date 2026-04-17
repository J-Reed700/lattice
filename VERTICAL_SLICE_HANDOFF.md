# Vertical-Slice Migration — Handoff for Next Agent

**Branch:** `Rearchitecture`
**Last commit:** `26edb46b refactor: migrate embedding feature to vertical slice`
**Status:** 25 features migrated, `cargo check` green. 5 features remain.

---

## 1. Read this first — the agreed architectural direction

This codebase is mid-migration from **DDD horizontal layers**
(`domain/` → `application/` → `infrastructure/` → `interfaces/`) to
**vertical feature slices** (`features/<name>/`).

That decision was made after two Gemini oracle consultations. The
workspace-split / 6-crate approach was **explicitly rejected** as
over-engineered for a solo + AI-assisted project. The final direction
is a single crate, feature-per-folder, with:

- **Shared ports** (`EmbeddingPort`, `LLMPort`, `VectorSearchPort`,
  etc.) stay in `application/ports/`. Features *consume* ports, they
  don't *own* them.
- **Cross-feature domain primitives** (e.g. `embedding_constants`,
  `FileMetadata` value object) stay in `domain/`.
- **App-bootstrap code** (`infrastructure/setup/*`) stays put.
- **Everything else that is feature-private** moves into
  `features/<name>/`.

DO NOT try to dismantle the DI container (`interfaces/di/container.rs`
at 1980 LOC + `modules.rs` at 2364 LOC). DO NOT convert `Arc<dyn Trait>`
ports into concrete imports. Those battles are lost on purpose — per
oracle, "in-place file relocation + `use` path updates only."

Project memory (`/Users/joshreed/.claude/projects/-Users-joshreed-Code-Recall/memory/`)
contains the long-form rationale in `project_architecture_direction.md`.

---

## 2. The migration recipe (Strangler Fig)

For each feature:

1. **Make `features/<name>/` with the canonical shape:**
   ```
   features/<name>/
     mod.rs           (navigational only, see rule below)
     dto.rs           (if feature has a DTO)
     commands.rs      (if feature has Tauri commands)
     plugin.rs        (if feature has a plugin)
     mod.rs           (top-level)
     use_cases/       (if there are multiple use cases)
       mod.rs
       <name>.rs ...
     <other>.rs       (adapter.rs, repository.rs, entity.rs,
                      mapper.rs, trait_def.rs, mocks.rs,
                      service.rs, etc. — whatever the feature needs)
   ```
2. **`git mv` the files** into the new location. Drop redundant
   suffixes from filenames (`_use_case.rs` → `.rs`, `_service.rs`
   → `.rs`) where it doesn't collide with siblings.
3. **Update the use_cases/mod.rs** — it was moved from the old
   location and will have `pub mod old_name;` declarations for files
   you just renamed. Rewrite it to point at the new filenames.
4. **Add `#[path]` redirects** at *every* registration site so legacy
   import paths resolve unchanged. Sites typically include:
   - `application/dtos/mod.rs` for the DTO
   - `application/use_cases/mod.rs` for the use_cases subdir
   - `application/mappers/mod.rs` for application mapper
   - `application/ports/mod.rs` if a port moves (rare — usually stays)
   - `domain/entities/mod.rs` for a domain entity
   - `domain/mod.rs` for a domain module (from `domain/modules/`)
   - `domain/events/mod.rs` for event types
   - `domain/repositories/mod.rs` for repository traits
   - `infrastructure/mod.rs` for top-level infra subdirs
   - `infrastructure/<subdir>/mod.rs` for specific infra modules
     (ml/, persistence/mappers/, persistence/repositories/,
     services/, services/traits/, services/mocks/, events/,
     sagas/, security/, observability/, extraction/ …)
   - `interfaces/commands/mod.rs` for commands
   - `plugins/mod.rs` for the plugin
5. **Create `features/<name>/mod.rs`** as a **navigational document**,
   not a module tree. It should contain only doc comments listing
   where each file lives canonically. **Do not declare `pub mod`**
   for files that already have canonical paths elsewhere — that
   would load each file at two module paths and duplicate types.
6. **Register the feature in `features/mod.rs`** with `pub mod <name>;`.
7. **Run `cargo check`** from `src/app/src/` — must be green before
   you commit.
8. **Commit** with message `refactor: migrate <name> feature to
   vertical slice`. One feature per commit.

### Critical rules (learned the hard way)

- **One canonical module path per file.** If a file lives at
  `features/X/foo.rs` and is loaded as `crate::application::use_cases::X`
  via Strangler Fig, DO NOT also declare `pub mod foo;` in
  `features/X/mod.rs`. Rust will load it twice under different paths
  and silently duplicate every type. The build will succeed but
  behavior will be subtly wrong.
- **Path tags in re-hosted `mod.rs` files resolve from the physical
  location.** If `features/X/use_cases/mod.rs` declares `pub mod foo;`,
  Rust looks for `features/X/use_cases/foo.rs` — not the old path.
  That's why step 3 always needs to rewrite the use_cases mod.rs.
- **Always use `git mv`**, not `mv` + `git add`. Git rename tracking
  matters for history. The one exception is the gitignored `models/`
  directory (see section 4).
- **Rename dropping `_use_case.rs` / `_service.rs` suffixes is optional
  but recommended.** When you do, update the submodule declarations in
  the corresponding `mod.rs` to match new file names.
- **Narrow scope on "shared" pieces.** If a file is consumed by
  multiple features (e.g. `link_parser.rs` used by extraction AND
  mentions), leave it in `infrastructure/` — it's shared infra, not
  feature-private. The feature's `mod.rs` doc block should call this
  out explicitly.
- **Test the build with `cargo check` only.** There is a pre-existing
  `auth::Permission: Serialize` error in `cargo check --tests` that
  was already there before this migration started. Don't try to fix
  it — it's unrelated and out of scope.

### Where to find worked examples

Good templates to copy:

- **Simple feature, single-file plugin:** `features/updates/` (commit
  `0e94198a`)
- **Directory plugin pattern:** `features/health/` (commit `36b53ee0`)
  or `features/credentials/` (commit `8fa09142`)
- **Feature with a domain entity:** `features/mentions/` (commit
  `7d16ebb3`) or `features/tags/` (commit `817bfd26`)
- **Feature with multiple services / complex shape:** `features/qa/`
  (commit `851af1bc`) or `features/embedding/` (commit `26edb46b`)
- **Feature with nested subdirectories:** `features/web/` (commit
  `6c26b233`) or `features/batch/` (commit `aed09a6d`)

Each feature's `mod.rs` contains a table mapping physical files to
canonical module paths — a reference for what Strangler Fig redirects
were needed.

---

## 3. What remains — 5 features

**All remaining features are "core" and heavily cross-consumed.**
They are harder than anything that's already been migrated. Take
your time, don't batch, commit each one separately.

Counts below include any file matching the feature name in its path,
and are an upper bound (some files may end up staying in shared
infra). Actual move counts will be lower after you narrow scope.

### 3.1 `search` — 59 files, highest complexity

Directories involved:
- `application/use_cases/search/` (5 files)
- `application/dtos/modules/search_dto.rs`
- `application/mappers/search_mapper.rs`
- `application/ports/{text_search,vector_search}_port.rs` — **STAY**
  (shared: `VectorSearchPort` consumed by qa, mentions, indexing)
- `di/providers/search_provider.rs` — probably stays in DI
- `domain/entities/search_result.rs`
- `domain/repositories/search_repository.rs`
- `domain/services/search_ranking_service.rs`
- `domain/value_objects/{search_mode,search_query}.rs`
- `infrastructure/persistence/repositories/search/` (4 files,
  tx-wrapper pattern — same shape as embedding/repository_tx/)
- `infrastructure/search/` — this is the meat:
  - `hybrid/` (4 files)
  - `modules/` (12 files — bm25, index, reranker, fusion, snippet,
    vector_ops, recency, file_search, profiler, builder, service,
    service_tests)
  - `query_expansion/` (dictionaries/ + modules/ + mod.rs — 7 files)
  - `strategies/` (2 files)
  - `text_search/` (4 files — bm25, file_search, sqlite_text_search)
  - `vector_search/` (2 files — usearch_index.rs is core)
- `infrastructure/services/{traits,mocks}/search.rs` + `mock_search.rs`
- `infrastructure/services/domains/search_enrichment_service.rs`
- `interfaces/commands/domains/search_commands.rs`
- `plugins/search/` (directory plugin, 2 files)
- `tests/plugins/search.rs` (integration test)

**Proposed shape** (consult oracle before starting):

```
features/search/
  mod.rs, dto.rs, commands.rs, plugin/, mapper.rs
  entity.rs                          (search_result)
  ranking_service.rs                 (domain service)
  repository.rs                      (domain trait)
  repository_tx/                     (4 tx files)
  value_objects/{mode, query}.rs
  use_cases/ (5 files)
  hybrid/ (4 files)
  modules/ (12 files from search/modules/)
  query_expansion/ (7 files)
  strategies/ (2 files)
  text_search/ (4 files)
  vector_search/ (usearch_index.rs)
  enrichment_service.rs              (was services/domains/)
  trait_def.rs, mocks.rs
```

**Things to watch for:**
- `VectorSearchPort` is the most cross-consumed port in the app
  (QA + mentions + indexing). Leave it in `application/ports/`.
- `vector_search/usearch_index.rs` is the beating heart of retrieval
  (see project memory on USearch migration, Feb 2026). A wrong
  `#[path]` here and search silently breaks.
- `search_ranking_service.rs` is in `domain/services/` — first domain
  service we'd move. Check it has no import-in violations before moving.
- Consider asking: is this one feature, or should `query_expansion/`
  and `hybrid/` be their own sub-features? Oracle will have opinions.

**Estimated effort:** half a day. Do NOT rush this.

### 3.2 `indexing` — 54 files

Directories:
- `application/use_cases/indexing/` (6 files)
- `application/dtos/modules/indexing_dto.rs`
- `application/mappers/indexing_mapper.rs`
- `domain/value_objects/indexing_outcome.rs`
- `infrastructure/indexing/` — massive:
  - `extraction/` (12 files: csv, docx, html, mime, odt, pdf, pptx,
    rtf, streaming, text, types, xlsx, mod)
  - `modules/` (11 files: actor, builder, chunker, error, events,
    metadata_extractor, progress, queue, state, chunker_test,
    indexer_tests)
  - `storage/` (7 files: checksum, chunks, context, documents,
    stats (already moved), types, mod)
  - `tests/` (3 files)
  - `transaction.rs`, `mod.rs`
- `infrastructure/persistence/database/performance_indexes.rs`
- `infrastructure/persistence/helpers/indexed_directories.rs`
- `infrastructure/services/{traits,mocks}/indexing.rs` + `mock_indexing.rs`
- `interfaces/commands/domains/indexing_commands.rs`
- No dedicated plugin — indexing commands flow through other plugins

**Things to watch for:**
- `infrastructure/indexing/extraction/` is where content extractors
  actually live for indexing — DIFFERENT from `infrastructure/extraction/`
  (which was mostly kept as shared during the extraction migration).
  Likely all 12 files move with indexing.
- No `IndexingPort` — it's a service trait pattern. Check
  `infrastructure/services/traits/indexing.rs`.
- `performance_indexes.rs` is DB performance config — might be
  shared infra, verify before moving.
- `indexed_directories.rs` helper is small but used across features.

**Estimated effort:** half a day.

### 3.3 `conversation` — 46 files

Directories:
- 3 DTOs (`conversation_dto.rs`, `conversation_message_bookmark_dto.rs`,
  `conversation_space_dto.rs`)
- `application/mappers/conversation_mapper.rs` (application-level)
- `infrastructure/persistence/mappers/conversation_mapper.rs`
  (persistence-level — two mappers, like tags)
- `application/ports/conversation_repository_port.rs` — **STAYS**
- `application/services/conversation_summarizer.rs`
- `application/use_cases/conversation/` (6 files)
- `domain/modules/{conversation, conversation_summary}.rs`
- `infrastructure/events/conversation_events.rs`
- `infrastructure/persistence/repositories/conversation_repository.rs`
- `infrastructure/sagas/conversation_summary_saga.rs`
- `infrastructure/services/domains/conversation_service.rs`
- `infrastructure/services/{traits,mocks}/conversation.rs` +
  `mock_conversation.rs`
- `interfaces/commands/conversation_chat.rs` (top file)
- `interfaces/commands/conversation_chat/` (directory — 18 files)
  including the giant `retrieval/` subdir (13 files) with
  `kb_retrieval`, `pipeline`, `policy`, `rerank`, etc.
- `interfaces/commands/domains/{conversation, conversation_plugin_impl}.rs`
- `plugins/domains/conversation_plugin.rs`

**Things to watch for:**
- The `conversation_chat/retrieval/` subdirectory is where RAG
  happens inside conversations. It imports heavily from the qa and
  search features. Proceed carefully.
- Conversation has its own *saga* pattern (`conversation_summary_saga`)
  — similar structure to the download saga we migrated.
- The retrieval pipeline (`kb_retrieval`, `pipeline`, `overlap`, etc.)
  has 13 files of retrieval-specific logic. Inside the already-modified
  files (see `git status` pre-session showed modifications to these),
  so merge with care.

**Estimated effort:** half a day.

### 3.4 `llm` — 33 files

Directories:
- `application/dtos/modules/llm_dto.rs`
- `application/ports/llm_port.rs` — **STAYS** (heavily shared)
- `application/use_cases/llm/` (9 files)
- `infrastructure/llm/` — split into `inference/`, `modules/`, `system/`:
  - `inference/` (4 files: config, engine, loader, mod)
  - `modules/` (9 files: circuit_breaker, factory, local_client,
    model_catalog_adapter, model_storage_adapter, models,
    noop_client, ollama_client, traits, types)
  - `system/` (4 files: capabilities, gpu, mod, platform)
- `infrastructure/cache/llm_cache.rs` — this is the orphan stub,
  leave alone (real one was moved to features/cache/)
- `interfaces/commands/domains/llm.rs`
- **No plugin** — LLM commands flow through model plugin

**Things to watch for:**
- **LLM ↔ model_management ↔ download are deeply entangled.** Use
  cases like `DownloadModelUseCase` live in `application/use_cases/llm/`
  but call into the `features/download/` feature we already moved.
  This is fine — downstream imports still resolve through Strangler Fig.
- `modules/factory.rs` and the `HuggingFaceAdapter` (which stayed in
  `infrastructure/` during huggingface migration) tie together here.
- `modules/model_catalog_adapter.rs` uses
  `infrastructure/huggingface_adapter.rs` — might graduate together.
- `inference/` has the mistral.rs local inference engine. Heavy deps,
  don't touch internals.

**Estimated effort:** ~4 hours.

### 3.5 `model_management` — 21 files

Directories:
- `application/use_cases/model_management/` (11 files)
- `domain/modules/model_management.rs`
- `infrastructure/persistence/repositories/model/` (4 files — tx wrapper)
- `interfaces/commands/domains/{model_management, model_management_commands}.rs`
- `plugins/model/` (directory plugin, 2 files)

**Plus adopt from LLM migration:**
- `infrastructure/huggingface_adapter.rs` (ModelCatalogPort impl —
  deliberately left alone during huggingface migration; should land
  here).
- `infrastructure/model_cache_adapter.rs`
- `infrastructure/model_catalog_cache.rs`
- (possibly) parts of `infrastructure/llm/modules/model_catalog_adapter.rs`
  depending on what stays with llm.

**Things to watch for:**
- model_management and llm are tightly coupled; consider migrating
  in the same session even though separate commits.
- Check which model_management use cases call LLM use cases and
  vice versa before scoping.

**Estimated effort:** ~3 hours.

---

## 4. Known pre-existing issues (don't fix, just know)

### 4.1 Gitignored `models/` directory

`src/app/src/src/models/` contains `mod.rs` + `tag.rs` (a ~25-line DDD
migration shim that re-exports `crate::domain::entities::tag::Tag` as
`crate::models::tag::Tag`). These files are referenced from
`lib.rs:551` (`pub mod models;`) and imported from ~5 other files,
but they are **gitignored** by `.gitignore:62` (`models/`) — the rule
was meant for ML model binaries and is accidentally catching this
source dir.

The shim is the tail end of an abandoned/incomplete DDD migration
from before this vertical-slice work started. It's orthogonal to our
migration. Leave it alone.

### 4.2 Pre-existing test error

`cargo check --tests` fails with:

```
error[E0277]: the trait bound `auth::Permission: serde::Serialize` is not satisfied
  --> src/interfaces/commands/domains/command_tests.rs:283
```

This was confirmed present **before** any migration work started
(verified with `git stash && cargo check --tests` on the Phase 0
commit). Not caused by our changes. Out of scope for this migration.

### 4.3 Orphan placeholder stubs

Scattered around the codebase are migration-placeholder files with
content like "Placeholder - implementation will be migrated in Phase
3." Some are registered in a mod.rs, some are not. Don't delete them
proactively during feature migration — that's a separate cleanup pass.
Known ones found so far:

- `infrastructure/cache/llm_cache.rs` (not registered)
- `infrastructure/web/modules/{content_extractor,metadata,web_fetcher}.rs`
  (registered but trivial)
- `infrastructure/audit/legacy/audit_logger.rs` (disappeared during
  tags migration via `git add -A`; was never registered)
- `infrastructure/ml/{tokenizer, model_manager}.rs` (registered but
  unused externally)
- `infrastructure/services/embedding_tests.rs` (not registered)
- `infrastructure/web/ingestion/` — entire 8-file directory is
  orphaned (moved to `features/web/ingestion/` for reference only,
  nothing loads it).

### 4.4 Tauri-generated schema files

Three files show as persistently dirty in `git status`:
- `src/app/src/gen/schemas/acl-manifests.json`
- `src/app/src/gen/schemas/desktop-schema.json`
- `src/app/src/gen/schemas/macOS-schema.json`

These are regenerated by the Tauri build and aren't source. They
drift with each `cargo check` but are not part of any commit. Leave
them alone.

---

## 5. Verification after each commit

1. `cargo check` from `src/app/src/` must pass (~1m 20s incremental).
2. Optionally `cargo check --tests` — should show only the known
   pre-existing `auth::Permission` error. Any new error is a
   regression you introduced; fix before committing.
3. `git log --stat HEAD~1..HEAD` — file renames should show as
   `rename ... (100%)`. Anything under 100% means you renamed a file
   AND modified it in the same commit, which is fine but worth
   noticing.
4. Commit message format: `refactor: migrate <name> feature to
   vertical slice` with a body explaining what moved and what
   deliberately stayed.

---

## 6. Existing tracked work

Commits on `Rearchitecture` branch since `master` (27 total):

| SHA | What |
|---|---|
| `ccbeb268` | Phase 0 path flatten (807 renames — `src/app/src/src/crates/recall/*` → `src/app/src/src/*`, Cargo.toml path= removed) |
| `0e94198a` | Scaffold `features/`, `core/`; updates feature (7 files) |
| `851af1bc` | qa (21 files) |
| `a7022c32` | backup (12 files) |
| `7d1736c0` | metrics (5 files) |
| `91ecc60e` | favorites (10 files) |
| `4bba1fda` | recent (8 files) |
| `7d16ebb3` | mentions (17 files) |
| `817bfd26` | tags (22 files) |
| `a9e28f52` | cache (10 files) |
| `36b53ee0` | health (7 files, directory plugin) |
| `cffc4252` | stats (3 files) |
| `8fa09142` | credentials (11 files, directory plugin) |
| `7d0dafae` | initialization (6 files) |
| `aed09a6d` | batch (19 files, parallel services) |
| `75e64c8b` | daily_notes (2 files) |
| `52ce8ed9` | custom_model (14 files) |
| `240c773f` | settings (11 files, directory DTO) |
| `a0da0dcd` | function_calling (11 files) |
| `2ce9c4c6` | file (13 files) |
| `6c26b233` | web (30 files, largest so far) |
| `8ed9369a` | huggingface (2 files) |
| `62089565` | download (13 files, spans domain/infra/events) |
| `10b05c9a` | extraction (9 files, narrow scope) |
| `4a68ba7d` | config (3 files) |
| `26edb46b` | embedding (23 files, tx wrapper) |

Before starting a new feature: `cargo check` from `src/app/src/`
should succeed in ~30s-1m (incremental). If not, something's off
with the working tree — investigate before proceeding.

---

## 7. When to stop and consult Gemini oracle

Oracle history is in memory; use it for judgment calls on:

- Scope questions ("does file X belong to this feature or stay
  shared?")
- Shape questions ("should this be one feature or split into
  sub-features?")
- Unusual patterns not seen yet (you'll encounter these in search
  and indexing)

Key previous oracle rulings:
1. Don't split into a workspace of multiple crates.
2. Don't dismantle the DI container during migration.
3. Strangler Fig via `#[path]` is the right pattern.
4. Shared ports stay shared; features consume, don't own.
5. One canonical module path per file, always.

Oracle consulted: `oracle ask "<prompt>"` (requires
`VERTEX_API_KEY` env var). Use `oracle ask --files path1,path2,...`
to attach files.

---

## 8. Final thought

This is a long migration and you are the second agent on it. The
pattern has been battle-tested across 25 features of every shape.
Every feature commit built green on first try. Follow the recipe,
don't skip the narrowing-scope question, commit per-feature, and
you'll be fine.

The remaining 5 features are genuinely harder than anything done
so far. Budget more time than the previous pattern suggests, ask
oracle when in doubt, and don't batch — one feature per commit,
always.

Good luck.
