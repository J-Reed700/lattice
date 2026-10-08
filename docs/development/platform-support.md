# Desktop platform scope

The release target is Windows 11 x64, macOS 13.3 or later on Apple Silicon and
Intel, and Ubuntu 24.04 LTS x64. Windows ARM, Linux ARM, other Linux distributions,
phones and tablets are outside the initial support promise.

## Intel macOS qualification

Intel Macs use CPU inference for chat, embeddings, reranking and transcription.
The Intel build does not advertise a GPU to model recommendations, even if the
Mac has a Metal-capable Intel or AMD display adapter. Apple Silicon continues
to use Metal. macOS 13.3 remains the minimum for both architectures.

Two separate Mac packages are built: `aarch64-apple-darwin` and
`x86_64-apple-darwin`. A universal application bundle is not currently offered.

The Intel application target remains a **qualification candidate**, pending
native CI evidence, physical-hardware acceptance, and signed/notarized installer
validation. Its runtime dependency is published: the pinned `llama/v0.6.0-r1`
sidecar release contains all six platform binaries, including
`llama-server-x86_64-apple-darwin`. Fetch, install, and production-build checks
require the checksum in `src-tauri/scripts/llama-server.lock`.

### Development

On an Intel Mac with Xcode tools and Rust installed, fetch the published sidecar
and build the target normally:

```bash
rustup target add x86_64-apple-darwin
bash src-tauri/scripts/fetch-llama-binaries.sh
npm run tauri -- build --debug --target x86_64-apple-darwin --bundles app
```

To exercise the source-build path instead, install CMake and Python 3.9+, run
`bash src-tauri/scripts/build-intel-macos-sidecar.sh`, and set
`LATTICE_ALLOW_UNPINNED_SIDECAR=1` for the following debug build. The script
uses the pinned llama.cpp tag and release flags, signs the Intel binary, deletes
the build tree, and checks its architecture, dependencies, and minimum OS. On
an Intel host it also requires successful execution. On Apple Silicon it
cross-compiles and performs static checks; that is not an Intel hardware test.
The source-built binary does not change the repository release lock.

### Automated checks

- The Rust CI matrix includes `macos-15-intel` for compilation, unit tests,
  credential/audit integration tests and linting.
- The sidecar release matrix builds a self-contained, signed Intel CPU binary
  and executes it after deleting the source/build tree.
- `desktop-compatibility` builds an instrumented release-mode candidate on each
  supported architecture, exercises the native UI and persistence across
  restarts, and executes the packaged AI engine with a checksum-pinned tiny
  model to exercise CPU generation. See the [native test guide](../../e2e/desktop/README.md).
  The Intel job deliberately builds the sidecar from source and writes that
  candidate's checksum into its temporary checkout; the repository's published
  release pin remains unchanged.
  Jobs upload results, screenshots and logs. The instrumented Mac bundles are
  ad-hoc signed test candidates, not notarized public installers.
- The binary verifier tests thin Intel executables, wrong architectures,
  unsigned files and the Intel slice of universal binaries independently.

### Release gates

1. Observe passing native Intel CI and package results for the release commit.
2. On a physical Intel Mac running the minimum supported macOS, install the
   candidate and test onboarding, model download, import, keyword/semantic
   search, chat with citations, journal saving, quit/reopen and library upgrade.
3. Measure memory use and model latency on a lower-memory Intel Mac. Recommend
   models using measured CPU performance; do not promise Apple Silicon speed.
4. On the clean Intel test machine, fetch the six-file sidecar release pinned in
   `src-tauri/scripts/llama-server.lock` and confirm the packaged application
   runs that exact verified binary.
5. Build, sign and notarize the separate Mac installers and test installation
   on clean machines before advertising Intel as released support.

CI configuration alone is not evidence that those jobs passed. Record the
commit, OS, CPU, RAM, models and results from each actual acceptance run.
