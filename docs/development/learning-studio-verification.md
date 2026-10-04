# Learning Studio verification and release evidence

Learning Studio records study activity and outcome evidence. It does not claim
that a learner has mastered a subject, is ready for employment, or will retain
material. A release result is valid only for the exact commit, platform, model,
and optional runtime named in its retained artifacts.

## Reference collection MVP — 2026-10-04

Learning Studio now uses the same immutable reference collection for lesson
writing, claim checking, and the “Search related saved material” inspector.
The configured embedding model supplies persisted passage vectors; keyword BM25
and cosine rankings are combined with reciprocal rank fusion. The MVP reuses
`learning_source_retrieval_index`, with an exact scan of the course vectors,
rather than placing private historical snapshots in global document search.
The lesson writer retrieves for its objective; the verifier independently
retrieves for every extracted claim. Ranking scores are not correctness scores.

New course creation retains complete captured source text instead of only its
outline excerpts. Library documents must finish indexing before capture. Web
references use the existing safe DNS/redirect/body-limited reader with a separate
two-million-character allowance; a short chat cache cannot masquerade as a whole
book. The reference path does not use the bounded browser-display fallback.
Reference extraction keeps headings, ordered lists, code indentation, and table
rows. Imports are one page per URL; there is no website crawler or automatic
source selection. The extraction can still omit information in images, complex tables,
or dynamic pages. Review the saved source text before relying on it.

Bounds: two million characters per source, twenty million per retrieval collection,
and the existing source-count limits. Old excerpt-only or truncated active
sources block new background lesson preparation until replaced with complete
material. Topic-only outlines are allowed; lesson preparation requires references.
Completed per-source indexes are reused. Interrupted indexing cannot publish a
partial version; that version is retried, while completed versions are reused.
Changing the embedding identity or passage layout rebuilds its cached vectors.
If no embedding model is ready, the explicit keyword fallback remains available.
An inference error during indexing/querying fails the operation rather than
recording a successful hybrid check.

The production CSV branches have been removed. The CSV example remains only a
regression fixture. Python and JavaScript worked examples still run in restricted
runtimes. Other languages, including Rust, receive evidence checks, with an
explicit “not compiled or executed” disclosure in the lesson report.

To try the MVP:

1. Run the current desktop development build (`npm run tauri:dev`).
2. In Studio, create a focused course. Add an indexed library document or a
   reference URL in Materials. For a larger Rust collection, use the official
   [Rust Book print page](https://doc.rust-lang.org/book/print.html). A narrower
   goal such as ownership and borrowing is easier to evaluate first.
3. Accept the outline. In Sources, inspect the saved text and use Search related
   saved material to check that relevant passages can be found. You can also add
   pasted text, individual recipe pages, or biology reading here.
4. Prepare a lesson. Open **View lesson evidence** above its teaching sections.
   Inspect the claim, supporting quote, retrieved passages, original source
   version, checker, retrieval mode, and execution limitations.
5. Add/adopt/delete a source, then reopen the report: it should identify the changed
   collection. In-flight publication rejects any changed active source set. The
   old lesson retains its historical evidence; automatic re-verification is not
   part of this MVP.

The public evidence DTO omits assessment claims, private answer keys, and their
rationales. Existing or imported lessons without a report are explicitly labeled.
Reports remain bound to lesson content and full source hashes, using
`lesson-evidence-v2`. The author and checker still use the configured course model;
independently configured judges, human-calibrated accuracy measurements, curriculum
coverage evaluation, general website import, and additional executable validators
remain follow-up work. These are reliability mechanisms, not a correctness guarantee.

Verification results for this MVP:

- Learning backend suite: 130 passed; six opt-in tests skipped in the suite.
  Focused retrieval and verifier tests were rerun after final fixes.
- Web-service tests: 75 passed, three opt-in tests skipped. Existing chat verifier:
  50 passed, one opt-in test skipped.
- Learning frontend: 154 tests passed. Chromium and WebKit each passed the evidence
  report journey at desktop and narrow viewport widths.
- The opt-in live Rust Book fetch passed separately: 1,333,339 characters, no
  truncation, final appendix present, using the production reference reader.
- TypeScript, IPC contracts, desktop command permissions, SQL contracts, Rust
  architecture boundaries, formatting and strict library Clippy passed.

Retained logs and source hashes are under
`e2e-results/learning-reference-mvp/2026-10-04/`. Browser fixtures validate the UI;
deterministic embeddings and model fixtures validate retrieval, repair, and
publication boundaries. The live fetch verifies acquisition, not model quality.
No live-model lesson-generation or factual-accuracy evaluation was run for this MVP.

## Evidence-gated lesson preparation — 2026-10-04

The first content-verification slice reuses the chat claim checker and passage
ranker under a strict lesson policy. Preparation now extracts and audits claims,
checks immutable full-text evidence, executes complete Python/JavaScript worked
examples, and rechecks one evidence-driven repair. Both publication paths require
a revision-bound, backend-only report in the same transaction as the ready state.
The report includes source hashes, coverage findings, checker identity and runtime
observations; it contains private answer-key claims and is not a learner DTO.

Verification on the local working tree:

- Learning Studio library suite: **129 passed**, five optional container checks
  skipped. This includes six new verification regressions and real Python/WASI
  and JavaScript execution.
- Existing chat verification suite: **50 passed**, one optional live-model check
  skipped. Legacy chat judgment and lexical fallback behavior remain covered.
- CSV regression: a scripted checker rejects the false whitespace claim, the
  repair receives captured evidence and independently authored runtime fixtures,
  and the corrected revision passes rechecking and persists with its report.
- Missing/invalid evidence, incomplete claim coverage, fabricated quotes,
  truncated and uncertain judgments, changed content, inactive sources and
  absent reports block publication. Failed publication preserves the revision.
- SQL contracts: 1,226 statements prepared against 32 migrations; 74 dynamic
  fragments remain covered through repository tests. Rust layer boundaries,
  formatting, strict library Clippy (`-D warnings`), and patch whitespace checks passed.

Logs and source hashes are retained under
`e2e-results/learning-verification/2026-10-04/`.

Historical first-slice behavior included automatic Python CSV reference acquisition.
The reference collection MVP above removes that topic-specific behavior. General
topic/claim research and independent checker-model selection remain follow-up work;
all topics require suitable saved references. Existing ready/imported lessons are not
retroactively certified. These deterministic model fixtures establish pipeline
and persistence behavior, not live-model factual accuracy or repair quality on
unseen lessons; the live source-acquisition path and held-out evaluations still
need separate evidence. See the
[implementation scope and research plan](../design/2026-10-04-learning-content-verification.md).

## Interactive teaching and placement review — 2026-10-04

The teaching loop now includes saved, in-lesson guided exercises, progressive
hints, critique, task-specific rubric feedback, and revisions that retain the
original submission. Revision sessions copy the original task, rubric, sources,
and answer, expose previous feedback, and remain assisted practice.

Curricula now persist prerequisite links and structured project milestones.
Lesson authoring receives the course sequence and recent lesson recaps. Curriculum,
lesson, placement-task, and assessment-item authoring use an additional AI review
with one repair and re-review. Schema violations and short teaching blocks also
enter the repair path. Review is calibrated to the authoring stage: outlines do
not need finished exercises, and later lessons may rely on earlier learning.
The final review receives the original findings and checks their resolution plus
concrete newly introduced errors; optional new suggestions do not become
publication blockers. Unresolved blocking defects prevent publishing the candidate.
Lesson multiple-choice keys receive an additional blinded check: the model solves
the questions without seeing the proposed keys or explanations. Disagreements
and ambiguous options enter the same bounded repair, then are checked again.
Every lesson section also requires a review finding with an exact quote from its
body. Missing sections and invented review quotes are rejected, and a section
marked defective enters repair even when the overall issue list is empty. The
same factual and product-capability checks apply after repair. This remains an AI
check, not a guarantee of correctness.

Optional placement uses short performance tasks with private keys, autosaved
answers, revision conflict recovery, and feedback tied to exact submitted text.
Recommendations suggest study or an optional module challenge. Challenges can be
authored from accepted objectives before lesson preparation. The next-step control
uses unfinished work, placement, missed outcomes, newer evidence, guided and
independent submissions, and due recall; it never silently completes lessons.
The lesson-preparation job path accepts an explicitly selected unfinished lesson
outside course order, preserves ordered batches, and follows a stable retry chain
after a failure or interruption.

A live Qwen feedback matrix exposed an uncertain response receiving numeric zeros.
The grader now requires null scores for uncertain judgments and the backend strips
numeric scores from uncertain results. The retained initial failure is useful
regression evidence, not a passing model-quality result. The first complete-course
trial also exposed an overly broad focused-course goal and a reviewer confusing
outline requirements with prepared lesson content. The reviewer context was fixed,
and the revised synthetic goal explicitly scopes a small CSV expense summarizer.
Agent inspection and local Python execution of the first prepared lesson also found incorrect question keys
and an example whose leading newline changed its output. The general critique
missed these, motivating the blinded key check and explicit example tracing.
The initial harness disabled model reasoning; it now forwards the same reasoning
controls as the production llama.cpp adapter. A retained reasoning-enabled probe
identified both incorrect keys in that lesson. The earlier non-reasoning runs
remain labeled as such and cannot establish production model quality.

With production reasoning controls, the initial HTTP-based feedback matrix passed: correct work
scored 8/8, incorrect work 0/8, partial work 6/8, and uncertain work had null
scores. The orienting hint did not reveal the solution. These are four synthetic
answers and one hint, not a broad grading benchmark. A full-lesson request then
exceeded the former three-minute timeout. Material requests now allow five
minutes, lesson preparation allows fifteen minutes including review and repair,
and the command wait allows the worker to finish or report its failure.
The harness now delegates to the actual llama.cpp streaming/retry adapter rather
than duplicating its HTTP transport. The first full lesson completed through that
adapter in 218 seconds. Its general review exhausted the initial 3,000-token
allowance before producing a judgment, so that review now requests up to 6,000
tokens within the provider's configured ceiling. Blinded checks also reject
impossible question premises; the quality review rejects unsupported promises
that written tutoring will execute code or provide external expert review.

The first actual production-adapter grading run rejected abbreviated evidence
quotes that used ellipses. Grading now explicitly requests continuous exact
quotes, budgets output for the rubric size, and makes at most one targeted repair
before rejecting invalid feedback. Deterministic checks cover successful evidence
repair and repeated invalid feedback; uncertain judgments still lose all numeric
scores. The subsequent production-adapter matrix passed all four cases (8/8,
0/8, 6/8, and null scores) and the orienting hint in 95 seconds. The source
configuration and saved application settings were not modified.

The nine-lesson live course is **not a passing end-to-end result**. The initial
review accepted a first lesson that still incorrectly described whitespace
handling and memory usage, and promised that the written tutor would run code.
The run was stopped during lesson two after these defects were identified. Its
saved first lesson is retained as failure evidence, not approved learning
material. That finding motivated the section-by-section review above and a
separate opt-in retained-lesson regression. Full-course completion, the final
checkpoint, and a native learner journey remain unverified for this version.

The retained-lesson regression also **failed** (236 seconds). The stricter review
identified header-whitespace and tutor-execution defects, but duplicated a section
index, omitted the recap, and reformatted several quotes. The backend rejected
that invalid review before repair or publication. The model additionally claimed
that empty CSV lines produce spurious rows; local Python execution shows that
`DictReader` skips empty data lines. It also missed the false memory-use claim.
These findings establish a remaining model-quality limitation. Requiring section
coverage detects incomplete review output; it does not make a model's factual
judgments reliable. This run is retained without retrying until it passes, and no
full-course or Coursera-level content-quality claim is supported by these checks.



Deterministic and native validation:

- All 151 component tests passed; the additional submission-conflict test also
  passed (152 unique component tests across the retained runs).
- All 42 Chromium/WebKit journeys passed, including guided attempts, placement,
  390px layouts, source selection, drafts, retry, and renderer restart.
- The final Learning Studio Rust suite passed 123 tests; five optional container tests
  remained excluded.
- A fresh isolated macOS app bundle passed all eight native tests,
  including real Learning Studio IPC, migrations, file access, and close/reopen
  persistence. Bundled Python and JavaScript runtime checks also passed.
  A run of the rebuilt bundle first timed out on the reopened Journal title's
  visibility check, despite the retained screenshot and database containing the
  saved content. One confirmation run passed all eight checks. Both results are
  retained; the intermittent visibility failure was not reproduced or explained.
  That native bundle predates the final generation/review changes, which are covered
  by the subsequent Rust checks and provider probes rather than that UI run.
- Application and browser TypeScript checks passed after bindings regeneration.
  ESLint, IPC contracts, SQL statement checks, command inventory, and patch
  whitespace checks passed.
- The native suite validates integration and lifecycle behavior; its course
  commands use an empty test library. Live course authoring below uses production
  services and a separate SQLite database, not a full native learner journey.

Logs and reviewed screenshots are retained under
`e2e-results/teaching-course-review-2026-10-04/`.

The opt-in provider harness reads an explicitly selected OpenCode configuration,
passes configured credentials only through sensitive HTTP headers, and records
synthetic prompts, outputs, model identity, latency, and token usage. Ordinary
tests never contact this endpoint. Reproduce with a provider-compatible config:

```bash
LATTICE_TEACHING_CONFIG_PATH=/path/to/opencode.json \
LATTICE_TEACHING_EVAL_DIR=/path/to/new-evaluation-directory \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --test teaching_course_evals \
  -- --ignored --nocapture --test-threads=1 --skip live_retained_lesson_review
```

For an interrupted retained evaluation, `LATTICE_TEACHING_RESUME=1` resumes the
same saved outline and skips already prepared lessons. Optional
`LATTICE_TEACHING_REPLAY=1` reuses only successful production-adapter calls whose
model, messages, reasoning effort, and output ceiling match exactly; each reuse
is printed in the log. Both are evaluation-only controls. Ordinary fresh runs
contact the provider for every authoring and review stage.

The separate retained-lesson regression requires `LATTICE_TEACHING_REVIEW_CALL`
to point to a synthetic `call-NNN.json` containing an authored lesson. Run only
`live_retained_lesson_review` with `--exact --ignored --nocapture`, the provider
config, and the evaluation directory. It uses the production review/repair path
and retains the reviewed lesson separately; it does not overwrite the course or
mark a failed full-course run as passed.


The full suite attempts to prepare every lesson in a synthetic course, authors starting-point
tasks, grades correct/incorrect/partial/uncertain responses, checks an orienting
hint, then attempts to author and persist a written/application checkpoint. The
checkpoint requires a successfully completed course; it is not implied by a
passing grading matrix. Raw answer-key
artifacts are explicitly named private. These checks do not measure human learning
outcomes or constitute independent subject-expert review.

## Course authoring and Space selection review — 2026-10-04

The creation form and backend previously required a document or URL. The picker
read all Library documents without an explicit Space, and lesson generation was
limited to three teaching blocks and six multiple-choice questions. Written
practice also discarded lesson-specific assignments in favor of a generic prompt.

Creation now supports a topic without documents, with optional materials selected
through the same Space-scoped document API used by Chat. The picker starts in the
currently selected Space (General when none is selected), searches that Space,
and retains visible, removable selections across Spaces. Only checked IDs are
sent for acquisition. The same picker is used when adding materials to an
existing course. A failed acquisition of explicitly selected material never
silently falls back to general knowledge.

The authoring contract distinguishes topic-based AI content from externally
sourced material. Topic-only content must not invent citations; source-backed
content still needs validated quotes. This distinction continues through lessons,
written practice, practical activities, checkpoints, and recall. Generated recall
without external references quotes the prepared lesson itself and retains empty
external source IDs.

Course depth is enforced by the backend: focused courses have 2–3 modules with
2–3 lessons each, complete courses have 4–6 modules with 3–5 lessons each, and
deep dives have 6–10 modules with 4–6 lessons each. Older clients retain their
existing 2–6 module bounds. Session length controls lesson size independently.

New lessons require 8–12 sections: at least two explanations, two worked examples,
guided practice, an independent assignment, reflection, and a recap. Teaching and
practice sections have minimum content lengths. The independent assignment is
frozen as the actual written-practice task. A full syllabus, lesson section links,
module milestones, written/applied checkpoints, and prefilled project/capstone
briefs connect the existing workspaces. Existing prepared lessons remain readable.

Verification on the working tree:

- Rust Learning Studio suite: 113 tests passed; five optional container checks
  were not rerun. New integration coverage exercises topic-only creation,
  preparation, saved assignments, recall acceptance, checkpoint creation and
  form startup, depth bounds, rejected thin lessons, and invalid citations.
- Learning Studio and section error-boundary component suite: 20 files, 140 tests
  passed in a serial run, including saving generated recall without external sources.
  An existing recall test timed out under parallel load and passed in isolation
  and in the complete serial run.
- The initial Chromium/WebKit suite passed 33 of 36 checks. Two layout checks
  exposed excessive course-map height at 800×600, which was corrected with a
  compact expandable map. One Canvas retry click raced with an autosave.
- All eight follow-up browser checks passed, covering the corrected layout,
  Canvas retry, topic-only creation, and new 390px creation journeys in both
  browsers. Reviewed screenshots include the builder and active workspace.
- ESLint passed for the Learning Studio components and browser fixture/spec.
- TypeScript checks passed for the application and browser tests.
- API bindings were regenerated and their consistency check passed.
- IPC contract guard, Rust formatting, and patch whitespace checks passed.

Logs and reviewed screenshots are retained under
`e2e-results/study-course-review-2026-10-04/`.

These are deterministic renderer, model-fixture, and real SQLite checks. Live-model pedagogical
quality, native packaged-app execution, and learning efficacy were not measured
by this review; course length and section validation alone cannot establish them.

## Studio usability and draft safety review — 2026-10-02

The local working tree based on `c25d7fb55062e459ba2692783085583215b2fd9f`
completed the following follow-up checks on macOS Apple Silicon. These results
cover the durable lab editor and Studio navigation changes, with additional
visual checks after the final theme-color adjustment.

| Boundary | Result |
|---|---|
| Full frontend Vitest with enforced coverage | 174 files, 1,331 tests passed |
| Learning Studio renderer coverage | 80.40% statements, 70.60% branches, 79.73% functions, 88.65% lines |
| Whole-renderer coverage | 54.06% statements, 48.95% branches, 49.12% functions, 54.95% lines |
| Complete renderer Playwright | 80 checks passed across Chromium and WebKit, with no retries or skipped checks |
| Final layout and theme verification | 4 additional browser checks passed after the last color adjustment |
| Rust Learning Studio library | 108 passed; 5 optional container checks were not rerun |
| Rust draft persistence | Revision, replay, private-file rejection, and file-backed SQLite close/reopen checks passed |
| TypeScript, E2E TypeScript, ESLint | Passed |
| Rust formatting and strict library Clippy | Passed with warnings denied |
| Generated bindings and command/SQL/IPC contracts | Passed; 353 registered commands and 27 migrations checked |

Lab edits now save in SQLite independently of execution. Tests exercise edits
during loading and saving, operation replay after a lost response, stale-revision
conflicts, both conflict recovery actions, undo back to saved content, activity
isolation, and reopening the database. Pending saves gate activity creation,
activity switching, program navigation, and route exits. Failed saves preserve
the editor and provide a retry path. No draft save is counted as a code run.

The editor provides local C#, Rust, Python, JavaScript, TypeScript, JSX/TSX,
and JSON syntax support, line numbers, indentation, search, undo, and a Run
shortcut. Browser journeys use the real CodeMirror editor and verify an escape
from its Tab-indentation behavior. Runnable lab creation requires an explicit
execution environment; review-only projects remain available. The activity
dialog traps focus, closes with Escape, and restores focus to its opener or the
requested environment setup panel.

Five primary navigation tabs group the full workspace, with practice activities
in a secondary row and Plan, Canvas, and Import & export under More. Module and
lesson selectors wait for saves before changing context. Canvas and all prior
workflows remain covered by the browser suite. Final visual review covered
1280×800, 800×600, and 390×844 in both browsers; the first lesson card starts at
410 pixels at the two desktop sizes. Tests inspect inner scroll containers as
well as the page, so a clipped app shell cannot hide horizontal overflow.
Search controls and primary buttons are checked for at least 4.5:1 text contrast
in light and dark themes.

Logs, the complete browser result, coverage totals, and reviewed screenshots are
retained under `e2e-results/studio-polish-2026-10-02/`. Browser journeys use a
deterministic stateful IPC fixture; the separate Rust tests exercise real SQLite
and migrations. This follow-up did not rerun the packaged native, live-model,
or real-container checks. Their earlier evidence and remaining platform limits
are recorded below.

## Earlier runtime integration evidence — 2026-10-01–02

The integrated local working-tree candidate completed these deterministic checks
on macOS Apple Silicon:

| Boundary | Result |
|---|---|
| Rust Learning Studio library | 106 passed; 5 optional container tests also executed separately below |
| Existing Study library | 11 passed |
| Article extraction and its boundary tests | 16 passed, including the shared QuickJS dependency change |
| Evaluation integration | 2 passed, 1 ignored opt-in live-model benchmark |
| Frontend Vitest with enforced coverage | 171 files, 1,305 tests passed |
| Learning Studio renderer coverage | 80.49% statements, 69.79% branches, 79.08% functions, 88.68% lines |
| Whole-renderer coverage | 53.50% statements, 48.32% branches, 48.49% functions, 54.36% lines |
| Complete renderer Playwright | 70 checks passed across Chromium and WebKit |
| Rust formatting and strict library Clippy | Passed with warnings denied |
| TypeScript, E2E TypeScript, ESLint | Passed |
| Generated bindings and command/SQL/IPC contracts | Passed |
| Real Docker execution | All 4 language presets accepted correct and rejected incorrect solutions |
| Container containment | Passed: protected evaluator, read-only host input/root, bounded writable guest storage, network isolation, output cap, cancellation cleanup, and missing-image status |
| Installed Python and JavaScript | Passed correct/incorrect exercises and host-access checks inside the signed application |
| Packaged macOS native UI | Passed: 9 recorded checks across startup, IPC, file access, editing, close/restart persistence, and resource churn |

The Rust library run includes 133 passing tests across Learning Studio, Study,
and article extraction. The five real-container tests ran from that same freshly
built test executable with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, exercising engine
discovery with a typical desktop launch environment. Docker's CLI and credential
helper directories are resolved for the child process without a login shell or
global environment changes. No container daemon is needed for the built-in
Python and JavaScript providers.

Completed check logs are retained under
`e2e-results/learning-runtime/2026-10-02/`. The installed-runtime result includes
the exact candidate identifier, executable, resource directory, architecture,
and profile. The packaged UI result is also retained at
`e2e-results/desktop/reports/1790917832494/result.json`; its 25-cycle resource
workload grew sampled resident memory by approximately 0.10 MiB, below the
64 MiB regression budget. This is evidence for that workload, not proof of zero
leaks. These are isolated, instrumented debug candidates, not notarized shipping
installers.

Earlier candidate evidence remains available for the CPU sidecar's tiny-model
execution (`e2e-results/desktop/reports/cpu-generation-local.json`) and the
allocator diagnostic (`e2e-results/desktop/reports/1790910887552/`). Those checks
were not repeated for this runtime integration. The opt-in live-model benchmark
has no configured endpoint and remains unverified. The Windows/Linux packaged
matrix and manual assistive-technology review also remain release evidence to
collect; configured CI jobs are not passing results.

## Fast and interactive test workflows

Use the focused commands while developing Learning Studio:

```bash
# Automated component and interaction tests
npm run test:learning:unit

# The same unit tests in Vitest's interactive browser UI
npm run test:learning:unit:ui

# Automated production-renderer journeys in Chromium and WebKit
npm run test:learning:e2e

# Step through, inspect, and replay those journeys in Playwright UI
npm run test:learning:e2e:ui
```

`npm run test:coverage` runs the complete renderer unit suite and enforces both a
whole-renderer baseline and stronger Learning Studio floors. The Studio floors are
77% statements, 67% branches, 75% functions, and 87% lines. CI runs this command
instead of an unenforced unit-test pass and retains the text, JSON summary, LCOV,
and HTML reports as the `frontend-coverage` artifact. Failed tests still produce a
report when the runner reaches coverage collection.

## Deterministic release gates

Run these from the repository root after generating bindings:

```bash
npm run bindings:generate
npm run bindings:check
npm run contracts:check
npm run contracts:commands
npm run contracts:sql

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
SQLX_OFFLINE=true cargo check --manifest-path src-tauri/Cargo.toml --lib
SQLX_OFFLINE=true cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --lib features::learning::
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test learning_studio_evals

npm run type-check
npm run type-check:e2e
npm run lint
npm run test:coverage
npm run test:learning:unit
npm run test:learning:e2e
```

The Rust Learning Studio tests use the real migration set and SQLite foreign
keys. They cover replayed operation IDs, compare-and-swap conflicts, immutable
history, answer-key isolation, source/version ownership, assessment uncertainty,
generation interruption, pack validation, scheduler replay, and practical-run
recovery. The browser suite uses a strict stateful IPC fixture: unsupported
commands, unexpected external requests, console errors, page errors, and
narrow-screen overflow fail the journey. Its stateful journeys cover saved
assessment responses with pause and fresh submission, curriculum preview and
acceptance, an explicitly unavailable lab runtime, and recall card versioning,
review, scheduler selection, and duplicate decisions. Assessment grading is
shown as unavailable in the fixture; these journeys do not claim model grading.
Browser fixtures validate renderer behavior; they do not establish native
service safety.

The packaged native suite exercises the real Tauri plugin registration and
migrated database as part of its isolated install, close, and restart journey:

```bash
npm run test:desktop:build
npm run test:desktop
```

This build is intentionally separate because it produces and launches an
instrumented application bundle. See `e2e/desktop/README.md` for platform setup,
isolation, and the CI evidence boundary.

The build helper verifies the bundled CPython interpreter and standard-library
hashes, then executes Python and JavaScript from the installed executable. On
macOS this exercises the signed hardened-runtime binary, including its Wasmtime
executable-memory entitlement. The fixed probe requires correct solutions to
pass, incorrect solutions to fail, and host access to remain unavailable. Its
report is `e2e-results/desktop/reports/embedded-runtime.json`. The native UI suite
also verifies the actual runtime catalog IPC and installed resource path.

## Language execution coverage

Python standard-library exercises run in the bundled CPython WASI interpreter;
JavaScript module exercises run in QuickJS. Both work without Docker, a local
language installation, or a network connection. Tests execute real learner code
and cover failed assertions, timeouts, cancellation, bounded output, and denied
host access. JavaScript tests also exercise memory exhaustion. Python compiles
its trusted interpreter once per application process; that initialization does
not consume the learner's exercise time limit.

C#, Rust, and React/TypeScript use optional Docker or Podman environments prepared
from the app's catalog. Preparation can download toolchains; actual exercises
run offline against the recorded immutable image ID. C# supports package-free
.NET projects, Rust uses the standard library, and React includes pinned React,
TypeScript, jsdom, and Testing Library dependencies. React exercises type-check
and run component interaction tests; a live visual preview is not implemented.

Run the real Docker catalog checks after starting Docker:

```bash
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --lib features::learning::runtime_catalog::tests::docker_ \
  -- --ignored --test-threads=1
```

Each language executes both a correct and an incorrect solution through the
production container runner. The React check renders a component and clicks its
button. These tests prepare missing images and therefore may download them.
The Ubuntu CI job runs these checks and the containment smoke test below.

## Opt-in contained-runtime smoke test

Ordinary tests inspect the exact Docker/Podman argv without requiring either
runtime. The opt-in smoke test uses an already-local, immutable image and never
pulls. The selected image must provide `/bin/sh` and `cat`.

```bash
LATTICE_LAB_SMOKE_ENGINE=docker \
LATTICE_LAB_SMOKE_IMAGE_ID=sha256:<64-lowercase-hex-digest> \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --lib \
  features::learning::lab_runtime::tests::opt_in_container_smoke_proves_runtime_containment_and_read_only_checks \
  -- --ignored --nocapture
```

The smoke test verifies that the learner can write to the bounded temporary
workspace inside the container while host input files remain unchanged.
Evaluator files cannot be changed through their read-only mounts, the container
root cannot be written, effective capabilities are empty, `NoNewPrivs` is set,
and the isolated network namespace exposes no `eth0`. It then cancels a busy
run and verifies that the deterministically named container no longer exists.
Production execution also
sets a wall-clock timeout, memory plus swap ceiling, CPU ceiling, process and
file-descriptor limits, bounded output, a read-only root filesystem, no image
pull, no network, and a deterministic container name for cancellation/recovery.

When a container exercise has no available Docker or Podman engine, its recorded
status is `runtime_unavailable`. Built-in Python and JavaScript practice,
authoring, and artifact review remain available.

## Opt-in live-model benchmark

The committed benchmark contacts no model during an ordinary test run. Configure
an Ollama-compatible local or private endpoint explicitly:

```bash
LATTICE_LEARNING_EVAL_MODEL=qwen3:8b \
LATTICE_LEARNING_EVAL_ENDPOINT=http://127.0.0.1:11434 \
LATTICE_LEARNING_EVAL_LOG_DIR="$HOME/lattice-eval-results" \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --test learning_studio_evals -- --ignored --nocapture
```

Optional authentication uses
`LATTICE_LEARNING_EVAL_AUTH_HEADER_NAME` and
`LATTICE_LEARNING_EVAL_AUTH_HEADER_VALUE`. The credential is passed to the
client and is never written to the trace.

The benchmark calls the production open-response grader with correct,
incorrect, ambiguous, and incomplete synthetic attempts. It records rubric
agreement, exact submitted-text support for numeric scores, false-confidence
count, latency, and peak observed process RSS. It then calls the production
practical-authoring pipeline, including independently generated hidden checks,
and records frozen-source grounding, public/check file counts, a withheld
solution canary, latency, and RSS. Each record is appended before an assertion,
so a failing run remains reviewable at
`lattice-learning-studio-evals-<pid>.jsonl`.

Retain the JSONL file with the model identifier and candidate commit. A passing
run establishes only that this model followed these bounded synthetic contracts
on that run. Cancellation, crash recovery, pack restoration, and operation
replay remain deterministic repository/runtime gates rather than model-quality
metrics.

## Platform behavior

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Programs, lessons, assessments, notes, Canvas, recall, sources, packs | Supported by the local Tauri/SQLite application | Supported by the local Tauri/SQLite application | Supported by the local Tauri/SQLite application |
| Built-in Python and JavaScript execution | Bundled interpreters; no Docker dependency | Same implementation; packaged CI required | Same implementation; packaged CI required |
| Container lab authoring and artifact review | Available without a runtime | Available without a runtime | Available without a runtime |
| Container execution | Optional Docker or Podman capability, probed at use time | Optional Docker or Podman capability, probed at use time | Optional Docker or Podman capability, probed at use time |
| Unsupported runtime state | Shown as unavailable; no evidence event is created | Shown as unavailable; no evidence event is created | Shown as unavailable; no evidence event is created |
| Pack import | Checksum and conflict preview required before writing | Same | Same |

The repository command builder and migration tests are portable. Cross-platform
support claims still require a green packaged-desktop matrix for the candidate
commit. Windows Server CI does not replace a Windows 11 clean-machine run, and
Linux package extraction does not test every distribution's container setup.

## Accessibility review

Automated journeys run both Chromium and WebKit at desktop and 390-pixel widths.
They use role/name locators, keyboard focus assertions, live status/error regions,
and overflow checks. Before tagging a release, retain a manual review record for:

- complete keyboard traversal, visible focus, modal focus entry/return, and
  logical focus order at 100%, 200%, and 400% zoom;
- VoiceOver on macOS plus NVDA or Narrator on Windows for headings, landmarks,
  form labels, error recovery, assessment disclosure, and card reveal state;
- light and dark themes, high-contrast settings, reduced motion, and text-only
  comprehension of Canvas through its written outline;
- 390-pixel layout with long titles, file paths, source quotes, and translated
  date/number strings.

Record failures with the platform, browser/webview, assistive technology, exact
control, and screenshot or trace. An unchecked manual item is unverified; it is
not a passing result.
