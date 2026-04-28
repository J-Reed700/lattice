#!/usr/bin/env bash
# Repository Barrier Rule check
#
# Per CLAUDE.md "Repository Barrier (SSOT)": no use_case may import
# tokio::fs / std::fs to make state decisions ("does X exist?").
#
# Filesystem use is legitimate when the file IS the resource (reading content,
# walking a user-provided ingestion path, deleting model artifacts). It is
# split-brain when the question being answered ("is this model installed?")
# could be answered by querying the repository instead.
#
# This check finds NEW violations: any use_case file that calls *::read_dir
# without an inline `// repository-barrier-allow:` justification comment.
#
# Exit 0 = clean. Exit 1 = unjustified violation found.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src/app/src/src/features"

if [[ ! -d "$SRC" ]]; then
  echo "error: features dir not found at $SRC" >&2
  exit 2
fi

# Find all use_cases/*.rs files that mention read_dir
violations=0
while IFS= read -r file; do
  # For each read_dir line, check if the line itself OR the line above
  # contains the allow marker.
  while IFS= read -r match; do
    line_num="${match%%:*}"
    above=$((line_num - 1))
    line_content=$(sed -n "${line_num}p" "$file")
    above_content=$(sed -n "${above}p" "$file")

    if echo "$line_content" | grep -q "repository-barrier-allow"; then continue; fi
    if echo "$above_content" | grep -q "repository-barrier-allow"; then continue; fi

    echo "VIOLATION: $file:$line_num"
    echo "  $line_content"
    echo "  Add '// repository-barrier-allow: <reason>' on the line above to justify."
    violations=$((violations + 1))
  done < <(grep -nE 'fs::read_dir|tokio::fs::read_dir|std::fs::read_dir' "$file" || true)
done < <(find "$SRC" -path '*/use_cases/*.rs' -type f)

if [[ $violations -gt 0 ]]; then
  echo ""
  echo "Found $violations Repository Barrier violation(s) in use_cases/."
  echo "See CLAUDE.md 'ARCHITECTURAL RULE: Repository Barrier (SSOT)'."
  exit 1
fi

echo "Repository Barrier check: clean ($SRC/*/use_cases/)."
exit 0
