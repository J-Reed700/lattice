# Sidecar Binaries

This directory holds the `llama-server` sidecar binaries that ship inside the Lattice installer. The binaries themselves are **not committed** — they are fetched at build time by `scripts/fetch-llama-binaries.sh`.

## How it works

1. **Build:** `.github/workflows/llama-build.yml` (in the repo root) compiles `llama-server` for each target (Mac+Metal, Windows+Vulkan, Windows+CPU, Linux+Vulkan) and attaches the resulting binaries to a GitHub Release tagged `llama/<llama-cpp-tag>`.

2. **Fetch:** `scripts/fetch-llama-binaries.sh` downloads those binaries into this directory, verifying SHA-256 checksums against `SHA256SUMS.txt`.

3. **Bundle:** Tauri's `externalBin` config (in `tauri.conf.json`) picks up the binaries here at app build time and bundles them into the installer.

4. **Spawn:** At runtime, Lattice spawns the right binary based on detected hardware (see `features/llm/engine/sidecar_manager.rs`).

## Local dev

To get the binaries locally:

```bash
cd src-tauri
bash scripts/fetch-llama-binaries.sh
```

This reads `scripts/llama-server-version.txt` for the pinned tag.

## Bumping the llama.cpp version

1. Edit `scripts/llama-server-version.txt` (single line, e.g. `b8981`).
2. Edit `.github/workflows/llama-build.yml`'s `LLAMA_CPP_TAG` env to match.
3. Manually trigger `.github/workflows/llama-build.yml` from the Actions tab, OR push a tag like `git tag llama/b8981 && git push origin llama/b8981`.
4. Once the Release is published, run `bash scripts/fetch-llama-binaries.sh` locally and verify.
5. Commit the version bump.

## Naming convention

Tauri's `externalBin` requires binaries to follow `<base-name>-<target-triple>` pattern. Our base name is `llama-server`. The targets are:

| File | Target triple | Backend |
|---|---|---|
| `llama-server-aarch64-apple-darwin` | macOS Apple Silicon | Metal |
| `llama-server-x86_64-pc-windows-msvc.exe` | Windows x64 | Vulkan |
| `llama-server-cpu-x86_64-pc-windows-msvc.exe` | Windows x64 | CPU (AVX2) — fallback |
| `llama-server-x86_64-unknown-linux-gnu` | Linux x64 | Vulkan |

The CPU Windows variant uses a non-standard prefix (`llama-server-cpu-`) because Tauri's `externalBin` only allows one binary per target triple. The runtime fallback logic (Sprint 3) explicitly picks between these two when Vulkan init fails.
