#!/usr/bin/env bash
# Install the pinned llama-server sidecar binaries into src-tauri/binaries/.
#
# Everything comes from scripts/llama-server.lock: the release to download,
# the repository that owns it, and the SHA-256 of every file. A file is only
# installed if its hash matches the lock AND verify_llama_binaries.py proves it
# is self-contained (and, for this machine's platform, that it actually runs).
# Installed files that already match the lock are not downloaded again; files
# that don't match are replaced, never kept.
#
# Usage:
#   bash scripts/fetch-llama-binaries.sh               install / repair
#   bash scripts/fetch-llama-binaries.sh --check       verify what is installed, download nothing
#   bash scripts/fetch-llama-binaries.sh --update-lock after publishing a release: pin its hashes
#   add --no-run to skip executing the host binary, or --strict to fail when a
#   Vulkan build can't be run because this machine has no Vulkan loader (CI)
#   add --repin to let --update-lock replace a hash the lock already pins
#
# --update-lock prints every line it would change, always runs the host's
# binary before pinning anything, and refuses to overwrite an existing pin
# unless --repin says so: a published release is immutable, so a changed hash
# means the release changed under us.
#
# Downloads use `gh` when it is installed and authenticated, otherwise curl
# (the release repository is public).
#
# Requires: bash, python >= 3.9, gh or curl.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARIES_DIR="$TAURI_ROOT/binaries"
LOCK="$SCRIPT_DIR/llama-server.lock"
VERIFY="$SCRIPT_DIR/verify_llama_binaries.py"

MODE=install
RUN=1
STRICT=
REPIN=0
for arg in "$@"; do
  case "$arg" in
    --check) MODE=check ;;
    --update-lock) MODE=update-lock ;;
    --no-run) RUN=0 ;;
    --strict) STRICT=--strict ;;
    --repin) REPIN=1 ;;
    -h|--help) sed -n '2,27p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown argument '$arg' (see --help)" >&2; exit 2 ;;
  esac
done

usage_error() { echo "error: $*" >&2; exit 2; }

# Contradictory flags are an error, never a silent downgrade of the stricter one.
if [[ $RUN -eq 0 && -n "$STRICT" ]]; then
  usage_error "--strict only decides how a run is judged; it cannot be combined with --no-run"
fi
if [[ "$MODE" == update-lock && $RUN -eq 0 ]]; then
  usage_error "--update-lock must run the binaries it pins; drop --no-run"
fi
if [[ $REPIN -eq 1 && "$MODE" != update-lock ]]; then
  usage_error "--repin only applies to --update-lock"
fi

RUN_FLAG=
if [[ $RUN -eq 1 ]]; then
  RUN_FLAG="--run $STRICT"
fi
if [[ "$MODE" == update-lock ]]; then
  # A hash is a promise that this build works; only an executed binary earns it.
  RUN_FLAG="--run --strict --require-run"
fi

die() { echo "error: $*" >&2; exit 1; }

lock_value() {
  awk -v key="$1" '$1 == key { print $2; n++ } END { exit n == 1 ? 0 : 1 }' "$LOCK" \
    || die "$LOCK must contain exactly one '$1' line"
}

# Prints the pinned hash of a file, or nothing when the lock has none.
lock_hash() {
  awk -v file="$1" '$1 == "sha256" && $3 == file { print $2 }' "$LOCK"
}

file_hash() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

PYTHON=
for candidate in python3 python; do
  if command -v "$candidate" >/dev/null 2>&1 \
    && "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 9))' >/dev/null 2>&1; then
    PYTHON="$candidate"
    break
  fi
done
[[ -n "$PYTHON" ]] || die "python >= 3.9 is required to verify the binaries"

# Install/check the published set. --update-lock requires the entire build
# matrix, including new architectures, before a new release can be pinned.
SELECT_FLAGS=()
if [[ "$MODE" != update-lock ]]; then
  SELECT_FLAGS+=(--pinned-only)
fi
FILES=()
while IFS= read -r file; do
  # Strip a trailing CR: the lock lookup matches filenames exactly, so one
  # stray carriage return makes every binary look unpinned. The verifier now
  # writes LF on every host, and this keeps that from being load-bearing.
  file="${file%$'\r'}"
  [[ -n "$file" ]] && FILES+=("$file")
done < <("$PYTHON" "$VERIFY" --lock "$LOCK" ${SELECT_FLAGS[@]+"${SELECT_FLAGS[@]}"} --print-targets files)
[[ ${#FILES[@]} -gt 0 ]] || die "$VERIFY --print-targets files listed no release files"

verify() {
  # shellcheck disable=SC2086 # RUN_FLAG is intentionally empty or split into flags
  "$PYTHON" "$VERIFY" --lock "$LOCK" ${SELECT_FLAGS[@]+"${SELECT_FLAGS[@]}"} --expect-all $RUN_FLAG "$@"
}

RELEASE="$(lock_value release)"
REPO="$(lock_value repo)"

hashes_pinned=1
for file in "${FILES[@]}"; do
  [[ -n "$(lock_hash "$file")" ]] || hashes_pinned=0
done

if [[ "$MODE" == check ]]; then
  [[ $hashes_pinned -eq 1 ]] || die "the lock has no checksums for $RELEASE (run --update-lock after publishing it)"
  verify --require-hashes "$BINARIES_DIR"
  echo "Installed sidecar binaries match $RELEASE."
  exit 0
fi

if [[ "$MODE" == install ]]; then
  [[ $hashes_pinned -eq 1 ]] \
    || die "the lock has no checksums for $RELEASE. If the release is published, run: bash $0 --update-lock"
  up_to_date=1
  for file in "${FILES[@]}"; do
    if [[ ! -f "$BINARIES_DIR/$file" || "$(file_hash "$BINARIES_DIR/$file")" != "$(lock_hash "$file")" ]]; then
      up_to_date=0
    fi
  done
  if [[ $up_to_date -eq 1 ]]; then
    verify --require-hashes "$BINARIES_DIR"
    echo "Sidecar binaries already match $RELEASE."
    exit 0
  fi
fi

# ---------------------------------------------------------------------------
# Download the whole release into a staging directory next to the install
# location, so the final moves are atomic renames on the same filesystem.
# ---------------------------------------------------------------------------
mkdir -p "$BINARIES_DIR"
STAGING="$(mktemp -d "$BINARIES_DIR/.fetch-XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT

echo "Fetching $RELEASE from $REPO"
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  for file in "${FILES[@]}" SHA256SUMS.txt; do
    gh release download "$RELEASE" --repo "$REPO" --pattern "$file" --dir "$STAGING"
  done
else
  command -v curl >/dev/null 2>&1 || die "install gh or curl to download the binaries"
  for file in "${FILES[@]}" SHA256SUMS.txt; do
    curl --fail --location --silent --show-error --retry 3 \
      --output "$STAGING/$file" \
      "https://github.com/$REPO/releases/download/$RELEASE/$file"
  done
fi

# The release's own checksum list catches a truncated or corrupted download.
for file in "${FILES[@]}"; do
  published="$(awk -v file="$file" '$2 == file || $2 == "*" file { print $1 }' "$STAGING/SHA256SUMS.txt")"
  [[ -n "$published" ]] || die "$RELEASE's SHA256SUMS.txt does not list $file"
  [[ "$(file_hash "$STAGING/$file")" == "$published" ]] || die "$file does not match $RELEASE's SHA256SUMS.txt"
done
rm "$STAGING/SHA256SUMS.txt"
chmod +x "$STAGING"/llama-server-*

if [[ "$MODE" == update-lock ]]; then
  # Prove the published files are sound, and that the host's binary really ran,
  # before pinning them.
  unhashed_lock="$STAGING/unhashed.lock"
  grep -v '^sha256 ' "$LOCK" > "$unhashed_lock"
  # shellcheck disable=SC2086
  "$PYTHON" "$VERIFY" --lock "$unhashed_lock" --expect-all $RUN_FLAG "$STAGING"
  rm "$unhashed_lock"

  # Show the pins that would change, and never replace one by accident.
  echo "Pins for $RELEASE in $LOCK:"
  changes=0
  repinned=
  for file in "${FILES[@]}"; do
    new="$(file_hash "$STAGING/$file")"
    old="$(lock_hash "$file")"
    if [[ -z "$old" ]]; then
      echo "  + sha256 $new $file"
      changes=1
    elif [[ "$old" != "$new" ]]; then
      echo "  - sha256 $old $file"
      echo "  + sha256 $new $file"
      repinned="$repinned $file"
      changes=1
    else
      echo "    sha256 $new $file (unchanged)"
    fi
  done
  if [[ -n "$repinned" && $REPIN -ne 1 ]]; then
    die "the lock already pins a different hash for:$repinned. A published release is immutable, so investigate why its bytes changed; pass --repin once you are sure."
  fi
  if [[ $changes -eq 0 ]]; then
    echo "Nothing to pin: $LOCK already matches $RELEASE."
  else
    {
      grep -v '^sha256 ' "$LOCK"
      for file in "${FILES[@]}"; do
        echo "sha256 $(file_hash "$STAGING/$file") $file"
      done
    } > "$LOCK.tmp"
    mv "$LOCK.tmp" "$LOCK"
    echo "Pinned $RELEASE checksums in $LOCK (commit this change)."
  fi
else
  for file in "${FILES[@]}"; do
    actual="$(file_hash "$STAGING/$file")"
    [[ "$actual" == "$(lock_hash "$file")" ]] \
      || die "$file from $RELEASE has sha256 $actual, but the lock pins $(lock_hash "$file"). Published releases must not change; investigate before trusting it."
  done
  verify --require-hashes "$STAGING"
fi

# ---------------------------------------------------------------------------
# Install: replace the sidecars and drop anything stale.
# ---------------------------------------------------------------------------
for stale in "$BINARIES_DIR"/llama-server-* "$BINARIES_DIR"/SHA256SUMS.txt; do
  [[ -e "$stale" ]] || continue
  keep=0
  for file in "${FILES[@]}"; do
    [[ "$(basename "$stale")" == "$file" ]] && keep=1
  done
  [[ $keep -eq 1 ]] || rm -f "$stale"
done
for file in "${FILES[@]}"; do
  mv -f "$STAGING/$file" "$BINARIES_DIR/$file"
done

echo "Installed $RELEASE into $BINARIES_DIR"
