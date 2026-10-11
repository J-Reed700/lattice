#!/usr/bin/env bash
# Parse Rust syntax rather than truncating files at the first test item.
#
# Ratcheted rules compare against scripts/rust-architecture-check/baseline.txt.
# `--write-baseline` rewrites that file and the Tauri command-orphan baseline
# from the current tree, so one command regenerates both.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
status=0
cargo run --quiet --locked --manifest-path "$ROOT/scripts/rust-architecture-check/Cargo.toml" -- "$ROOT/src-tauri/src" "$ROOT/api-rust/src" "$@" || status=$?
for argument in "$@"; do
  if [[ "$argument" == "--write-baseline" ]]; then
    python3 "$ROOT/scripts/check-tauri-command-inventory.py" --write-baseline || status=$?
  fi
done
exit "$status"
