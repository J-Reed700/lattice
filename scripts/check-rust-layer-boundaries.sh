#!/usr/bin/env bash
# Parse Rust syntax rather than truncating files at the first test item.
#
# Ratcheted rules compare against scripts/rust-architecture-check/baseline.txt.
# `--write-baseline` rewrites that file and the Tauri command-orphan baseline
# from the current tree, so one command regenerates both. Feature edges are held
# at zero, never baselined; the few allowed outright are in
# scripts/rust-architecture-check/feature-graph.txt.
# `--list <rule> [subject]` prints every site of a rule, baselined or not.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
status=0
cargo run --quiet --locked --manifest-path "$ROOT/scripts/rust-architecture-check/Cargo.toml" -- "$ROOT/crates/lattice-desktop/src" "$ROOT/crates/lattice-sync/src" --core "$ROOT/crates/lattice-core/src" "$@" || status=$?
for argument in "$@"; do
  if [[ "$argument" == "--write-baseline" ]]; then
    python3 "$ROOT/scripts/check-tauri-command-inventory.py" --write-baseline || status=$?
  fi
done
exit "$status"
