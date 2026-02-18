#!/usr/bin/env python3
"""Script to check for missing return type hints in Python files."""

from __future__ import annotations

import ast
import argparse
from pathlib import Path
from typing import List, Tuple


class TypeHintChecker(ast.NodeVisitor):
    """AST visitor to check for missing return type hints."""

    def __init__(self):
        self.missing_hints: List[Tuple[str, int, str]] = []
        self.has_annotations = False

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Check if file imports future annotations."""
        if node.module == "__future__" and any(
            alias.name == "annotations" for alias in node.names
        ):
            self.has_annotations = True
        self.generic_visit(node)

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Check if function has return type hint."""
        # Skip private functions (start with _)
        if node.name.startswith("_"):
            self.generic_visit(node)
            return

        # Check if function has return annotation
        if node.returns is None:
            # Check if it's an async function
            is_async = isinstance(node, ast.AsyncFunctionDef)
            func_type = "async def" if is_async else "def"
            self.missing_hints.append((node.name, node.lineno, func_type))

        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Check async functions."""
        # Use same logic as regular functions
        self.visit_FunctionDef(node)


def check_file(file_path: Path) -> Tuple[bool, List[Tuple[str, int, str]]]:
    """Check a Python file for missing type hints.

    Returns:
        Tuple of (has_future_annotations, missing_hints)
    """
    try:
        content = file_path.read_text(encoding='utf-8')
        tree = ast.parse(content, filename=str(file_path))

        checker = TypeHintChecker()
        checker.visit(tree)

        return checker.has_annotations, checker.missing_hints

    except Exception as e:
        print(f"Error checking {file_path}: {e}")
        return False, []


def check_directory(directory: Path, pattern: str = "**/*.py") -> dict[str, dict]:
    """Check all Python files in directory.

    Returns:
        Dictionary mapping file paths to analysis results
    """
    results = {}

    for file_path in sorted(directory.glob(pattern)):
        # Skip __pycache__ and similar
        if '__pycache__' in str(file_path) or file_path.name.startswith('.'):
            continue

        has_annotations, missing_hints = check_file(file_path)

        if not has_annotations or missing_hints:
            results[str(file_path)] = {
                'has_annotations': has_annotations,
                'missing_hints': missing_hints
            }

    return results


def print_report(results: dict[str, dict]) -> None:
    """Print analysis report."""
    if not results:
        print("\n✅ All files have future annotations and complete type hints!")
        return

    total_files = len(results)
    total_missing = sum(len(data['missing_hints']) for data in results.values())
    files_without_annotations = sum(
        1 for data in results.values() if not data['has_annotations']
    )

    print(f"\n{'='*80}")
    print("TYPE HINT ANALYSIS REPORT")
    print(f"{'='*80}\n")

    print(f"Files analyzed: {total_files}")
    print(f"Files without future annotations: {files_without_annotations}")
    print(f"Total functions missing return types: {total_missing}\n")

    print(f"{'='*80}")
    print("DETAILED RESULTS")
    print(f"{'='*80}\n")

    for file_path, data in results.items():
        rel_path = file_path.replace('/home/user/Recall/vault/backend/', '')

        issues = []
        if not data['has_annotations']:
            issues.append("❌ Missing future annotations")
        if data['missing_hints']:
            issues.append(f"⚠️  {len(data['missing_hints'])} functions missing return types")

        print(f"\n📄 {rel_path}")
        for issue in issues:
            print(f"   {issue}")

        if data['missing_hints']:
            for func_name, line_no, func_type in data['missing_hints']:
                print(f"      Line {line_no:4d}: {func_type} {func_name}(...)")


def main():
    parser = argparse.ArgumentParser(description='Check for missing type hints')
    parser.add_argument('path', type=Path, help='Directory or file to check')
    parser.add_argument('--pattern', default='**/*.py', help='Glob pattern for files')
    parser.add_argument('--json', action='store_true', help='Output as JSON')

    args = parser.parse_args()

    if args.path.is_file():
        has_annotations, missing_hints = check_file(args.path)
        if args.json:
            import json
            print(json.dumps({
                'has_annotations': has_annotations,
                'missing_hints': missing_hints
            }, indent=2))
        else:
            if not has_annotations:
                print(f"❌ {args.path}: Missing future annotations")
            if missing_hints:
                print(f"⚠️  {args.path}: {len(missing_hints)} functions missing return types")
                for func_name, line_no, func_type in missing_hints:
                    print(f"   Line {line_no:4d}: {func_type} {func_name}(...)")
    else:
        results = check_directory(args.path, args.pattern)
        if args.json:
            import json
            print(json.dumps(results, indent=2))
        else:
            print_report(results)


if __name__ == '__main__':
    main()
