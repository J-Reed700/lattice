#!/usr/bin/env bash
# Repository Barrier (SSOT) check.
#
# High-level feature code must not decide persisted state by inspecting the
# filesystem. Filesystem-backed repositories and legitimate file-resource
# workflows may opt out at a specific call site with:
#
#   // repository-barrier-allow: <why the file itself is the resource>
#
# The marker must be on the matching line or one of the two lines above it.
# Test modules are excluded: assertions about their own fixtures are not
# application state decisions.
#
# SQL outside data modules is checked by the architecture check
# (scripts/rust-architecture-check, rule 2), which parses the code.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# The app's features, and the learning feature split out of them into its own crate.
SOURCES=("$ROOT/crates/lattice-desktop/src/features" "$ROOT/crates/lattice-learning/src")

for src in "${SOURCES[@]}"; do
  if [[ ! -d "$src" ]]; then
    echo "error: source dir not found at $src" >&2
    exit 2
  fi
done

FS_PATTERN='((tokio::fs|std::fs)::(read_dir|metadata|try_exists)|\.(exists|is_file|is_dir|try_exists)\(\))'
violations=0
checked_files=0

is_high_level_file() {
  local file="$1"
  local base
  base="$(basename "$file")"
  [[ "$file" == */use_cases/* ]] ||
    [[ "$file" == */commands/* ]] ||
    [[ "$file" == */plugin/* ]] ||
    [[ "$file" == */repository/* ]] ||
    [[ "$base" == commands*.rs ]] ||
    [[ "$base" == plugin*.rs ]] ||
    [[ "$base" == watcher*.rs ]] ||
    [[ "$base" == service*.rs ]] ||
    [[ "$base" == repository*.rs ]]
}

has_allow_marker() {
  local file="$1"
  local line_num="$2"
  local start=1
  if ((line_num > 2)); then
    start=$((line_num - 2))
  fi
  sed -n "${start},${line_num}p" "$file" | grep -q 'repository-barrier-allow:'
}

check_matches() {
  local file="$1"
  local kind="$2"
  local pattern="$3"
  local match line_num line_content

  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    line_num="${match%%:*}"
    line_content="${match#*:}"
    if has_allow_marker "$file" "$line_num"; then
      continue
    fi

    echo "VIOLATION [$kind]: $file:$line_num"
    echo "  $line_content"
    echo "  Add '// repository-barrier-allow: <reason>' immediately above to justify."
    violations=$((violations + 1))
  done < <(
    # Repository-barrier checks apply to production code. In this codebase,
    # cfg(test) modules are terminal sections of their source files.
    awk '/^[[:space:]]*#\[cfg\(test\)\]/{exit} {print}' "$file" |
      grep -nE "$pattern" || true
  )
}

while IFS= read -r -d '' file; do
  if ! is_high_level_file "$file"; then
    continue
  fi
  # Test modules split into their own files (`mod tests;` under #[cfg(test)]).
  case "$file" in
    */tests.rs | *_tests.rs | */tests/*) continue ;;
  esac
  checked_files=$((checked_files + 1))
  check_matches "$file" "filesystem state" "$FS_PATTERN"
done < <(find "${SOURCES[@]}" -type f -name '*.rs' -print0)

if ((violations > 0)); then
  echo
  echo "Found $violations Repository Barrier violation(s) across $checked_files production feature files."
  echo "See CONTRIBUTING.md for the repository boundary rules."
  exit 1
fi

echo "Repository Barrier check: clean ($checked_files production feature files)."
