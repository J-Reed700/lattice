#!/usr/bin/env bash
# Download the llama-server sidecar binaries from a Lattice GitHub
# Release into src/app/src/binaries/.
#
# This script is for local dev and CI. The binaries are produced by
# .github/workflows/llama-build.yml (separate, manual workflow) and
# attached to a release tag like `llama/b8981`.
#
# Usage:
#   bash scripts/fetch-llama-binaries.sh [<release-tag>]
#
# If no tag is given, the script reads the pinned tag from
# scripts/llama-server-version.txt.
#
# Auth strategy:
#   - If `gh` (GitHub CLI) is installed and authenticated, use it via
#     `gh release download`. Works for both public and private repos.
#   - Otherwise fall back to `curl`, which works for public repos only.
#
# The `gh` path is preferred because Lattice's release repo is currently
# private — anonymous curl gets a 404 even for valid asset URLs.
#
# Requires: gh OR curl, plus shasum (or sha256sum on Linux).

set -euo pipefail

# --------------------------------------------------------------------
# Resolve repo root (two levels up from this script's directory).
# --------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARIES_DIR="$TAURI_ROOT/binaries"
VERSION_FILE="$SCRIPT_DIR/llama-server-version.txt"

# --------------------------------------------------------------------
# Resolve which tag to fetch.
# --------------------------------------------------------------------
if [[ $# -ge 1 ]]; then
  LLAMA_TAG="$1"
elif [[ -f "$VERSION_FILE" ]]; then
  LLAMA_TAG="$(tr -d '[:space:]' < "$VERSION_FILE")"
else
  echo "error: no tag argument and $VERSION_FILE missing" >&2
  exit 1
fi

# Owner/repo containing the llama-build.yml workflow's releases. Default
# matches the active Lattice org; override via env if you fork.
GH_REPO="${LATTICE_LLAMA_REPO:-posh-industries/lattice}"
RELEASE_TAG="llama/${LLAMA_TAG}"
BASE_URL="https://github.com/$GH_REPO/releases/download/$RELEASE_TAG"

echo "Fetching llama-server $LLAMA_TAG from $GH_REPO"
mkdir -p "$BINARIES_DIR"

# --------------------------------------------------------------------
# Files to fetch. Each is uploaded by a job in llama-build.yml.
# --------------------------------------------------------------------
FILES=(
  "llama-server-aarch64-apple-darwin"
  "llama-server-x86_64-pc-windows-msvc.exe"
  "llama-server-cpu-x86_64-pc-windows-msvc.exe"
  "llama-server-x86_64-unknown-linux-gnu"
  "SHA256SUMS.txt"
)

# --------------------------------------------------------------------
# Pick a download backend. `gh release download` handles both public
# and private repos via the user's existing auth; bare curl works only
# for public repos. We prefer gh whenever it's installed AND authed.
# --------------------------------------------------------------------
USE_GH=0
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  USE_GH=1
  echo "  using: gh CLI (handles private repos)"
else
  echo "  using: curl (public-repo only — install/auth gh CLI for private)"
fi

if [[ $USE_GH -eq 1 ]]; then
  # `gh release download` accepts --pattern to filter assets and
  # downloads to --dir. Skip files that already exist by checking
  # before each call.
  cd "$BINARIES_DIR"
  for file in "${FILES[@]}"; do
    if [[ -f "$file" ]]; then
      echo "  skip (exists): $file"
      continue
    fi
    echo "  download: $file"
    gh release download "$RELEASE_TAG" \
      --repo "$GH_REPO" \
      --pattern "$file" \
      --dir . \
      --clobber
  done
  cd - >/dev/null
else
  for file in "${FILES[@]}"; do
    dest="$BINARIES_DIR/$file"
    if [[ -f "$dest" ]]; then
      echo "  skip (exists): $file"
      continue
    fi
    echo "  download: $file"
    curl --fail --location --silent --show-error \
      --output "$dest" \
      "$BASE_URL/$file"
  done
fi

# --------------------------------------------------------------------
# Verify checksums. The SHA256SUMS.txt was generated on Ubuntu (shasum)
# so we accept either sha256sum or shasum.
# --------------------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then
  HASH_TOOL="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  HASH_TOOL="shasum -a 256"
else
  echo "warning: no sha256sum/shasum found; skipping checksum verification" >&2
  HASH_TOOL=""
fi

if [[ -n "$HASH_TOOL" ]]; then
  cd "$BINARIES_DIR"
  $HASH_TOOL --check SHA256SUMS.txt
  cd - >/dev/null
fi

# --------------------------------------------------------------------
# chmod +x for the unix binaries (they've been downloaded as 644).
# --------------------------------------------------------------------
chmod +x "$BINARIES_DIR/llama-server-aarch64-apple-darwin" 2>/dev/null || true
chmod +x "$BINARIES_DIR/llama-server-x86_64-unknown-linux-gnu" 2>/dev/null || true

echo "Done. Binaries in $BINARIES_DIR"
