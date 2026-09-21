# Desktop compatibility validation — 2026-09-19

Added a packaged native application suite and a four-platform CI matrix. The
suite drives the real Tauri webview and Rust handlers, using an isolated test
application identity and library. See the [test guide](../../e2e/desktop/README.md)
for commands and coverage.

## Observed locally

Host: macOS 26.6.2 (25G83), Apple Silicon arm64. Candidate: debug Rust profile
with the production Vite renderer, copied from its application bundle to a path
containing spaces and Unicode. Its ad-hoc code signature verified successfully.

- Native suite: five scenarios plus the parent test, **6 passed, 0 failed**.
  Covers onboarding persistence, native file reads and error recovery, journal
  editing at 800×600, native close with a focused title, and saved settings and
  journal content after a new process starts.
- Packaged CPU sidecar: checksum-pinned tiny GGUF loaded successfully and
  generated **8 tokens**. This checks execution, not model quality or speed.
- TypeScript type checking passed. The existing first-run and native-shutdown
  regression tests passed: **9 tests in 2 files**.
- Normal Rust library and application passed Clippy with `-D warnings`.
- Formatting, workflow lint, JavaScript syntax and diff whitespace checks passed.
- The normal Cargo dependency tree excludes the optional WebDriver plugin.
  Enabling `desktop-e2e` with the production identity was rejected by the build
  guard as intended.

Results, screenshots and native logs are generated under
`e2e-results/desktop/reports/`. These are local artifacts, excluded from Git.
The final native run is `1789850327220`. A local attempt to run standalone CPU
generation concurrently with application startup was terminated by the app's
cleanup of sidecars from its own bundle. The sequential rerun passed; CI already
runs those checks sequentially.

## Configured, awaiting execution

CI builds instrumented release-mode packages and runs the native suite and
packaged CPU generation on macOS 15 Apple Silicon, macOS 15 Intel, Ubuntu 24.04
x64 and Windows Server 2025 x64. Those remote jobs were not run during this local
validation. Their configuration is not evidence of passing results.

Windows 11, the minimum supported macOS 13.3, GPU drivers, installer trust and
notarization, and representative low-memory hardware still require acceptance
testing. Ubuntu CI extracts and executes the Debian package; it does not test
package-manager dependency installation. Intel sidecar publication remains a
separate release gate described in [platform support](platform-support.md).

The test driver requires the Tauri 2.10 line. The Rust core and JavaScript
API/CLI were updated together; instrumentation is opt-in and is not included
in normal builds.
