# Release readiness pass (2026-09-19)

This records checks against the working tree, including the existing uncommitted
changes. It is evidence for the tested paths, not a certification that the app is
defect-free.

## Fixes

- Removed a dangling `.dark .dark` selector in `src/index.css`. Vite's development
  server served the app, but production CSS minification failed at the following
  media query, preventing desktop packaging.
- Browser smoke tests now build and serve the production renderer. They use a
  dedicated output directory and a strict preview port, so a concurrent desktop
  build or existing development server cannot change what the tests exercise.
- Added WebKit to the browser matrix alongside Chromium and installed both in
  CI. These are renderer tests with simulated IPC, not native platform tests.
- Formatted the web service changes that previously failed the Rust formatting
  gate, preserving their behavior.
- Fixed stale screen data after background work. The shared query client had
  disabled all refetches on mount, including explicitly invalidated queries.
  WebKit exposed a deck generated away from Study missing on return. The study
  navigation regression test now uses the app's real query defaults and failed
  before the fix. Returning to a screen now refreshes stale/invalidated data;
  fresh data still uses the cache.
- Native acceptance testing reproduced journal data loss on Cmd-Q: an edit
  visibly marked "Saving…" was absent after restarting. The pinned Tao/AppKit
  native Quit path bypasses Tauri's `ExitRequested` event
  ([upstream issue](https://github.com/tauri-apps/tauri/issues/12978)). A macOS
  delegate bridge now defers native termination until the existing renderer-save
  handshake and backend cleanup finish. It also handles cancellation and ignores
  stale responses. This uses AppKit's public termination-delegate API and is
  isolated behind the macOS build flag.
- Shutdown blurs the focused editor field before collecting pending saves, so
  titles that commit on blur participate in the save handshake. A new test failed
  before this fix and passes afterward.

## Completed checks

- TypeScript type checking and ESLint: passed.
- Final frontend unit/integration suite: 126 files, 1,043 tests passed.
- Production Vite renderer build: passed after the CSS repair.
- Production browser flows: all 32 passed (16 Chromium, 16 WebKit), including
  PDF rendering, collections, theme persistence, spaces, provider settings,
  import progress, study generation/review/editing, error recovery, and deletion
  cancellation. This is the final rerun after the background-refresh repair.
  Both browser shutdown checks also passed after the subsequent focused-field
  save repair.
- IPC contract guard: 243 response boundaries, 254 command contracts, passed.
- Generated TypeScript bindings match the backend exports.
- Repository and Rust layer boundaries: passed.
- SQL contracts: 464 statements checked; 49 dynamic fragments require repository
  tests, as reported by the checker.
- Whole desktop crate Rust formatting: passed.
- Strict Rust Clippy for the library and desktop binary (`-D warnings`): passed
  after the native quit repair. Final whitespace/diff check: passed.
- Expanded `cargo test --all-targets`: 3,834 passed, zero failed, 82 ignored,
  across 35 test harnesses (includes 3,248 library tests), before the native quit
  repair. After that repair, the three shutdown state tests and the complete
  freshly built library suite passed again: 3,248 passed, zero failed, 54 ignored.
- Retrieval evaluation tooling: 64 tests passed.
- Sidecar verifier: 93 tests passed, one platform-specific test skipped.
- All five packaged sidecar files passed checksum and dependency inspection.
  The macOS ARM sidecar also executed successfully. Windows and Linux binaries
  were inspected but could not be executed on this host.

## Native macOS acceptance

Built and launched `Lattice Release Check.app` with identifier
`tech.lattice.releasecheck`, using a fresh, separate application data directory.
This is a debug Rust backend with the production Vite renderer and bundled
llama-server, ad-hoc signed; it is not a notarized distribution build.

- Bundle signature verification (`codesign --verify --deep --strict`): passed.
- Journal creation/editing and navigation: passed with actual native IPC/SQLite.
- Import with no configured embedding model: showed the actionable model error
  and retained the file with a retry action.
- Before repair: Cmd-Q dropped an edit visibly marked "Saving…"; reproduced
  twice, including an accessibility snapshot confirming the edit before quit.
- After repair: the pending body edit survived Cmd-Q and reopening the UI.
- A title still focused at quit was committed and persisted.
- Held a write lock on the disposable database: quitting waited for saving and
  offered "Keep working". Cancellation kept the draft open. After releasing the
  lock, retrying the save succeeded; no late completion closed the app.
- Window-close with a pending edit persisted the complete marker.
- Final SQLite `quick_check`: `ok`; foreign-key violations: zero.

The final markers and database checks are recorded locally in
`/private/tmp/lattice-release-review/native-acceptance.json`. The Dock UI could
not be automated on this host; its shared AppKit termination callback is covered
by the Cmd-Q run, but Dock Quit was not independently exercised. System logout
and shutdown were not triggered.

## Limits

Windows/Linux execution still requires those CI runners or machines. Existing
synthetic retrieval results remain regression evidence; they do not establish
quality on a representative, human-reviewed real library. No model quality
claim or retrieval-default change is made by this pass.

Detailed local command output is in `/private/tmp/lattice-release-review`.
Changes remain uncommitted; unrelated work in the checkout was preserved.
