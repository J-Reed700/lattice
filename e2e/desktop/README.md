# Native desktop compatibility tests

This suite starts the real packaged Tauri application, uses its actual webview
and Rust IPC handlers, writes to an isolated library, closes the native window
and starts the application again. It does not mock Tauri commands.

## Run locally

Install the normal Tauri prerequisites and CMake. Then:

```bash
npm ci
bash src-tauri/scripts/fetch-llama-binaries.sh
npm run test:desktop:build
npm run test:desktop
```

On an Intel Mac, build the unpublished candidate sidecar with
`bash src-tauri/scripts/build-intel-macos-sidecar.sh` before the application build.
The local debug helper allows that unpinned development sidecar. Release builds
still require verified checksum pins. CI pins its source-built Intel candidate
only inside that job; it does not change the repository release pin.

Linux without a desktop session: `xvfb-run --auto-servernum npm run test:desktop`.
To build an optimized candidate, use `npm run test:desktop:build -- --release`.
The build helper assigns a unique identifier. Before each full run the runner
moves that candidate's existing test library aside, so onboarding starts fresh
and failed-run data remains available for debugging. The close/reopen portion
uses the same library across both processes.
Build artifacts default to `src-tauri/target/desktop-e2e`, keeping the normal
development executable unchanged. Set `CARGO_TARGET_DIR` only to override this
with another dedicated test build directory.

## Coverage

1. Native startup, first-run dialog and real persistence of its dismissal.
2. Learning Studio command registration and missing-program rejection for
   assessment, curriculum, practical-workspace and recall reads against the
   real migrated SQLite database. It does not create a generated program or
   verify successful Learning Studio writes in the packaged app.
3. File reads through Rust with spaces and Unicode in the path, rejection of a
   missing file, and continued backend responsiveness after the error.
4. Journal navigation and editing at the app's 800×600 minimum window size.
5. Native window close while the page title is still focused and edits may be
   pending; the actual shutdown save gate must complete successfully.
6. A new process restores the settings, title and body from disk, and the UI
   displays the saved page.
7. Custom collections survive a native restart and support rename and delete
   through the real SQLite repository and IPC handlers.
8. Runtime catalog registration and the installed Python resource directory.

Before the UI suite, the build helper runs fixed Python and JavaScript exercises
through the installed executable. It verifies correct and incorrect results,
denied host access, and the bundled Python resource hashes. On macOS the probe
runs the signed application with the hardened runtime enabled, exercising the
executable-memory entitlement required by Wasmtime. Its result is retained in
`e2e-results/desktop/reports/embedded-runtime.json`. The probe's command-line entry
point exists only in `desktop-e2e` builds and accepts no learner code.

CI additionally loads a checksum-pinned tiny GGUF model with the packaged CPU
sidecar and requires generated text. That tests model execution, not useful
answer quality, embeddings, semantic search or GPU performance.
Run that standalone engine check after the native suite finishes, as CI does:
application startup cleans up sidecar processes from the same installed bundle.

## CI and evidence

The `desktop-compatibility` matrix in `.github/workflows/ci.yml` covers:

| Runner | Architecture | Package handling |
|---|---|---|
| macOS 15 | Apple Silicon | Copy and verify the application bundle |
| macOS 15 Intel | x64 | Copy and verify the application bundle |
| Ubuntu 24.04 | x64 | Extract the Debian package and run its installed layout under Xvfb |
| Windows Server 2025 | x64 | Run the NSIS installer silently and start its installed executable |

Jobs fail on build, installation, assertion or CPU-generation errors. Each job
uploads a JSON result, native process logs and screenshots under an artifact
named `desktop-compatibility-<platform>`. Logs remain available when a test fails.
Local results are under `e2e-results/desktop/reports/<run-timestamp>/`.

Configured jobs are not passing results. Observe a green run for the candidate
commit before claiming cross-platform compatibility. Hosted Windows runners use
Windows Server; Windows 11 still needs a clean-machine acceptance run. Likewise,
macOS 15 CI does not establish the macOS 13.3 minimum, GPU/driver coverage,
low-memory performance, sleep/wake behavior or installer signing/notarization.
Debian extraction does not test package-manager dependency installation.

## Isolation

The embedded WebDriver server is an optional Cargo dependency behind
`desktop-e2e`; normal builds omit it. The build script rejects that feature with
the production app identity. Test bundles use `tech.lattice.compatibility.*`,
separate application data and a dynamically allocated loopback port. Tauri's
global API, test-driver capability, and explicit window-focus permission are
enabled only by the test config. The harness shows and focuses its isolated
window and waits for webview visibility before interaction. On macOS the desktop
must be unlocked: locked WebKit views pause their animation clocks. The harness
rejects a locked session explicitly rather than reporting misleading UI timeouts.
These instrumented candidates are testing artifacts, not shipping installers.

The driver requires Tauri 2.10 or newer; the Rust core and JavaScript API/CLI use
the same 2.11 minor line. The suite uses WebdriverIO's standard WebDriver API
against `tauri-plugin-wdio-webdriver`, without the command-mocking plugin.

## Resource regression workload

The native scenario also runs 25 context-panel open/close cycles and 250 native
notes reads. Its JSON report records DOM nodes and the candidate process tree's
RSS (Windows: working set). After five warmup cycles, final five-sample mean
growth must be below 64 MiB, closed-panel DOM variation below 100 nodes, and
child-process count must plateau. These portable budgets detect substantial
retention in this workload; they are not proof of zero leaks or latency benchmarks.
Normal close also checks that sampled child processes exit, and rejects
shutdown timeouts or orphan-sidecar recovery on the next launch.
macOS WebKit XPC processes and Metal allocations need separate Instruments
measurements. The workload uses only the isolated compatibility library.

On macOS, `LATTICE_NATIVE_LEAK_SCAN=1 npm run test:desktop` additionally captures
Apple `leaks` output for the candidate before shutdown in `native-leaks.txt`.
It also captures `native-leaks-baseline.txt` before the repeated-use workload
so startup retention can be distinguished from allocations added by the cycles.
The candidate launches with `MallocStackLogging=1` so findings include allocation
stacks. This instrumentation adds overhead; use a separate ordinary run for RSS.
This diagnostic is reported separately from the portable RSS/DOM budgets;
inspect its exit code and output for allocator findings or permission errors.
