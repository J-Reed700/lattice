# Learning Studio completion plan

## Execution decision — October 1

Learning Studio remains a subject-neutral local module. Programming execution
uses two providers selected explicitly when an activity is created:

| Environment | Execution | Intended scope |
| --- | --- | --- |
| C# | Optional Docker/Podman, Microsoft .NET SDK | Compile real C# projects and run independent checks; the first setup card |
| Rust | Optional Docker/Podman, official Rust image | Compile Rust and run its test harness |
| React / TypeScript | App-prepared Node image with pinned dependencies | Type checking and React component tests without network during a run |
| Python | Bundled CPython compiled to WASI, hosted by Wasmtime | Standard-library exercises, read-only supplied files, no ambient host access |
| JavaScript | Embedded QuickJS | ECMAScript modules and algorithms without browser, Node or host bindings |

Python also has an optional container preset. Runtime support never gates notes,
flashcards, lessons, quizzes, simulations or other subjects. Setup starts from
the environment chooser; Lattice owns image selection, preparation and immutable
image resolution. Generated material receives the actual language, command and
supported-dependency contract before generating the brief and independent checks.
The model does not select images or execution commands.

The backend keeps evaluator files, validates all supplied paths, stores the
learner's exact submitted file snapshot and persists execution results separately
from AI feedback. Embedded runtimes reuse those contracts rather than accepting
an unverified pass/fail report from the renderer. Historical container attempts
retain their original image and command. Imported activities remain review-only
because learning packs do not contain hidden evaluator files.

Verification must include actual correct and incorrect programs for each runtime,
timeouts, cancellation, output bounds, unavailable Docker, setup retry, and
Chromium/WebKit UI journeys. Mock command construction alone is insufficient:
real Docker execution exposed an invalid `--mount` option and a `noexec` build
output path. These are covered by the real preset smoke tests.

Implementation references: [Wasmtime security](https://docs.wasmtime.dev/security.html),
[CPython WASI build](https://github.com/python/cpython/tree/3.14/Platforms/WASI),
[QuickJS embedding](https://bellard.org/quickjs/quickjs.html),
[Docker run](https://docs.docker.com/reference/cli/docker/container/run/).

The embedded Python interpreter and standard library ship as application
resources, built from checksum-pinned CPython 3.14.8 sources. End users do not
install Python, Wasmtime, or a package manager. JavaScript uses an isolated
QuickJS context. Both use bounded memory/output, cancellation, and a shared
two-guest limit. Python exposes only read-only exercise and interpreter files;
JavaScript has no filesystem or network bindings. Initial Python compilation
runs off the UI thread and is excluded from the exercise time allowance.
Wasmtime is pinned to 48.0.3, including its
[September security fixes](https://github.com/bytecodealliance/wasmtime/releases/tag/v48.0.3).

Container preparation downloads app-owned toolchains once. Each run freezes
the resolved image ID, command, evaluator, and learner files; writes happen in
bounded tmpfs, and host input files are mounted read-only. C# uses .NET 10,
Rust uses standard-library tests, and the React/TypeScript image includes pinned
React, TypeScript, jsdom and Testing Library dependencies. React currently
provides type checks and component interaction tests, not a visual live preview.
Dependency installation and arbitrary project package restoration during a run
remain outside these offline execution contracts.

Embedded exercises also freeze an interpreter contract version. After an
incompatible runtime upgrade their saved results stay readable, and the app
requires a newly generated activity rather than silently changing its execution
environment. Imported exercises remain review-only until regenerated, since
their hidden evaluator files are intentionally not exported.

Status: slices 5–10 implemented and integrated, 2026-10-01. Deterministic
candidate evidence is recorded in
`docs/development/learning-studio-verification.md`; opt-in and manual release
evidence remains explicitly separate.

The first four production contracts were implemented in the main Learning Studio
design: course workflow, learning memory, visual thinking, and versioned sources.
The six slices below are now implemented as one coherent candidate rather than as
isolated features.

## Slice 5 — grounded practice workbench

Deliver one durable lesson attempt from prompt to evidence. An attempt freezes its
lesson prompt, rubric, and exact source versions; records Explore, Practice, or
Demonstrate conditions; autosaves learner work; and preserves every hint, source
open, critique, mode change, and solution reveal. Tutor claims cite an immutable
source version and an exact quote that the backend validates against saved text.
Submission returns criterion-level provisional feedback, model identity, explicit
uncertainty, and evidence events. Misconception and follow-up suggestions require
learner acceptance.

Gate: restart and lost-response recovery, compare-and-swap conflicts, citation
validation, immutable submitted attempts, Demonstrate aid restrictions, keyboard
operation, narrow layout, Chromium and WebKit journeys, and model-failure recovery.

## Slice 6 — evidence, assessment, and adaptive follow-up

Extend assessments beyond exposed multiple choice. Add short answer, explanation,
ordering, and artifact submissions; versioned blueprints and rubrics; immutable
forms; retake history; cumulative and delayed transfer checks; and an outcome
evidence timeline separating recall, explanation, application, and transfer.
Recommendations are deterministic and explain their reason. Generated variants
are constrained by accepted goals and frozen sources and cannot rewrite prior work.

Gate: answer keys never reach the renderer before submission; interrupted work is
not graded; exposed repeats are labeled; parallel forms preserve blueprint
coverage; grader disagreement and uncertainty are represented; historical results
remain readable after curriculum or source changes.

## Slice 7 — editable programs and durable generation

Make the accepted curriculum editable through visible revisions. Support add,
edit, reorder, replace, skip, and prerequisite challenge operations for upcoming
work. Preserve accepted revisions and display semantic diffs with reasons. Add a
skippable diagnostic, source-coverage gaps, durable generation jobs, cancellation,
retry, crash recovery, and bounded preparation of the next few lessons.

Gate: completed lessons and started assessments are immutable; current position is
stable across preview and plan changes; denominator changes are disclosed; a
cancelled or failed job cannot publish partial curriculum; operation replay cannot
duplicate revisions or prepared lessons.

## Slice 8 — engineering labs and simulations

Add a generic practical-work contract plus an engineering pack. Labs freeze their
instructions, files, runtime image, checks, rubric, and assistance conditions.
The first execution backend is an optional local Docker/Podman capability with no
network, a read-only root filesystem, dropped capabilities, no-new-privileges,
bounded CPU/memory/processes/output/time, a per-run writable workspace, and an
explicit image allowlist. Unsupported hosts retain authoring and review without
pretending execution occurred. Add debugging, testing, code review, incident,
system-design, project revision, and interview simulations on the same attempt
and evidence model.

Gate: cancellation kills the whole run, paths cannot escape the workspace,
untrusted output is inert, checks are independent from generated solutions,
runtime absence is honest, and the command builder plus a real opt-in smoke run
cover the containment flags. Never use privileged containers or mount the Docker
socket inside the learner container.

Docker documents that containers have no resource constraints by default, so the
runner must set them explicitly. It also provides `no-new-privileges`, capability,
network, and read-only controls; those controls are mandatory here:

- https://docs.docker.com/engine/containers/resource_constraints/
- https://docs.docker.com/reference/cli/docker/container/run/

## Slice 9 — portable programs, source completion, and recall evolution

Export and restore a versioned, checksummed program bundle with a dry-run preview,
conflict policy, privacy manifest, selected evidence, independently authored
activities, and only redistribution-safe source material. Add source deletion with
historical tombstones, exact quote selectors, program-scoped semantic retrieval,
and explicit re-import. Expand recall to typed cloze, reverse, code prediction,
and reconstruction cards with duplicate suggestions and card-version history.
Add an FSRS scheduling adapter using default parameters while retaining the full
legacy scheduler and review log; do not optimize personal parameters without
sufficient review history.

Gate: export/import round trips preserve IDs and immutable history, reject path
traversal and checksum mismatch, preview every conflict before writing, avoid
credentials/private unrelated data, and replay existing schedules deterministically.
FSRS stays versioned and reversible. Current crate API research:
https://docs.rs/fsrs/latest/fsrs/.

## Slice 10 — release hardening and evaluation

Exercise the real Tauri command surface and actual desktop webviews, then add an
opt-in live-model benchmark with correct, incorrect, ambiguous, and incomplete
attempts. Measure citation support, solution leakage, false confidence, rubric
agreement, cancellation, crash recovery, export/restore, latency, and peak memory.
Run accessibility checks plus manual keyboard, focus, screen-reader, contrast,
dark/light, and zoom audits. Verify macOS, Windows, and Linux capability behavior
and document unavailable features without synthetic success states.

Gate: deterministic suites remain green; benchmark failures are retained and
reviewable; no unsupported readiness or mastery claim is emitted; all destructive
imports/restores have previews and recovery; focus and keyboard behavior meet the
WCAG 2.2 requirements used by the product:

- https://www.w3.org/TR/wcag/
- https://www.w3.org/WAI/WCAG22/Understanding/focus-order
- https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance

## Shared implementation rules

- The backend owns durable truth. React state holds only unsaved input and view
  selection.
- Every retryable mutation has a stable operation identifier and payload digest.
- Every mutable aggregate uses compare-and-swap revisions.
- Generated content is bounded, schema-validated, source-scoped, and visibly
  identified. Model output never becomes an oracle for its own assessment.
- Submitted attempts, evidence, source versions, curriculum revisions, lab runs,
  and exports are immutable historical records.
- Unknown means unknown. Opening content, time spent, model confidence, or a single
  score does not imply mastery or job readiness.
- Tests use real migrations and strict IPC fixtures. Unknown commands, unexpected
  external requests, console errors, and horizontal overflow fail browser journeys.

## Implementation record

- Slice 5 ships immutable grounded attempts, autosave, aid-event history,
  criterion feedback, citation validation, and explicit uncertainty.
- Slice 6 ships versioned assessment forms and rubrics, saved and submitted work,
  retakes, evidence timelines, recommendation reasons, and answer-key isolation.
- Slice 7 ships revisioned curriculum operations, semantic previews,
  compare-and-swap acceptance, durable jobs, cancellation, retry, and interrupted
  job recovery.
- Slice 8 ships a subject-neutral practical-work contract, engineering activities,
  and an optional Docker/Podman runner with explicit containment and honest
  `runtime_unavailable` results.
- Slice 9 ships checksummed `.lattice-learning` packs with preview, privacy and
  conflict policies, source tombstones and selectors, typed versioned recall cards,
  duplicate decisions, and reversible FSRS 6 scheduling.
- Slice 10 ships strict browser journeys, a packaged-desktop native journey,
  deterministic repository/runtime gates, and an opt-in retained live-model
  benchmark. Manual assistive-technology review, the packaged cross-platform
  matrix, a real contained-runtime smoke run, and a configured live-model run are
  release evidence rather than unfinished implementation slices.
