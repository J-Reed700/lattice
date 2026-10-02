# Learning Studio verification and release evidence

Learning Studio records study activity and outcome evidence. It does not claim
that a learner has mastered a subject, is ready for employment, or will retain
material. A release result is valid only for the exact commit, platform, model,
and optional runtime named in its retained artifacts.

## Current candidate evidence — 2026-10-01–02

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
