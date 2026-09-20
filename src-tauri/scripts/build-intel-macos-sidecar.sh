#!/usr/bin/env bash
# Build an Intel development sidecar from the same pin and flags as release CI.
# Does not change the release lock or authorize a production release.
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || { echo 'This build requires macOS and Xcode tools.' >&2; exit 1; }
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK="$SCRIPT_DIR/llama-server.lock"
DEST="${1:-$SCRIPT_DIR/../binaries}"
mkdir -p "$DEST"
DEST="$(cd "$DEST" && pwd)"
BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lattice-intel.XXXXXX")"
trap 'rm -rf "$BUILD_DIR"' EXIT
TAG="$(awk '$1 == "llama_cpp_tag" { print $2 }' "$LOCK")"
MACOS_MIN="$(awk '$1 == "macos_min" { print $2 }' "$LOCK")"
FILE=llama-server-x86_64-apple-darwin
BACKEND="$(python3 - "$SCRIPT_DIR" "$FILE" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
from verify_llama_binaries import TARGETS
print(TARGETS[sys.argv[2]].cmake_backend)
PY
)"
git clone --filter=blob:none --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$BUILD_DIR/source"
args=(
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_SKIP_RPATH=ON -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF -DGGML_OPENMP=OFF -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_SERVER=ON -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_MIN"
)
# Backend flags come from the verifier's target table, just as in release CI.
# shellcheck disable=SC2206
args+=($BACKEND)
cmake -S "$BUILD_DIR/source" -B "$BUILD_DIR/build" "${args[@]}"
cmake --build "$BUILD_DIR/build" --target llama-server -j 2
cp "$BUILD_DIR/build/bin/llama-server" "$BUILD_DIR/$FILE"
codesign --force --sign - "$BUILD_DIR/$FILE"
chmod +x "$BUILD_DIR/$FILE"
echo "Built upstream commit $(git -C "$BUILD_DIR/source" rev-parse HEAD)"
rm -rf "$BUILD_DIR/source" "$BUILD_DIR/build"
grep -v '^sha256 ' "$LOCK" > "$BUILD_DIR/unhashed.lock"
RUN_FLAGS=()
if [[ "$(uname -m)" == x86_64 ]]; then
  RUN_FLAGS=(--run --strict --require-run)
fi
python3 "$SCRIPT_DIR/verify_llama_binaries.py" --lock "$BUILD_DIR/unhashed.lock" \
  ${RUN_FLAGS[@]+"${RUN_FLAGS[@]}"} "$BUILD_DIR/$FILE"
mv "$BUILD_DIR/$FILE" "$DEST/$FILE"
echo "Intel development sidecar: $DEST/$FILE (release lock unchanged)"
