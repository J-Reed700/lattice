# Renderer journeys

These Playwright tests exercise the production renderer bundle through real routes and controls. A strict, stateful Tauri fixture supplies deterministic backend behavior. Unknown IPC commands fail the test so a new application dependency cannot silently pass through a permissive mock.

## Test layers

| Layer | Command | What it verifies |
| --- | --- | --- |
| Unit and component interactions | `npm run test:unit` | Vitest and Testing Library exercise logic, hooks, components, errors and asynchronous lifecycles. |
| Component integration | `npm run test:integration` | Search, real QueryClient caching, debouncing, stale responses, result grouping and keyboard flows. |
| Renderer coverage | `npm run test:coverage` | All renderer source is counted, including unimported files. Global and feature floors plus individual 100% gates fail the run on regressions. |
| Native unit and integration | `npm run test:rust` | Every non-ignored Rust library, binary and integration test, including real SQLite, file extraction, HTTP fixtures and command contracts. Build the renderer first with `npx vite build`. |
| Browser journeys | `npm run test:e2e` | Production renderer interactions in Chromium and WebKit, with deterministic native command responses. |
| Packaged desktop | `npm run test:desktop:build` then `npm run test:desktop` | Actual Tauri webview, native IPC, filesystem boundaries, pending saves and process restart. See [native prerequisites](desktop/README.md). |

`npm run test:check` runs renderer type checks, lint, IPC contracts and coverage.
`npm run test:all` runs those checks, builds the renderer, runs all Rust and
browser tests, then builds and exercises an isolated native desktop package.
The complete command needs the normal Tauri/sidecar prerequisites and an
unlocked desktop session; Linux needs Xvfb when no display is available. Live
model evaluations and explicitly ignored environment-dependent Rust tests remain
opt-in and are not counted as passed tests.

The coverage report is `coverage/index.html`, with machine-readable results in
`coverage/coverage-summary.json` and `coverage/lcov.info`. Complete coverage is
enforced separately for pending-save barriers, file reads, debounce lifecycles,
batch job wrappers/history, and download progress formatting. The rest of the
app has measured coverage floors, not a claim of 100% coverage. Browser and
native journeys verify different boundaries and do not increase Vitest's
coverage percentage.

Tests use controlled promises and fake clocks to reproduce out-of-order replies,
unmounts, lost acknowledgements, cancellation, stale journal reloads, retained
pins, import timeouts and failure recovery. Explorer history also runs 1,000
repeatable model-based navigation actions. Failed browser runs retain traces,
screenshots and videos, including local runs with retries disabled.

## Quality gates

```sh
npm run type-check:e2e
npm run lint:e2e
npm run test:e2e
```

`npm run test:e2e` runs every `e2e/*.spec.ts` on the `web-smoke` (Chromium) and
`webkit-smoke` projects. Playwright builds its own renderer into
`e2e-results/renderer` and serves it on port 4173 with `--strictPort`, so keep
that port free; it never reuses a running dev server. `npm run test:learning:e2e`
runs only `e2e/learning-studio.spec.ts`.

For an interactive inspector with time travel, DOM snapshots, locators, and manual
step execution, run `npm run test:learning:e2e:ui`. Component-level interaction
tests have their own focused automated and interactive commands:
`npm run test:learning:unit` and `npm run test:learning:unit:ui`.

The Learning Studio notebook and recall journeys run on Chromium and WebKit at desktop and 390px widths. They cover immediate notebook-save flushing, persisted route state, generation failure and explicit recovery, draft editing and acceptance, prompt-first answer/source disclosure, idempotent review retry, and due-count updates.

The Canvas journey also runs on both browsers and widths. It exercises the packaged Excalidraw editor, accessible text outline, immediate navigation with a pending save, session-backed fixture rehydration after a page reload, same-operation replay after a simulated lost save response, named checkpoints, pre-restore history, and JSON export. During Canvas loading and use, the test rejects HTTP(S) requests to any origin other than the local Vite app; `data:` and `blob:` resources remain allowed. It checks for horizontal overflow at 390px and uncaught page errors.

The versioned Sources journey runs on Chromium and WebKit at desktop and 390px widths. It adds a public URL through the shipped sheet, searches saved text, checks unchanged and failed refreshes, creates a pending update, retries a lost response with the same operation ID, verifies pending-version reuse and reload persistence, clears a stale pending pointer when a later check matches the active digest, explicitly adopts the update, changes freshness policy through a compare-and-swap conflict, and reopens the retired version after reload. The strict fixture keeps source versions immutable and verifies source workspace/search isolation between programs. It checks keyboard operation, local-only browser requests, page errors, and narrow overflow.

The Grounded Practice Workbench journey runs on Chromium and WebKit at desktop and 390px widths. It models strict session revisions and immutable submissions, retries a lost autosave response with the same operation ID and payload, checks changed text uses a new operation, verifies Demonstrate mode denies source/tutor/solution assistance, follows all four exact-version-cited hints, gates solution reveal behind confirmation, accepts and rejects tutor proposals, and persists a rubric result across reload while the original source version remains frozen after library adoption. Browser mocks verify renderer command behavior and do not establish native service safety.

The extended Studio journey exercises saved and interrupted assessments, fresh
retakes, answer submission, curriculum revision preview and acceptance, explicit
runtime-unavailable lab behavior, pack preview/cancel/apply, source tombstone and
re-import, recall-card versioning, scheduler switching, review, and duplicate
confirmation. It asserts durable fixture state after the interactions and runs the
same workflow at desktop and 390px widths in both browser engines.

The app-shell suite (`e2e/app-shell.spec.ts`) covers the rest of the shell:
navigation and Settings, PDF import and reopening, library collections, theme
startup and contrast, native quit, chat sidebar and spaces, provider
connections and errors, the model catalog, bulk import and deletion, and
flashcard decks inside Studio. It separately verifies that Studio opens through
the production navigation, renders a complete active course, preserves an
in-progress quick-check answer across check types and workspace tabs, and
exposes saved attempt history. This keeps route and shell integration coverage
independent from the larger strict Studio fixture.

The Explorer suite (`e2e/explorer.spec.ts`) covers keyboard tree navigation,
back/forward history, exact-line search results, empty and failed searches,
empty/binary/oversized/missing files, cancelled folder pickers, permission-error
recovery, folder and thread isolation, persisted tree visibility and reopening.
The navigation journey runs at 1440px and the app's 800px minimum window width
in both engines. Fixture state survives renderer reloads, and every test rejects
unimplemented commands and uncaught renderer exceptions.

To retain full-page screenshots from a successful focused run:

```sh
LATTICE_CAPTURE_E2E_SCREENSHOTS=1 npx playwright test e2e/learning-studio.spec.ts --project=web-smoke
```

Screenshots and traces are written beneath `e2e-results/artifacts/`, which is ignored by Git.

## Test boundary

Renderer journeys do not claim to run native Tauri IPC or prove native network safety. Repository transactions, migrations, command behavior, idempotency, and the native DNS-aware safe-fetch boundary are verified in Rust. The browser fixture models published command contracts, retains backend state across route changes, and exposes operation counters when a UI assertion depends on exactly-once behavior. Browser request observers only establish that the renderer does not fetch source URLs directly; they do not inspect requests made by the native backend.

When extending a journey, prefer role and accessible-name locators, assert persisted fixture state after navigation, include a recoverable failure where the workflow promises recovery, and keep answer keys or other gated content absent from the DOM until the learner reveals it.
