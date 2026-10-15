# Move a generated course to another computer and resume

Date: 2026-10-13. Status: build brief v2 (reviewed against the code) for branch
`learning/pack-move`. Two builders: backend (`src-tauri/`) and frontend (`src/`),
working from the Contract section alone. Integrate backend first: the frontend's
type-check only passes once the regenerated bindings land.

## Goal

A learner exports a generated course (with their progress) to a file they choose,
copies it to another computer, imports it there with no course open, lands on
that course, and resumes at the lesson they left off, with their flashcard
schedule and quiz attempts intact. Studying on the second computer and bringing
the course back to the first one works too (replace, with a backup). They can also
export every course at once into a folder as a backup.

## What exists (do not rebuild)

`.lattice-learning` packs: zstd tar with `manifest.json`, per-entry SHA-256, root
checksum, format version 3 (`src-tauri/src/features/learning/portability/pack.rs`).
Decode never extracts to disk (entries live in a `BTreeMap`), `validate_pack_path`
rejects absolute/`..`/`\`/`:` paths (pack.rs:112-137), non-regular tar entries are
refused (pack.rs:484-486), and every size is bounded: compressed, decompressed,
per entry, manifest (2 MiB), entry count. Commands in
`src-tauri/src/features/learning/plugin/portability.rs`:
`get_learning_portability_workspace`, `export_learning_pack`,
`preview_learning_pack_import`, `apply_learning_pack_import`,
`cancel_learning_pack_import_preview`. Repository in
`portability/pack_repository.rs` (2149 lines; do not add to it beyond wiring) with
`export.rs`, `replacement.rs`, `provenance.rs`, `remap.rs`, `validation.rs`.
Conflict policies: `create_copy` (new IDs), `merge_safe` (keep IDs, blocked when
the program or any record ID already exists, no test exercises it),
`replace_after_backup` (BEGIN IMMEDIATE, backup pack written before delete,
`PRAGMA defer_foreign_keys=ON`, one transaction; replacement.rs:14-42).

Resume state is always in the pack: `current_lesson_id` and every lesson's
`completed` ride in `program/program.json`, `curriculum_state`/`replacement_lesson_id`
in `curriculum/lesson-state.json` (export.rs:38-58). With `includeEvidence`
(UI default on): `program/attempts.json`, the 32-table aggregate
`evidence/learning-aggregate.json` (pack_repository.rs:467-500; flashcard decks,
cards with `stability`/`difficulty`/`due_at`/`last_reviewed_at`, reviews,
assessments, practice, tutor turns, recall, operations), evidence events,
follow-ups and canvases. With `includePracticalArtifacts` (UI default off):
activities, runs, simulations. Preview and apply already work with no program in
the database: `workspace()` tolerates a missing program (pack_repository.rs:582-595)
and the apply response's `programId` is the imported program (:1622), also
exposed as `imports[0].importedProgramId` (portability_dto.rs:83). The preview's
`incomingProgramId` is the id the import WILL produce (a fresh uuid for
`create_copy`; :1092-1103).

UI: `src/features/learning/portability/PortabilityPanel.tsx`,
`useLearningPortability.ts` (hooks keyed by `programId`, query disabled without
one), client at `src/features/learning/api/client.ts:447-485`, entry only inside
an ACTIVE course (`ProgramWorkspace.tsx:223` gates the More menu on
`activeProgram`, :241 the panel). The import file is chosen with the renderer's
`open()` from `@tauri-apps/plugin-dialog` (PortabilityPanel.tsx:83);
`capabilities/main.json` already grants `dialog:default`, `dialog:allow-open`,
`dialog:allow-save`. After an import, `ProgramWorkspace.handleImported` (:67-79)
returns early when the imported id differs from the open course.

## Gaps to close (all in scope)

1. **Attempts never import.** `export.rs:40` moves attempts out of `program.json`
   into `program/attempts.json`; `preview()` reads that entry back (:688-690) but
   `apply()` parses only `program/program.json` (:1194-1200), so `insert_program`
   (:2126) and the create-copy id map (:1376) see zero attempts. Quiz and module
   test history is silently dropped on every import today. No test mentions attempts.
2. **Export goes only to the app data folder** and fails on a name collision
   (`managed_path`, :345-367).
3. **No import entry when no course is open** (fresh machine, zero courses).
4. **The user is not taken to the imported course** (copy, or any import started
   from the course list).
5. **Moving back is impossible in practice.** `merge_safe` is blocked when the
   program exists (:723-741, :1242-1246), and `replace_after_backup` is blocked
   whenever the original has a single lesson note, a runtime profile reference or
   a hidden evaluator file (`require_recoverable_on`, replacement.rs:44-67). After
   A -> B -> A only `create_copy` remains: a duplicate course with new IDs.
6. **Nothing blocks export or replace while a generation job runs.** Replace
   cascades `learning_jobs`, and the `learning_program_jobs_deleted` trigger
   (migrations/20261009140000_jobs.sql:77-80) deletes the `jobs` rows under a
   running worker.
7. **All 32 evidence tables go into one entry and all canvases into one entry**
   (export.rs:88-93, :174-189) under an 8 MiB entry cap; heavy history will not fit.
   `MAX_PACK_BYTES` (pack_repository.rs:32) caps the file read at 64 MiB too.
8. **Draft courses cannot be exported from the UI** (ProgramWorkspace.tsx:223).
9. **No "export all courses" backup.**
10. **A pack from a different build is accepted silently** when only the format
    version matches: `program.json` is parsed with default serde (unknown fields
    ignored), so DTO drift between builds loses data without an error. The
    evidence import already fails loudly on column drift (:145-157); the rest
    does not.
11. **`merge_safe` has no test.**

Not a gap (dropped from v1): `content_revision`. It is not in
`LearningProgramSummaryDto` (dto.rs:113-124), so no pack ever carried it; it is
maintained by triggers (migrations/20261006002000) that fire on every
module/lesson/block/question/answer-key insert, so `insert_program` would be
overwritten by its own inserts; and nothing portable reads it (only
`learning_jobs.content_revision`, and jobs are not portable). Leave it to the
triggers.

Out of scope, flagged in the PR and in the export UI: lesson notebook notes
(`learning_lesson_note_links` -> daily notes; counted and shown before export,
preserved across a replace, see below), practical drafts
(`learning_practical_drafts`, counted and shown), runtime profiles, cross-build
compatibility (pre-release, no compat shims, see memory `no-legacy-support`).
Both machines must run the same build; the pack now proves it (gap 10).

## Contract (backend <-> frontend)

All commands live in the `learning` plugin. A new command is registered in FOUR
places or it fails at runtime: `plugin.rs` `generate_handler!`, `build.rs`
`InlinedPlugin::new().commands(&[...])` for `learning`, `bin/export_bindings/main.rs`
`collect_commands!`, and `capabilities/main.json` as `learning:allow-<name-with-dashes>`
(`scripts/check-tauri-command-inventory.py` enforces this). No dialog permission
changes are needed. No new migration is needed.

Native dialogs are opened FROM RUST inside the command, as the backup feature does
(`features/backup/plugin.rs:183-200`, `plugin_choose_archive_destination` :265-294,
`interfaces/di/container.rs:500-506`, design doc 2026-09-16 lines 94-98): the
webview never supplies a write path, so a compromised renderer cannot turn export
into an arbitrary-write primitive. Learning gets its own ten-line `pick_save_file`
/ `pick_folder` helpers in `plugin/portability.rs` (importing backup's would add a
`features::learning -> features::backup` edge that the ratchet baseline does not
have). The import file keeps the renderer-side `open()` as today: the read is
bounded by `read_untrusted` (symlink, non-file, empty and size checks,
pack_repository.rs:368-380) and the decoder, so the worst case is "not a pack".

### Unchanged shapes

`get_learning_portability_workspace`, `preview_learning_pack_import`,
`apply_learning_pack_import`, `cancel_learning_pack_import_preview`,
`ExportLearningPackRequestDto`.

### Changed behaviour, same shape

`export_learning_pack(request)` still writes to `<app data>/learning-packs/`.
On a name collision it writes `<stem> (2).lattice-learning`, `(3)`, ... instead of
failing. It refuses (typed `AppError::InvalidInput`, surfaced as the error string)
when the course has a pending/running job or fails `validate_program`
(message names the reason: "still being prepared" / "has no modules yet").

`LearningPortabilityWorkspaceDto` gains one field:

```ts
exportNotice: {
  activeJob: boolean;          // pending/running job for this course
  notebookNoteCount: number;   // lesson notes that stay on this computer
  practicalDraftCount: number; // unsaved lab drafts that stay on this computer
};
```

`apply_learning_pack_import` additionally imports attempts (gap 1) and, under
`replace_after_backup`, keeps lesson note links whose lesson still exists after
the import (same lesson IDs).

`preview_learning_pack_import` adds a blocked conflict
`{ entityKind: "program_jobs", resolution: "blocked" }` under
`replace_after_backup` when the target course has an active job, and a blocked
conflict `{ entityKind: "pack_schema", resolution: "blocked" }` (any policy) when
the pack's `schemaVersion` differs from the local database.

### New commands

```ts
// Save one course to a file the user picks in a native save dialog (opened by Rust).
save_learning_pack_as(request: ExportLearningPackRequestDto) -> SaveLearningPackAsResultDto
type SaveLearningPackAsResultDto = {
  cancelled: boolean;                       // true: nothing written, nothing recorded
  export: LearningPackExportDto | null;     // the written pack; `destinationPath` is the real path
};
```
`request.fileName` is only the dialog's suggested name. Rules applied to the chosen
path in Rust: must end in `.lattice-learning` (appended if missing); must not be
inside the app data directory (same rule as the backup destination); if it exists
it must be a regular file with that extension (never a symlink or directory), and
it is replaced atomically because the OS dialog already asked; parent directory is
not created. Same ledger replay semantics as `export_learning_pack`.

```ts
// Export every course (draft and active), one file each, into a folder the user picks (Rust folder dialog).
export_all_learning_packs(request: ExportAllLearningPacksRequestDto) -> ExportAllLearningPacksResultDto
type ExportAllLearningPacksRequestDto = {
  operationId: string;
  includeEvidence: boolean;
  includePracticalArtifacts: boolean;
  includeSourceBodies: boolean;
  sourceBodyRedistributionConfirmed: boolean;
};
type ExportAllLearningPacksResultDto = {
  cancelled: boolean;
  destinationDir: string | null;
  exported: { programId: string; title: string; path: string }[];
  failed: { programId: string; title: string; error: string }[];
};
```
File name from the title via `pack_file_name(title)` (strip `/ \ :` and control
chars, trim, cap at 80 bytes on a char boundary, fallback `course`), collision
suffixing, never overwrite, folder must exist, be a directory after
canonicalisation and not be inside the app data directory. Continues past
per-course failures (active job, validation, I/O) and reports each. Each course
uses operation id `uuid::Uuid::new_v5(&batch_operation_id, program_id)` so a
retried batch replays instead of writing `(2)` duplicates; the batch itself is
recorded in the ledger (`scope: "portability"`, `kind: "export_all"`, result = the
DTO) so an identical retry returns the recorded result. Zero courses ->
`{ cancelled: false, destinationDir, exported: [], failed: [] }`. No progress
channel: the activity panel shows the task; results arrive at the end.

### Pack format v4 (accept only 4)

- `LEARNING_PACK_VERSION = 4`; decode rejects every other version (test
  `only_the_current_pack_version_imports` keeps passing). Stale comment at
  pack.rs:16-17 replaced. Consequence (pre-release, acceptable, say so in the PR):
  v3 files in `learning-packs/` and replacement backups made before this build
  cannot be previewed; their `learning_pack_exports` rows still list them.
- Manifest gains `schemaVersion: number` = `SELECT MAX(version) FROM _sqlx_migrations`
  at export. Preview refuses a pack whose value differs ("made by a different build
  of Lattice"). This is the "same build" guarantee: a migration bump ships with
  every DTO change.
- Evidence becomes one entry per table: `evidence/tables/<table>.json`, kind
  `evidence`, in `EVIDENCE_TABLES` order; on import all 32 must be present when
  the manifest declares learner evidence, extra `evidence/tables/*` entries are
  rejected, and the in-memory `LearningEvidenceSnapshot` is rebuilt so
  `validate_aggregate_id_namespaces`, `map_evidence_ids` and
  `insert_evidence_snapshot` are untouched. `evidence/learning-aggregate.json` is
  gone. `validate_content_policy` (pack.rs:263-279) gets the reserved prefix
  `evidence/tables/` -> `Evidence`. No table name contains `evaluator`,
  `solution` or `check-key` (pack.rs:247-255 would reject it; checked).
- Canvases become one entry per canvas: `program/canvases/<canvas-id>.json`
  (kind `canvas`; the canvas row plus its snapshots), reserved prefix
  `program/canvases/` -> `Canvas`. `program/canvases.json` is gone.
- Limits: `MAX_ENTRY_BYTES` 32 MiB, `MAX_TOTAL_BYTES` 256 MiB,
  `MAX_COMPRESSED_BYTES` 256 MiB, `MAX_PACK_BYTES` (pack_repository.rs:32)
  256 MiB, `MAX_ENTRIES` stays 4096. The whole pack is held in memory during
  import (file + decoded + parsed, about three times the pack size); accepted
  for a desktop app. Import keeps its existing per-table 50 000-row cap (:159-161).
- Foreign-key order across the split entries needs no work: import runs in one
  `BEGIN IMMEDIATE` transaction with deferred foreign keys (replacement.rs:21-25),
  tables are inserted in `EVIDENCE_TABLES` order as today, and a failure anywhere
  rolls the whole import back.

### Conflict policy wording (frontend) and the move-back flow

Three choices, presented as plain choices, not a dropdown:

| policy | label | one-line description |
|---|---|---|
| `merge_safe` | Move it here | Keeps the course and your progress exactly as exported. For a course that is not on this computer yet. |
| `replace_after_backup` | Replace the copy on this computer | A backup of this computer's copy is saved first; then the pack's version takes its place with the same course identity. |
| `create_copy` | Add as a separate copy | Keeps both. The imported course gets new identifiers, so its history is separate. |

Flow: pick file -> preview with `merge_safe` -> if the preview has a blocked
`program` conflict, show "This course is already on this computer (<existing
title>)" with **Replace the copy on this computer** (default) and **Add as a
separate copy**; either re-previews with that policy (new `previewId` and
`operationId`; cancel the blocked preview) -> Apply confirmation (existing) ->
`onImported(programId)`. Any other blocked conflict (`pack_schema`,
`program_jobs`, foreign record IDs) is shown with its text and only the policies
that can proceed.

Replace is the move-back policy. Backend changes that make it work:
`require_recoverable_on` blocks only on hidden evaluator files (`role IN
('check','solution')`), which the pack genuinely cannot restore. Runtime profile
references already survive when the profile exists locally
(pack_repository.rs:1860-1876). Note links are snapshotted inside the replace
transaction before the delete and re-inserted after `insert_program` for every
`(program_id, lesson_id)` that still exists; the backup pack omits them, but
nothing is lost because they are re-attached in the same commit.

### "Resume" after import

Landing on the course is resuming: `ProgramWorkspace` initialises module and
lesson from `summary.currentLessonId` (ProgramWorkspace.tsx:52-53) and shows
Continue / CourseCoach. The frontend selects the imported program id, invalidates
`LEARNING_PROGRAMS_KEY` and `learningProgramKey(id)`, and runs
`refreshLearningProgramAfterImport(client, id)` (already done by the apply hook).
No lesson-level routing is needed.

### Export options for a move

Evidence on, practical artifacts on, source bodies off (licensing self-attestation
stays an explicit tick), with a one-line hint that without full text the other
computer gets excerpts only and web references can be refreshed there.

## Backend work (`src-tauri/`)

New modules (pack_repository.rs only gets wiring; nothing else grows past 800 lines):

- `src/features/learning/portability/pack_repository/program_entry.rs`:
  `load_program(&DecodedLearningPack) -> Result<LearningProgramDto>` = program
  entry + `program/attempts.json` when present. Used by `preview` and `apply`
  (fixes gap 1; `validate_program` and `validate_answer_keys` then see attempts).
- `pack_repository/evidence_entries.rs`: `encode_evidence(&LearningEvidenceSnapshot)
  -> Result<Vec<LearningPackEntry>>`, `decode_evidence(&DecodedLearningPack) ->
  Result<Option<LearningEvidenceSnapshot>>` (strict set/order/no extras), and
  `encode_canvases` / `decode_canvases` for `CanvasPackSnapshot`. `export.rs`,
  `preview` (:981, :1041) and `apply` (:1217-1221, :1212-1216) call these.
- `pack_repository/destinations.rs`: `pack_file_name(title)`, `next_free_path(dir,
  file_name)` (suffixing, symlink/non-file refusal), `chosen_file(app_data_dir,
  chosen) -> Result<PathBuf>`, `chosen_folder(app_data_dir, chosen) -> Result<PathBuf>`.
  `managed_path` delegates its collision handling to `next_free_path`.
- `pack_repository/guards.rs`: `active_job_exists(conn, program_id)` =
  `SELECT EXISTS(SELECT 1 FROM jobs j JOIN learning_jobs lj ON lj.job_id=j.id
  WHERE lj.program_id=? AND j.status IN ('pending','running'))`;
  `export_notice(conn, program_id) -> LearningPackExportNoticeDto`;
  `require_exportable(conn, program_id)` (no active job + `validate_program` on
  the loaded program); `schema_version(conn) -> i64`.
- `pack_repository/export_all.rs`: `LearningPackRepository::export_all(&self,
  req, dir: &Path) -> Result<ExportAllLearningPacksResultDto>`; iterates
  `LearningRepository::list()`, calls `export_to_path` per course with the v5
  operation id, collects failures, records the batch in the ledger.
- `pack_repository/replacement.rs` (edit): narrow `require_recoverable_on`;
  add `snapshot_note_links` / `restore_note_links` used by `apply` around the delete.
- `pack_repository.rs` (wiring only): `export_to_path(&self, req, path) ->
  Result<LearningPackExportDto>` shared by `export`, `save as` and export-all;
  `LearningPackRepository::new(pool, managed_dir, app_data_dir)`; `workspace()`
  fills `export_notice`; `preview` adds the `pack_schema` and `program_jobs`
  conflicts; `apply` uses `load_program`, the entry codecs and note-link
  preservation; `begin_apply` re-checks the active job under the lock.
- `pack.rs`: version 4, `schema_version` in `LearningPackManifest` and
  `LearningPackInput`, limits, reserved prefixes, comment.
- `portability_dto.rs`: `LearningPackExportNoticeDto`, `SaveLearningPackAsResultDto`,
  `ExportAllLearningPacksRequestDto`, `ExportAllLearningPacksResultDto`
  (+ the two row structs), field on `LearningPortabilityWorkspaceDto`.
- `plugin/portability.rs`: `save_learning_pack_as`, `export_all_learning_packs`
  (`app: AppHandle<R>` + `State<Container>`, dialogs via `spawn_blocking` as in
  backup), repository constructed with `container.core.data_dir()`.
- Registrations: `plugin.rs`, `build.rs`, `bin/export_bindings/main.rs`,
  `capabilities/main.json` (`learning:allow-save-learning-pack-as`,
  `learning:allow-export-all-learning-packs`).
- Regenerate `src/lib/bindings.ts` and `src/shared/ipc/routes.generated.ts`
  (`npm run bindings:generate`).

Errors are `crate::shared::error::Result`/`AppError` throughout; export-all
converts per-course errors to strings only in the result DTO. Request structs, no
long parameter lists. No `crate::interfaces::di` imports outside the plugin files.

## Frontend work (`src/`)

- `features/learning/api/client.ts`: `saveLearningPackAs(request)`,
  `exportAllLearningPacks(request)`. `lib/studyActivity.ts` (descriptions map,
  :38-40): entries for both commands, tab `portability`.
- New `features/learning/portability/conflictPolicies.ts`: the three labels and
  descriptions above (single source for panel, flow and tests).
- New `features/learning/portability/ImportCourseFlow.tsx`: program-less import
  (file pick via `open()`, preview with `merge_safe`, the "already on this
  computer" branch, warnings/conflicts/privacy manifest as today, confirm, apply);
  props `{ onImported(programId: string, title: string): void }`. Uses new hooks
  in `useLearningPortability.ts`: `useImportCourse()` (preview/apply/cancel
  without a programId; on apply success runs `refreshLearningProgramAfterImport`
  for the imported id and invalidates `LEARNING_PROGRAMS_KEY`),
  `useSaveLearningPackAs(programId)`, `useExportAllLearningPacks()`.
- New `features/learning/portability/ExportAllCourses.tsx`: options (same as a
  single export, move defaults), "Choose folder and export", result list with
  per-course status, path and Reveal; failures listed with their reason;
  cancelled -> quiet no-op; zero courses -> "There are no courses to export yet".
- `PortabilityPanel.tsx`: export section gets **Save to…** (calls
  `save_learning_pack_as`) beside the existing app-folder export; shows
  `exportNotice` (active job disables both buttons with "This course is still
  being prepared"; "N lesson notes and M lab drafts stay on this computer");
  import section is replaced by `<ImportCourseFlow>`; heading and copy updated
  for the move use case.
- `workspace/LearningStudioPage.tsx`: header gets **Import a course…** and
  **Export all courses…** next to Build a program; the empty state gets
  **Import a course** beside "Build your first program"; both open the flows in
  place of the list. `onImported(id)` -> `setSelectedId(id)` + invalidate
  `LEARNING_PROGRAMS_KEY` and `learningProgramKey(id)`. New prop
  `onOpenProgram(id)` passed to `ProgramWorkspace`.
- `curriculum/ProgramWorkspace.tsx`: the More menu (`:223`) also renders for
  drafts with only "Import & export", and the portability panel (`:241`) renders
  for drafts; `handleImported` (`:67-79`) calls `onOpenProgram(id)` when the id
  differs instead of returning early.
- Tests: update `PortabilityPanel.test.tsx` (labels, Save to…, notice, import
  branch), add `ImportCourseFlow.test.tsx` (fresh-machine happy path with
  `merge_safe`, "already on this computer" -> Replace re-preview, blocked
  `pack_schema` shows no Apply), add `ExportAllCourses.test.tsx` (results and
  failures, cancelled), extend `LearningStudioPage.test.tsx` (import entry on
  list and empty state, landing on the imported course, draft More menu,
  `onOpenProgram` on a different id), keep `learningImportRefresh.test.tsx`
  green. The shared `src/tests/mocks/latticeApiMock.ts` has no learning methods
  today and these suites mock `@/lib/api` locally; add the two methods there only
  if a test uses the shared mock. e2e: `e2e/learning-studio.spec.ts:2295-2302`
  asserts the old heading and button; update it and
  `e2e/helpers/learningStudioBackend.ts` fixtures (`exportNotice`, new commands,
  manifest `schemaVersion`); `e2e/helpers/learningStudioNavigation.ts` keeps
  "Import & export".

## Tests (Rust, same module family)

- `portability/pack_tests/move.rs`: (a) full-evidence export, `merge_safe` import
  into a FRESH database preserves `current_lesson_id`, every lesson's `completed`
  and `curriculum_state`, every attempt (count, answers, results), and the
  flashcard schedule columns `stability`, `difficulty`, `due_at`,
  `last_reviewed_at`, `interval_days`, `review_count` exactly, plus the review
  log; (b) move back: original machine has older progress and a lesson note
  link; `merge_safe` preview is blocked with the `program` conflict;
  `replace_after_backup` succeeds, progress is the pack's, the note link is still
  attached, the backup pack is readable; (c) replace is blocked while the target
  has a pending job (`program_jobs` conflict, and `begin_apply` re-check);
  (d) a pack with a different `schemaVersion` is blocked at preview.
- `pack_tests/destinations.rs`: managed export suffixing `(2)`, `(3)`;
  `export_to_path` writes to a temp path, refuses a path inside the app data dir,
  a symlink target and a non-pack existing file, replaces an existing pack
  atomically; `pack_file_name` strips separators and caps length.
- `pack_tests/export_all.rs`: one file per program (draft and active), a course
  with an active job is reported in `failed` without aborting, replaying the same
  batch operation id returns the recorded result and writes no `(2)` files.
- `pack_tests/evidence_entries.rs`: a table above 8 MiB round-trips; a pack
  missing one of the 32 entries or carrying an extra `evidence/tables/` entry is
  rejected; two canvases round-trip as two entries.
- `pack_repository/replacement_tests.rs`: notebook links no longer block;
  hidden evaluator files still do (existing test).
- `pack.rs` unit tests: version 4 only; limits; reserved prefixes; `schemaVersion`
  in the manifest digest.
- `export.rs` test: export of a program with an active job is refused; an empty
  outline is refused with the "no modules" message.

## Verification before the PR (matches `.github/workflows/ci.yml`)

Rust: `cargo fmt --all -- --check`, `cargo clippy --all-targets`,
`cargo test --lib learning::portability` while iterating, then the CI command
`cargo test --locked --features bindings-export --lib --bins --tests --no-fail-fast`
once at the end (build-cache rule), `npm run bindings:check`.
Architecture job: `bash scripts/check-repository-barrier.sh`,
`bash scripts/check-rust-layer-boundaries.sh` (feature-edge counts must not rise),
`cargo test --locked --manifest-path scripts/data-contract-check/Cargo.toml`,
`python3 scripts/check-sql-contracts.py` (new SQL must prepare against the
migrations; the QueryBuilder paths are exercised by the round-trip tests),
`python3 -m unittest scripts/test_tauri_command_inventory.py`,
`python3 scripts/check-tauri-command-inventory.py`.
Frontend: `npm run type-check`, `npm run type-check:e2e`, `npm run contracts:check`,
`npm run lint`, `npm run test:coverage` (CI enforces coverage floors; `test:unit`
is not the CI command), `npx vite build`, `npm run test:e2e` for the updated
studio spec.
