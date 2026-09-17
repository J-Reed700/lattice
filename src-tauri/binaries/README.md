# Sidecar Binaries

This directory holds the `llama-server` sidecar binaries that ship inside the Lattice installer and run local GGUF models. They are **not committed**. `scripts/fetch-llama-binaries.sh` installs them from a GitHub Release pinned in `scripts/llama-server.lock`.

## Guarantees

Each guarantee has a check that fails loudly when it breaks:

| Guarantee | Enforced by |
|---|---|
| Every binary is one self-contained file: only OS-provided libraries (plus the system Vulkan loader for the Vulkan builds), no rpaths, macOS minimum ≤ `macos_min`, glibc ≤ `glibc_max`, static C++ runtime, no OpenMP | `scripts/verify_llama_binaries.py`, run by the build workflow, the fetch script and CI |
| The binary for each runner starts after its build tree is deleted | `llama-build.yml` (`--run`) |
| A published release never changes | `llama-build.yml` refuses to publish over an existing release; the lock pins every file's SHA-256 |
| The installed files are exactly the pinned ones | `fetch-llama-binaries.sh` replaces anything that doesn't match the lock; CI's `sidecar-binaries` job fetches, verifies and runs them on macOS, Windows and Linux |
| A release build can't bundle anything else | `build.rs` fails release builds whose sidecars don't match the lock |
| A broken binary is reported, not silently ignored | the app runs `--version` at startup and names the reason in model settings; Windows and Linux fall back to the CPU binary |

## Files

| File | Platform | Backend |
|---|---|---|
| `llama-server-aarch64-apple-darwin` | macOS Apple Silicon | Metal |
| `llama-server-x86_64-pc-windows-msvc.exe` | Windows x64 | Vulkan |
| `llama-server-cpu-x86_64-pc-windows-msvc.exe` | Windows x64 | CPU (AVX2), fallback |
| `llama-server-x86_64-unknown-linux-gnu` | Linux x64 | Vulkan |
| `llama-server-cpu-x86_64-unknown-linux-gnu` | Linux x64 | CPU (AVX2), fallback |

Tauri's `externalBin` names files `<name>-<target-triple>`, so the CPU fallbacks are a second sidecar, `binaries/llama-server-cpu`, bundled on Windows and Linux through `tauri.windows.conf.json` and `tauri.linux.conf.json`.

## Local development

```bash
bash src-tauri/scripts/fetch-llama-binaries.sh          # install or repair
bash src-tauri/scripts/fetch-llama-binaries.sh --check  # verify only
```

Files that already match the lock are not downloaded again.

## Bumping llama.cpp or rebuilding

1. In `scripts/llama-server.lock`, set `llama_cpp_tag` (for a rebuild, keep it), set `release` to `llama/<llama_cpp_tag>-r<N>` with a new `N`, and delete the `sha256` lines.
2. Optional dry run: run **Build llama-server sidecar binaries** from the Actions tab on your branch. It builds and verifies all five binaries without publishing. Pull requests that touch the pipeline do the same automatically.
3. Commit the lock, then push the matching tag: `git tag llama/<llama_cpp_tag>-r<N> && git push origin llama/<llama_cpp_tag>-r<N>`. The workflow checks that the tag equals the lock's `release` and publishes the release.
4. Pin it: `bash src-tauri/scripts/fetch-llama-binaries.sh --update-lock`. This downloads the release, checks it against its `SHA256SUMS.txt`, verifies every binary and writes the `sha256` lines.
5. Commit the updated lock. Until then CI's `sidecar-binaries` job and release builds fail on purpose.

When raising `macos_min`, raise `bundle.macOS.minimumSystemVersion` in `tauri.conf.json` to match; `build.rs` rejects release builds where they differ.
