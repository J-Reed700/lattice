#!/usr/bin/env bash
# Enforce compile-time dependency direction for the Rust core.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_SRC="$ROOT/src/app/src/src"
violations=0

scan_layer() {
  local directory="$1"
  local forbidden_pattern="$2"
  local rule="$3"
  local file matches line

  while IFS= read -r -d '' file; do
    # Test-only modules may depend on adapters and mocks. In this codebase they
    # are terminal sections, so only scan production content above cfg(test).
    matches="$(
      awk '/^[[:space:]]*#\[cfg\(test\)\]/{exit} {print}' "$file" |
        rg -n "$forbidden_pattern" || true
    )"
    [[ -z "$matches" ]] && continue

    while IFS= read -r line; do
      echo "VIOLATION [$rule]: ${file#"$ROOT/"}:$line"
      violations=$((violations + 1))
    done <<< "$matches"
  done < <(find "$directory" -type f -name '*.rs' -print0)
}

scan_layer \
  "$RUST_SRC/domain" \
  'crate::(features|infrastructure|interfaces)::' \
  'domain may not depend on features, infrastructure, or interfaces'

scan_layer \
  "$RUST_SRC/application" \
  'crate::(features|infrastructure|interfaces)::' \
  'application may not depend on features, infrastructure, or interfaces'

if ((violations > 0)); then
  echo
  echo "Found $violations Rust layer-boundary violation(s)."
  exit 1
fi

echo "Rust layer boundary check: clean."
