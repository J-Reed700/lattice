#!/usr/bin/env python3
"""Script to add 'from __future__ import annotations' to Python files."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import List


def has_future_annotations(content: str) -> bool:
    """Check if file already has future annotations import."""
    lines = content.split('\n')
    for line in lines[:10]:  # Check first 10 lines
        if line.strip() == 'from __future__ import annotations':
            return True
    return False


def add_future_annotations(file_path: Path) -> bool:
    """Add future annotations to a Python file.

    Returns:
        True if file was modified, False otherwise
    """
    try:
        content = file_path.read_text(encoding='utf-8')
    except Exception as e:
        print(f"Error reading {file_path}: {e}")
        return False

    # Skip if already has future annotations
    if has_future_annotations(content):
        print(f"✓ {file_path} already has future annotations")
        return False

    # Skip empty files
    if not content.strip():
        print(f"⊘ {file_path} is empty, skipping")
        return False

    lines = content.split('\n')

    # Find insertion point (after shebang and docstring if present)
    insert_index = 0
    in_docstring = False
    docstring_char = None

    for i, line in enumerate(lines):
        stripped = line.strip()

        # Skip shebang
        if i == 0 and stripped.startswith('#!'):
            insert_index = i + 1
            continue

        # Handle module docstring
        if not in_docstring and (stripped.startswith('"""') or stripped.startswith("'''")):
            docstring_char = '"""' if stripped.startswith('"""') else "'''"
            if stripped.endswith(docstring_char) and len(stripped) > 3:
                # Single line docstring
                insert_index = i + 1
                continue
            else:
                in_docstring = True
                continue

        if in_docstring:
            if docstring_char in stripped:
                in_docstring = False
                insert_index = i + 1
            continue

        # Found first non-comment, non-docstring line
        if stripped and not stripped.startswith('#'):
            break

    # Add blank line before if there's content before
    if insert_index > 0 and lines[insert_index - 1].strip():
        lines.insert(insert_index, '')
        insert_index += 1

    # Insert future annotations
    lines.insert(insert_index, 'from __future__ import annotations')

    # Add blank line after if next line is not blank
    if insert_index + 1 < len(lines) and lines[insert_index + 1].strip():
        lines.insert(insert_index + 1, '')

    # Write back
    try:
        file_path.write_text('\n'.join(lines), encoding='utf-8')
        print(f"✓ Added future annotations to {file_path}")
        return True
    except Exception as e:
        print(f"Error writing {file_path}: {e}")
        return False


def process_directory(directory: Path, pattern: str = "**/*.py", exclude: List[str] = None) -> tuple[int, int]:
    """Process all Python files in directory.

    Returns:
        Tuple of (modified_count, total_count)
    """
    exclude = exclude or []
    modified = 0
    total = 0

    for file_path in sorted(directory.glob(pattern)):
        # Skip excluded paths
        if any(excl in str(file_path) for excl in exclude):
            continue

        # Skip __pycache__ and similar
        if '__pycache__' in str(file_path) or file_path.name.startswith('.'):
            continue

        total += 1
        if add_future_annotations(file_path):
            modified += 1

    return modified, total


def main():
    parser = argparse.ArgumentParser(description='Add future annotations to Python files')
    parser.add_argument('path', type=Path, help='Directory or file to process')
    parser.add_argument('--pattern', default='**/*.py', help='Glob pattern for files')
    parser.add_argument('--exclude', nargs='*', default=[], help='Paths to exclude')

    args = parser.parse_args()

    if args.path.is_file():
        success = add_future_annotations(args.path)
        print(f"\n{'Modified' if success else 'No changes'}")
    else:
        modified, total = process_directory(args.path, args.pattern, args.exclude)
        print(f"\n{'='*60}")
        print(f"Modified: {modified}/{total} files")
        print(f"{'='*60}")


if __name__ == '__main__':
    main()
