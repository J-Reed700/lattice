# Intel macOS candidate validation — 2026-09-19

Scope: add Intel Macs to the desktop targets, retaining macOS 13.3 as the
minimum. No Windows ARM, Linux ARM or mobile target was added.

## Implemented

- Intel CPU sidecar target and native Intel release-build runner.
- Target-aware Mach-O verification, including the correct universal slice.
- CPU-only Candle selection on Intel; shared selection for transcription.
- CPU capability reporting for Intel model recommendations.
- Intel Rust CI and a release-mode application packaging job, including
  signature/architecture checks and generation with a checksum-pinned fixture.
- Current published sidecars remain usable while the new target is qualified;
  release builds still require a pinned Intel checksum.

## Observed locally

Host: Apple Silicon macOS. Logs and the built Intel sidecar are under
`/private/tmp/lattice-intel-check/`.

| Check | Result |
|---|---|
| Intel sidecar source build | Passed; llama.cpp b8981, commit `d77599234ea6e498775aeadbce665eece5bd98cd` |
| Intel binary format | Thin x86_64 Mach-O |
| Intel dependency/minimum-OS/signature checks | Passed; only system libraries, minimum macOS 13.3, valid ad-hoc signature |
| Intel sidecar startup under Rosetta | Passed; reports version 8981 |
| Intel CPU inference under Rosetta | Passed; loaded stories260K.gguf and generated 8 tokens |
| Rust library/application Intel cross-compilation check | Passed using the explicit debug-only unpinned-sidecar override |
| Intel Candle features | No Metal or CUDA feature enabled |
| Apple Silicon strict Clippy (library and application) | Passed with `-D warnings` |
| Shared CPU-selection unit test (Apple Silicon host) | Passed; Intel-specific assertions are scheduled in native Intel CI |
| Binary verifier suite | 100 passed, 1 optional real-binary test skipped |
| Existing published sidecars | All five pass checksums/dependency checks; Apple Silicon sidecar executes locally |
| GitHub Actions workflow validation | `actionlint` passed for both modified workflows |
| Shell script syntax | Passed |
| Rust formatting and whitespace checks | Passed |

The fixture SHA-256 is
`270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d`,
matching the [upstream model file](https://huggingface.co/ggml-org/models-moved/blob/main/tinyllamas/stories260K.gguf).
This tiny model checks execution, not real-model speed or response quality.

The Intel Rust check completed with a `truncate_messages` dead-code warning in
`context_manager.rs`. The later strict Apple Silicon Clippy run passed. This
task did not modify that file.

## Still required before release

The native Intel CI jobs are configured but have not been run remotely in this
session. An Intel application installer was not built or installed locally.
Rosetta execution does not establish performance or compatibility on physical
Intel hardware or on macOS 13.3.

Complete the physical-device acceptance and publication gates in
[platform support](platform-support.md). In particular, the published
`llama/b8981-r2` release has no Intel asset; a new immutable six-file release,
verified checksums, signed/notarized installers and clean-machine acceptance
are required before advertising released Intel support.
