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

## Coverage

1. Native startup, first-run dialog and real persistence of its dismissal.
2. File reads through Rust with spaces and Unicode in the path, rejection of a
   missing file, and continued backend responsiveness after the error.
3. Journal navigation and editing at the app's 800×600 minimum window size.
4. Native window close while the page title is still focused and edits may be
   pending; the actual shutdown save gate must complete successfully.
5. A new process restores the settings, title and body from disk, and the UI
   displays the saved page.

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
global API and test-driver capability are enabled only by the test config.
These instrumented candidates are testing artifacts, not shipping installers.

The driver requires Tauri 2.10, so the Rust core and JavaScript API/CLI are on
the same 2.10 minor line. The suite uses WebdriverIO's standard WebDriver API
against `tauri-plugin-wdio-webdriver`, without the command-mocking plugin.
