#!/usr/bin/env bash
set -euo pipefail

# CPython's upstream WASI helper and WASI SDK 24 currently support this pinned
# release-build host. The resulting WASI Preview 1 module is host-independent.
if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "Build this pinned CPython WASI bundle on macOS arm64 (the WASI module itself is portable)." >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
RESOURCES="$ROOT/src-tauri/resources/learning-python"
BUILD="$(mktemp -d "$ROOT/.learning-python-build.XXXXXX")"
trap 'rm -rf "$BUILD"' EXIT

CPYTHON_VERSION=3.14.8
CPYTHON_SHA256=a65b20a728f169f4e66ae143f40b1bd3d33c38d770251663f627c9767b79b210
SDK_VERSION=24.0
SDK_SHA256=aeae999396d5f5caa5ce419f52e83c35869d5fd21d40af80acba2c80f51b0b3a
WASMTIME_VERSION=48.0.1
WASMTIME_SHA256=88cc08b395fbfb960b99f355a81224af975679b8a5f4b74a51d59e5e34b20dcd
PYTHON="${PYTHON:-python3}"

"$PYTHON" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11 or newer is required")'

fetch_checked() {
  local url="$1" expected="$2" file="$3" actual
  curl --fail --location --silent --show-error "$url" --output "$file"
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $(basename "$file"): $actual" >&2
    exit 1
  fi
}

fetch_checked "https://www.python.org/ftp/python/$CPYTHON_VERSION/Python-$CPYTHON_VERSION.tgz" \
  "$CPYTHON_SHA256" "$BUILD/Python-$CPYTHON_VERSION.tgz"
fetch_checked "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-24/wasi-sdk-$SDK_VERSION-arm64-macos.tar.gz" \
  "$SDK_SHA256" "$BUILD/wasi-sdk-$SDK_VERSION-arm64-macos.tar.gz"
fetch_checked "https://github.com/bytecodealliance/wasmtime/releases/download/v$WASMTIME_VERSION/wasmtime-v$WASMTIME_VERSION-aarch64-macos.tar.xz" \
  "$WASMTIME_SHA256" "$BUILD/wasmtime-v$WASMTIME_VERSION-aarch64-macos.tar.xz"

tar -xzf "$BUILD/Python-$CPYTHON_VERSION.tgz" -C "$BUILD"
tar -xzf "$BUILD/wasi-sdk-$SDK_VERSION-arm64-macos.tar.gz" -C "$BUILD"
tar -xJf "$BUILD/wasmtime-v$WASMTIME_VERSION-aarch64-macos.tar.xz" -C "$BUILD"
export WASI_SDK_PATH="$BUILD/wasi-sdk-$SDK_VERSION-arm64-macos"
export PATH="$BUILD/wasmtime-v$WASMTIME_VERSION-aarch64-macos:$PATH"

# Build in a child of the real checkout path: CPython's helper derives relative
# script paths from cwd, and /tmp aliases /private/tmp on macOS.
cd "$BUILD"
"$PYTHON" "$BUILD/Python-$CPYTHON_VERSION/Tools/wasm/wasi" build \
  --quiet --wasi-sdk "$WASI_SDK_PATH" --logdir "$BUILD/logs" -- --config-cache

install -d "$RESOURCES/lib"
install -m 0644 "$BUILD/Python-$CPYTHON_VERSION/cross-build/wasm32-wasip1/python.wasm" "$RESOURCES/python.wasm"
install -m 0644 "$BUILD/Python-$CPYTHON_VERSION/LICENSE" "$RESOURCES/LICENSE"
install -m 0644 "$ROOT/scripts/learning-python.lock.json" "$RESOURCES/PROVENANCE.json"
install -m 0644 "$ROOT/scripts/learning-python-third-party-notices.txt" "$RESOURCES/THIRD_PARTY_NOTICES.txt"
"$PYTHON" "$ROOT/scripts/package-learning-python.py" \
  "$BUILD/Python-$CPYTHON_VERSION/Lib" "$RESOURCES"

echo "Packaged CPython $CPYTHON_VERSION WASI runtime at $RESOURCES"
shasum -a 256 "$RESOURCES/python.wasm" "$RESOURCES/lib/python314.zip"
