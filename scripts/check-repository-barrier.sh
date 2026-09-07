#!/usr/bin/env bash
# Repository Barrier (SSOT) check.
#
# High-level feature code must not decide persisted state by inspecting the
# filesystem, and orchestration code must not own SQL. Filesystem-backed
# repositories and legitimate file-resource workflows may opt out at a
# specific call site with:
#
#   // repository-barrier-allow: <why the file itself is the resource>
#
# The marker must be on the matching line or one of the two lines above it.
# Test modules are excluded: assertions about their own fixtures are not
# application state decisions.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src/app/src/src/features"

if [[ ! -d "$SRC" ]]; then
  echo "error: features dir not found at $SRC" >&2
  exit 2
fi

FS_PATTERN='((tokio::fs|std::fs)::(read_dir|metadata|try_exists)|\.(exists|is_file|is_dir|try_exists)\(\))'
SQL_PATTERN='sqlx::(query|query_as|query_scalar|raw_sql)'
violations=0
checked_files=0

is_high_level_file() {
  local file="$1"
  local base
  base="$(basename "$file")"
  [[ "$file" == */use_cases/* ]] ||
    [[ "$base" == commands*.rs ]] ||
    [[ "$base" == watcher*.rs ]] ||
    [[ "$base" == service*.rs ]] ||
    [[ "$base" == repository*.rs ]]
}

owns_no_sql() {
  local file="$1"
  local base
  base="$(basename "$file")"
  [[ "$file" == */use_cases/* ]] ||
    [[ "$base" == commands*.rs ]] ||
    [[ "$base" == watcher*.rs ]] ||
    [[ "$base" == service*.rs ]]
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
      rg -n "$pattern" || true
  )
}

while IFS= read -r -d '' file; do
  if ! is_high_level_file "$file"; then
    continue
  fi
  checked_files=$((checked_files + 1))
  check_matches "$file" "filesystem state" "$FS_PATTERN"
  if owns_no_sql "$file"; then
    check_matches "$file" "raw SQL outside repository" "$SQL_PATTERN"
  fi
done < <(find "$SRC" -type f -name '*.rs' -print0)

if ((violations > 0)); then
  echo
  echo "Found $violations Repository Barrier violation(s) across $checked_files production feature files."
  echo "See CLAUDE.md 'ARCHITECTURAL RULE: Repository Barrier (SSOT)'."
  exit 1
fi

echo "Repository Barrier check: clean ($checked_files production feature files)."
