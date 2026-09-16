#!/usr/bin/env bash
# Parse Rust syntax rather than truncating files at the first test item.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo run --quiet --locked --manifest-path "$ROOT/scripts/rust-architecture-check/Cargo.toml" -- "$ROOT/src/src/src" "$ROOT/api-rust/src"
