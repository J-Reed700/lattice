# Architecture audit, 2026-10-09

Scope: the whole tree at `codex/repair-ci-checks` (e2617393 plus a 15-file
uncommitted diff): `src-tauri/src` (≈379k lines, one crate), `src/` (≈125k
lines), CI, and `api-rust/`. Four read-only reviewers covered Rust structure,
the AI pipeline, Learning Studio plus the data model, and the renderer. Each
claim marked [verified] was read in the code by a reviewer. The six claims
this document leans on most were re-checked by hand (see §7). Counts come
from grep and are lower bounds: they don't expand macros or follow `super::`
paths.

This is a structural audit. For bug-level findings see
`2026-09-25-application-audit.md`; for the AI-defaults scorecard see
`2026-09-24-state-of-the-app-and-roadmap.md`.

## 1. Verdict

The design is sound and the core is clean. The app has outgrown how it is
put together.

- **The core is clean, and checked.** `domain/`, `application/` and `shared/`
  depend only inward; nothing in them imports a feature. A syntax-aware checker
  enforces this in CI. This core (≈44k lines) could become its own crate today.
- **The outer layer is tangled.**
  - 18 of the 37 features form one dependency cycle: 81 feature-to-feature
    edges, 364 references.
  - They are wired together by a `Container` that business code reaches into
    directly, plus 59 process-global statics. About 14 of those hold live
    state that shutdown cannot drain.
  - None of this is checked.
- **The dominant debt is duplication, not layering.** Each feature built its
  own infrastructure. The app now has:

  | Mechanism | How many |
  |---|---|
  | job/progress/resume mechanisms | ≈19 |
  | places that call the model | ≈13 (only chat uses the full stack) |
  | vector stores | 5 |
  | library search orchestrators | 2 |
  | generations of the LLM API, kept alive by 19 fallback branches | 2 |
  | renderer paths to the backend | 5 |
  | idempotency ledgers inside Learning alone | 13 |

  Fixing anything in one of them (token counting, cancellation, retries,
  KV-cache reuse, verification) has to be repeated N times, or it only lands
  in chat.
- **Learning Studio is a second application inside the first.**
  - 59k Rust lines written in about ten days.
  - 91 of the app's 378 registered commands.
  - 77 of about 140 tables.
  - Its own retrieval stack, job runtime and code-execution runtimes.

  Nothing imports it, which makes it the easiest feature to carve out.
- **Quality is still measured on the wrong path.** The retrieval eval drives
  `HybridSearchService`, but chat retrieves through `HybridSearchUseCase`. The
  v1–v3 scores and the "reranker gives no gain" conclusion describe a path
  chat never takes. No quality eval runs in CI.

The next phase should be **consolidation, not more layering**:
1. a few shared runtimes: jobs, model admission, retrieval, grounded
   generation;
2. a ratchet on the outer ring, so the duplication stops growing while it is
   paid down.

## 2. Scorecard, with changes since 09-24

| Area | State today | Since 09-24 |
|---|---|---|
| Inner layers (domain/application/shared) | Clean, enforced in CI | Better: syntax-aware checker, shared error contract isolated |
| Feature boundaries | 18-feature cycle, unchecked; `Container` used as a service locator; 59 statics | New finding |
| Composition | Three places: `interfaces/di`, 28 feature registrars, ≈440 lines in `setup/app.rs`. 10 features have no registrar and build repositories per call (≈180 pool pulls) | New finding |
| IPC surface | 397 command functions (378 registered, 19 orphans); 3 error shapes (`ApiError` 329, `AppError` 41, `String` 28); 5-edit registration covered by a checker | Inventory checker added |
| Background work | `shared/runtime/background` is right, but checked for only 8 named files; ≈16 raw `tokio::spawn` remain, incl. batch import and backup scheduler | Better, not finished |
| Turn pipeline | One ≈1,000-line `run_turn`; the chat runtime port exposes the raw pool and concrete services | Unchanged |
| Model access | One port with two API generations; no scheduler; 5 per-feature limiters; 3 cancellation mechanisms | New finding |
| Sidecar tool calling | **On** for catalog models (`factory.rs:291`, `curated.rs:47`) | Fixed |
| Grounding verifier | 3 windows per source, `Unverified` state, 90 s judge, reused by Learning | Improved |
| Tokens / KV reuse | `len/4` everywhere, no `/tokenize`, no `id_slot`, `cache_prompt` only forced off on retry | Unchanged |
| Reranker / sparse | Both off by default | Unchanged (and the evidence for "off" came from the wrong path) |
| OCR | No-op port | Unchanged |
| Evaluation | Retrieval eval on the wrong orchestrator; no answer, grounding or tool-loop eval; CI only validates datasets | Worse than thought |
| Data model | ≈140 live tables, 62 triggers, 39 migrations (18 Learning migrations in 7 days, 24 ALTERs within 1–6 days of creation) | New finding |
| Renderer | Streaming and virtualisation done; React Query discipline in Learning; `components/` vs `features/` migration stalled (4 of 29) | Virtualisation fixed |
| Release | CI builds and drives packages on macOS ARM and Intel, Ubuntu and Windows; single-instance guard; still ad-hoc signing (`signingIdentity: "-"`), no updater | Much better |

## 3. Findings, ranked by leverage

### 3.1 Every feature built its own infrastructure

**Jobs and progress: ≈19 implementations** [verified]

- About ten durable tables use six status vocabularies:
  - `batch_jobs`
  - `download_sessions`
  - `summary_work_queue`
  - `vault_write_outbox`
  - `cluster_runs`
  - `documents.status`
  - `conversation_messages.status`
  - `learning_generation_jobs` and its checkpoints
  - `learning_practical_runs`
  - `learning_pack_imports`
- About nine in-memory registries or channels. Among them, indexing has two
  different `IndexProgress` structs (`indexing/commands.rs:501`,
  `indexing/engine/progress.rs:7`).
- `learning_generation_jobs` (`20260930080000:53`) is the most complete job
  schema in the app: idempotency, progress, staged result, retry, activity
  record, startup recovery. It is private to Learning.
- Deep research, the longest-running work, cannot survive a restart.

**Model access: ≈13 paths, no scheduler** [verified; contention effects inferred]

- Chat (including deep research, Explorer and journal synthesis) uses the
  full stack: retrieval, budget, tool loop and verifier.
- These build their own budget and retrieval:
  - handoff
  - study
  - learning (lessons, assessment, practice, planning)
  - summaries
  - compare
  - starters
  - labelling
  - HyDE
  - intent/router
- The context assembler budgets only conversation memory, not the whole
  prompt.
- Limiters exist per feature:
  - Ollama: `ollama_client.rs:48`
  - maintenance: `compaction.rs:179`
  - lesson generation: `generation_jobs.rs:34`
  - lab guests: `embedded_runtime.rs:128`
  - grounding judge: `judge.rs:58`
- Chat itself is not limited.
- llama-server's parallel slots share one context window
  (`sidecar_manager/startup.rs:20-28`), but chat budgets as if it owned the
  whole window (`chat/retrieval/mod.rs:361`).
- The grounding judge starts after the answer is saved (`turn.rs:943`), so it
  competes with the user's next turn.
- When the memory plan doesn't fit, compaction runs before generation
  (`turn.rs:775`, `memory_context.rs:72`).

**LLM API: two generations** [verified]

- All four real providers implement the typed `CompletionRequest` path.
- 19 fallback branches in 14 files keep the old string API alive, and only
  test mocks reach them.
- `ollama_client.rs` (2,687 lines) implements three generations of the same
  call.
- The tool loop picks a replay strategy by matching provider names as strings
  (`tool_loop.rs:414,423`).
- Stream, tool-call and reasoning parsing exists about four times.

**Retrieval: two orchestrators and five vector stores** [verified]

- `HybridSearchUseCase` (chat) and `HybridSearchService` (tool executor and
  eval) duplicate branch execution, the sparse-search switch and weight
  normalisation. They share only the fusion arithmetic.
- Explorer has its own chunker, rank fusion and `chunks.db`.
- Conversation memory and Learning each use brute-force cosine search.
- Learning stores its embeddings as JSON text in
  `learning_source_retrieval_index`, and re-embeds library documents that
  are already chunked and embedded (`learning/references/source_library.rs:161`
  reads `documents`/`text_chunks` with raw SQL).

**Learning persistence** [verified]

- 13 `learning_*_operations` ledgers, each with its own replay function.
- Seven copies each of `fn hash` and `fn now`.

**Study and Recall** [verified]

- Two schedulers write the same `study_cards` table.
- Study refuses to review FSRS cards (`study/repository.rs:520`).

**Why this ranks first:** every cross-cutting improvement on the roadmap is
blocked or multiplied by it. Real token counts, KV reuse, priority
cancellation, resumable research and a reranker decision all depend on it.

### 3.2 The eval measures code chat doesn't run [verified, re-checked]

- `examples/retrieval_eval/production.rs:1` calls itself "the app's real
  retrieval path", but it builds `HybridSearchService` (`:51`, `:170`).
- Chat calls `container.hybrid_search_use_case()`
  (`chat/retrieval/kb_retrieval.rs:228`) and reranks in a separate step
  (`chat/retrieval/rerank.rs:17`).
- Every retrieval number since 09-16 describes the tool executor's path. A
  chat retrieval change cannot move the score.
- No quality eval covers prompt assembly, the tool loop, routing or grounding
  verdicts. CI only unit-tests the graders and validates the datasets
  (`eval-integrity.yml`).
- This was Phase 1 of the 09-24 plan and is still the gate for every
  retrieval decision.

### 3.3 Feature boundaries are nominal [verified]

**The cycle** has these two-way pairs:
- embedding↔indexing
- embedding↔search
- llm↔model_management
- download↔model_management
- conversation↔qa
- conversation↔explorer
- conversation↔web
- function_calling↔web
- indexing↔web
- settings↔vault
- cache↔search
- daily_notes↔vault

**Contracts bypass `application`.** `lib.rs:11-14` says cross-feature
contracts go through `application`; 364 references bypass it.

**Business code reaches into the Container.** Non-adapter files that import it:
- `conversation/synthesis.rs:7`
- `branching.rs:8`
- `compaction.rs:21`
- `handoff.rs:14`
- `compare/use_case.rs:12`
- `qa/starters.rs:22`
- `explorer/folders.rs:14`
- `learning/references/sources.rs:8`

The checker forbids it in only two files.

**Live state in globals** (about 14 statics):

| State | Where |
|---|---|
| Explorer index `MANAGER` | `explorer/index/manager.rs:1064` |
| Summaries hook registry | `summaries/trigger.rs:20` |
| Chat cancellation registry | |
| Handoff in-flight set | |
| Four learning `ACTIVE` maps | |
| Learning `GENERATION` and `GUEST_SLOTS` semaphores | |
| Background `REGISTRY` (a global back door to the managed tasks) | |

**Single-feature code sits in shared layers:**
- `application/services/{conversation_memory,context_assembler}` (≈7k) is
  conversation-only.
- `infrastructure/services/{article_extractor,router,intent,model_manager,context_manager}`
  each belong to one feature.
- `shared/http/stealth.rs` is web-only and configured by `RECALL_*`
  environment variables outside settings.

**Infrastructure re-exports feature internals:**
- `infrastructure/services/mod.rs:31-47`
- `ml/mod.rs:10-15`
- `events/mod.rs:6`
- the persistence mappers and repositories

The doc admits the motive was to keep the layer rules checkable
(`RUST_ARCHITECTURE.md:24-27`).

**Cost:**
- No feature can be compiled or tested on its own.
- Testing synthesis or compaction needs a whole Container.
- Every edit re-typechecks a 250k-line production crate.
- More than 30 integration-test binaries each link all of it.

### 3.4 Learning is a product inside the product [verified]

**Size and ownership:**
- 58.9k lines (≈15k of them tests).
- 91 commands (98 with study).
- 77 of about 140 tables.
- 18 of 39 migrations.

Its internal split by capability is clean:

| Submodule | Lines |
|---|---|
| lessons (content_verification is 11k of this) | 21k |
| planning | 6.7k |
| portability | 6k |
| practice | 5.7k |
| runtime | 3k |

**Repositories hold the workflows.** They are transaction scripts that also
contain state machines. `curriculum_repository.rs` runs the job lifecycle:
`start_job:919`, `advance_job:1298`, `recover_running_jobs:1571`.

**The repository barrier can't see Learning's SQL.** It matches on file
names, so it misses the SQL in `source_library.rs` (48 call sites),
`reference_collection.rs`, `generation_jobs.rs` and
`content_verification/mod.rs`.

**It writes other features' tables inside its own transactions:**
- daily notes (`repository.rs:696`)
- study decks and cards (`:920`, `:1118`)
- `plugin/memory.rs` touches four other features

**Lab code runs in the app process** (the risk is inferred):
- JavaScript runs in QuickJS on a worker thread.
- Python is CPython on the wasmtime JIT.
- The sandbox itself is well built: read-only mounts, fuel and epoch
  limits, a self-test probing host, workspace and network access.
- But the guest shares an address space with the database pool and keyring
  access, and the code it runs is LLM-generated from web-sourced material.
- Cost: about 43 extra crates and a 39 MB Python bundle in every build, with
  no cargo feature to leave them out.

**The good news:** nothing imports Learning, so it is the cleanest
candidate for a feature crate. It is blocked only by its Container access
(67 pool pulls, 14 `get_or_load_llm`) and its imports of web, study and
daily_notes.

### 3.5 The renderer's boundaries hold only literally [verified]

**Server state escapes React Query.**
- 48 of the 102 files that call the backend use no React Query hook. 31 of
  them fetch in `useEffect` into `useState`.
- Spaces are a cached query, but are fetched again ad hoc in five views
  (`FileBrowser.tsx:180`, `ContextMenu.tsx:110`, `BatchFileImport.tsx:260`,
  `FolderSettingsDialog.tsx:45`, `LearningDocumentPicker.tsx:24`).
- `ChatPanel.tsx:324` re-reads settings even though `useSettingsQuery`
  exists.
- Explorer index status has two listeners feeding two states.
- Invalidation can't reach any of these copies.

**Five ways to reach the backend.**
- The `invoke` lint ban is commented out (`eslint.config.js:160`).
- Six files call raw `invoke()`.
- 58 view files import `VaultAPI` or a feature client directly. The
  view→transport rule is linted only for the sidebar.
- Chat streams over a broadcast event; Learning uses request-scoped
  `Channel`s.

**The IPC plumbing duplicates generated information.**
- `shared/ipc/routes.ts` has 352 hand-written entries, of which 346 just
  restate a domain.
- The generated `commands` object is never called (it uses bare,
  un-prefixed names).
- The transport imports Learning's `studyActivity`.
- About 33 DTO names are re-declared by hand in `types/api`.

**The folder migration stalled.**
- Only 4 of 29 `features/` dirs were migrated; the other 25 contain only
  `api/client.ts`.
- Import cycles: chat↔explorer and chat↔journal.
- "Spaces" has no owner: its picker, editor and `GENERAL_SPACE_ID` live
  inside chat.

**Initial JS is at the ceiling.**
- 1,047,130 of 1,050,000 bytes.
- The chat controller is mounted on every route.
- A legacy localStorage migration runs at startup
  (`App.tsx:10`, against the no-legacy rule).

**Backend-owned links are kept in localStorage.** The folder→thread map, journal pins and
note-by-space mapping are lost on backup or restore.

**Design primitives are duplicated.**
- Two Buttons, three Skeletons, two toast systems.
- 19 hand-rolled dialogs.
- Case-only filename pairs (`Button`/`button`), which are a hazard on
  macOS's case-insensitive filesystem.

**The e2e mocks sit outside the contract check.** Eight copied
`__TAURI_INTERNALS__` shims are not covered by
`scripts/check-ipc-contracts.mjs` (which scans only `src/`), so a DTO rename
can pass the browser tests.

### 3.6 Unused surface and data gaps [verified unless marked]

**Secret exposed with no caller.** `credentials_get` returns a stored API
key to the webview (`credentials/plugin/commands.rs:24`), is allowed in
`capabilities/main.json:224`, and nothing in `src/` calls it. Six other
credentials commands are also uncalled.

**Unused code:**
- The legacy QA pipeline (≈2.5k lines) is built on a `NoOpLLMClient`
  (`qa/di.rs:24-45`) and never reached:
  - `qa/conversational_service.rs`
  - `qa/engine/*`
  - `infrastructure/services/context_manager.rs`
- `AppContainer` (`interfaces/di/mod.rs:140`).
- 19 command functions that are never registered.
- About 14 registered commands with no renderer caller.
- `infrastructure/sagas` is empty.
- `persistence/repositories/mocks.rs` (1,476 lines) compiles into
  production.
- The Cargo features `search`, `qa` and `extraction` gate nothing.

**Backups carry derived data.** `learning_source_retrieval_index` is
derived JSON-vector data but is not in `EXCLUDED_TABLES`
(`backup/archive/snapshot.rs:31`), so backups carry it.

**Stores span two roots** (`data_dir` and `~/.lattice`), with no registry of
who owns each store or whether it is backed up. The web archive
(`~/.lattice/web-archive`) appears to be absent from backups [inferred].

**`api-rust`** (2.3k lines) has no caller. It costs a CI job with a
Postgres service, an audit step, a lockfile and a 440 MB `target/`.

## 4. Keep

These are working and should not be reworked:

- **The inner layer rules and their syntax-aware checker.** This is the seam
  for a crate split.
- **The command inventory and IPC contract checkers.**
  - 378 commands are consistent across `generate_handler!`, `build.rs` and
    capabilities.
  - 365 renderer call sites checked against 376 generated contracts.
- **The typed `CompletionRequest`** (budget, output cap, native input).
- **One llama-server shared across roles,** and the sidecar's reuse of the
  llama.cpp message and stream code.
- **Conversation memory's layering:** pure validation, atomic CAS commit,
  ownership checked in every query, and explicit budget overflow.
- **`claim_verification` as one judge with policies** (`Chat`, `Strict`,
  `Fidelity`), already reused by Learning. It is the template for the
  consolidation in §5.
- **`learning_generation_jobs` as a schema.** Promote it rather than design
  a new one.
- **Operation-id and payload-hash idempotency.** The idea is right; only the
  implementation is duplicated.
- **The renderer's streaming path:** rAF batching, selector subscriptions,
  memoised `Message`, and the virtualised list.
- **Learning Studio's renderer structure** and its coverage floor.
- **Lazy routes, error boundaries and the JS budget gate.**
- **The Python sandbox's limits.** Keep them as they are when execution
  moves out of process.
- **The CI desktop-compatibility matrix,** including the Intel Mac.

## 5. Plan

The order is chosen so that each step makes the next cheaper. Tier 0 stops
the debt from growing; Tier 1 builds the shared runtimes the roadmap's
remaining phases need.

### Tier 0: one session, low risk (all S)

1. **Delete the dead surface:**
   - the legacy QA pipeline (move HyDE and starters to search/conversation)
   - `AppContainer` and the 19 orphan `#[tauri::command]` attributes
   - the unused credentials commands, `credentials_get` first
   - `infrastructure/sagas`
   - the vestigial Cargo features
   - the renderer's legacy collections migration

   Re-check each uncalled command against `src/` before deleting it.
2. **Fix the backup data gaps:**
   - Add `learning_source_retrieval_index` to `EXCLUDED_TABLES`.
   - Confirm whether `~/.lattice/web-archive` is backed up.
   - Write one list of the on-disk stores, their owners and their backup
     status.
3. **Ratchet the outer ring.** Record today's violations as a baseline so
   CI fails only when a count goes up. Rust rules:
   - infrastructure must not import features
   - feature→feature imports must follow an allowlisted graph
   - the Container may be imported only from plugin, command, DI and
     desktop files
   - no raw `tokio::spawn` anywhere in the crate, except at sites carrying
     an allow marker
   - the inventory script flags unregistered command functions

   Renderer rules:
   - re-enable the `invoke` ban
   - the view→client rule applies to all `components/**`
   - `import/no-cycle` for `src/features`
4. **Correct the doc drift in §6.**
5. **Run `api-rust` CI only on a path filter or by hand.**

### Tier 1: shared runtimes (M each; 1 gates retrieval work, 2 and 3 are independent)

1. **One retrieval orchestrator, with the eval pointed at it.**
   - Keep `HybridSearchUseCase` and move chat's rerank step into it.
   - Delete `HybridSearchService`, then rewire the tool executor and the
     eval to the one orchestrator.
   - Re-baseline v3 once.
   - Then redo the reranker and sparse-search decisions (roadmap Phase 5)
     on the real path.
2. **One scheduler in front of the model.**
   - Priorities: interactive chat, then the grounding judge, then
     maintenance, then batch generation.
   - It knows the server's slots and context window.
   - Each request carries a cancellation token.
   - It replaces the five per-feature limiters and the three cancellation
     mechanisms.
   - Real token counts (llama-server `/tokenize`, cached) belong here. So
     does roadmap Phase 4 (KV reuse, `id_slot`).
3. **A shared job runtime.**
   - Promote the `learning_generation_jobs` schema and its cancel registry
     to `shared/runtime/jobs`.
   - First adopters: deep research and synthesis, so both become
     restart-resumable.
   - Then batch import and Explorer folder runs.
4. **Renderer: give Spaces an owner.**
   - Create `features/spaces`.
   - Move spaces, settings and Explorer status into React Query.
   - Turn FileBrowser's writes into mutations.
   - Store the folder→thread link on the `explorer_folders` row.
   - This breaks the chat↔journal cycle.

### Tier 2: structural (M–L; after Tier 1)

5. **Typed LLM API only.**
   - Delete `LLMClient` and the 19 fallbacks.
   - Providers return their own replay items, so the tool loop stops
     matching names.
   - Merge the sidecar and remote llama.cpp clients, including retry.
6. **A grounded-generation service.**
   - The assembler owns every budget pool: evidence → assembler → typed call
     → optional `claim_verification`.
   - Move handoff, study, learning, compare and summaries onto it.
   - Split `run_turn` into stages, and narrow the chat runtime port.
7. **Per-feature DI structs instead of the Container.**
   - Start with learning, conversation, explorer, daily_notes and study.
   - Move the global registries into them so shutdown can drain them.
   - Move single-feature code from `application`/`infrastructure` back into
     its feature.
8. **Consolidate Learning.**
   - One operations ledger and one replay helper.
   - Move state machines out of the repositories.
   - Fold Study into Recall, using FSRS only.
   - Learning retrieval goes through a library port and reuses the
     library's vectors.
   - Optionally re-squash the 18 Learning migrations. That needs Josh's call
     and a DB delete.
9. **Lab execution out of process** behind a `learning-labs` cargo feature,
   the way llama-server already runs.
10. **Split the crate:**
    1. core (domain, application, shared)
    2. infrastructure, once its re-exports are gone
    3. the app crate
    4. Learning
11. **Renderer:**
    - Derive the IPC map from the generator, emitting plugin-qualified names,
      and delete `routes.ts`.
    - Migrate to feature-first folders one feature per PR, splitting
      ChatPanel at the composer and attachment seams.
    - Build one typed mock-IPC harness covered by the contract check.
    - Remove the duplicate primitives.

### Still open from the 09-24 roadmap

| Phase | Status |
|---|---|
| Phase 1 (answer and grounding eval sets) | Now depends on Tier 1.1 |
| Phase 4 (tokens, KV reuse) | Folds into Tier 1.2 |
| Phase 5 (reranker, sparse) | After Tier 1.1 |
| Phase 6 (OCR, layout PDF) | Untouched |
| Phase 7 (Developer ID signing, notarisation, updater) | The packaging half is done |

## 6. Doc drift

Fix these together with the Tier 0 deletions. In `docs/RUST_ARCHITECTURE.md`:
- **:17-23.** The doc says there are no `#[path]` aliases and the
  infrastructure aliases are gone. Two production `#[path]`s remain
  (`learning/planning/curriculum_repository.rs:15`,
  `setup/renderer_shutdown.rs:6`), and the infrastructure re-exports remain.
- **:27-29.** The doc says the container holds only construction, with a
  registrar per feature. Ten features have no registrar, and `AppContainer`
  still compiles.
- **:104.** The doc says view files cannot import the IPC transport. That is
  true only literally (§3.5).
- **:110-118.** The doc implies supervised background work is the norm, with
  a shared generation slot. Raw spawns remain, and the slot is shared only
  within Learning.
- **:130-132.** The doc says workflow modules cannot reach the container.
  Only two files are checked.
- **:162-168.** The doc says the assembler is the single budget owner, shared
  by chat and QA. Chat budgets outside it, and the QA path is dead.
- **:198-207.** The provider section leaves out the cloud provider and the
  sidecar's shared llama.cpp code.
- **:288-292.** The doc says Study owns flashcards. A Learning migration owns
  the schema, and the review paths are split.
- **Missing entirely:** Learning's JSON vector index, the `~/.lattice`
  roots, and the fact that lab runtimes run in-process.

Elsewhere:
- **`examples/retrieval_eval/production.rs:1`** calls itself "the app's real
  retrieval path". It isn't.
- **`qa/di.rs:25-27`** says the service "picks up" the real client. It never
  does.
- **`lib.rs:11-14`** says cross-feature contracts go through `application`.
  364 references bypass it.
- **`CONTRIBUTING.md:69-81`.** Feature clients, React Query for backend state,
  and one representation per DTO are each contradicted (§3.5).
- **`eslint.config.js:210`** cites a `DESIGN_TOKENS.md` that does not exist.
- **The 09-24 roadmap** (lines 15, 17) is stale on sidecar tools and the
  verifier window. The 09-25 audit is stale on virtualisation.

## 7. Method and limits

- **Reviewers.** Four Opus reviewers, read-only, no builds, about 75 tool calls
  each (≈665k tokens in total).
- **Counts** are grep-based lower bounds. They exclude `#[cfg(test)]` modules
  where stated.
- **Re-checked by hand:**
  - eval vs chat orchestrator (`production.rs:1,51`; `kb_retrieval.rs:228`)
  - QA on `NoOpLLMClient` (`qa/di.rs:24-45`)
  - `credentials_get` returns `Option<String>`, is in capabilities, and has
    no caller in `src/`
  - `EXCLUDED_TABLES` lacks the Learning index (`snapshot.rs:31-40`)
  - the JS budget constant (`check-initial-js-budget.mjs:8`)
  - compaction only blocks the turn when the plan does not fit
    (`memory_context.rs:72,84`), a refinement of the reviewer's wording
- **Not covered:**
  - runtime behaviour, performance and memory under load
  - Windows paths
  - the 15-file uncommitted diff, which a tool may still be editing
  - the effect of the 27 commits on this branch that are not yet on
    `origin/main`
