#!/usr/bin/env bash
# Check source dependency direction, including modules loaded via path aliases.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_SRC="$ROOT/src/app/src/src"
violations=0
scanned_files=""

scan_file() {
  local file="$1" forbidden_pattern="$2" rule="$3"
  local matches line alias target
  case "$scanned_files" in
    *"|$file|"*) return ;;
  esac
  scanned_files="$scanned_files|$file|"

  matches="$(
    awk '/^[[:space:]]*#\[cfg\(test\)\]/{exit} {print}' "$file" |
      rg -n "$forbidden_pattern" || true
  )"
  if [[ -n "$matches" ]]; then
    while IFS= read -r line; do
      echo "VIOLATION [$rule]: ${file#"$ROOT/"}:$line"
      violations=$((violations + 1))
    done <<< "$matches"
  fi

  # A module's physical location does not change its architectural ownership.
  # Follow explicit aliases so moving a file cannot hide its dependencies.
  while IFS= read -r alias; do
    target="$(dirname "$file")/$alias"
    if [[ ! -f "$target" ]]; then
      echo "Missing Rust module referenced by $file: $alias" >&2
      exit 1
    fi
    target="$(cd "$(dirname "$target")" && pwd)/$(basename "$target")"
    if [[ "$(basename "$target")" == "mod.rs" ]]; then
      scan_layer "$(dirname "$target")" "$forbidden_pattern" "$rule"
    else
      scan_file "$target" "$forbidden_pattern" "$rule"
    fi
  done < <(sed -n 's/^[[:space:]]*#\[path = "\([^"]*\)"\].*/\1/p' "$file")
}

scan_layer() {
  local directory="$1"
  local forbidden_pattern="$2"
  local rule="$3"
  local file

  while IFS= read -r -d '' file; do
    # Test-only modules may depend on adapters and mocks. In this codebase they
    # are terminal sections, so only scan production content above cfg(test).
    scan_file "$file" "$forbidden_pattern" "$rule"
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
