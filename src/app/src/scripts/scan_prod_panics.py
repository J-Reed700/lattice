#!/usr/bin/env python3
"""
Smart Panic Scanner - Production Unwraps Only

Scans Rust codebase for panic sites (unwrap, expect, panic!, unreachable!)
while EXCLUDING test code.

Context-aware parsing:
- Detects #[cfg(test)] attributes
- Detects mod tests { } blocks
- Tracks brace depth to ignore nested content
- Only counts production unwraps

Oracle Directive: Step 2 of Heatmap V2 generation
"""

import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple
from dataclasses import dataclass
from enum import Enum


class ScanMode(Enum):
    PRODUCTION = "production"
    TEST = "test"


@dataclass
class PanicSite:
    file_path: str
    line_number: int
    panic_type: str  # unwrap, expect, panic, unreachable
    context: str  # surrounding code


class RustScanner:
    """Context-aware Rust code scanner that excludes test code."""

    def __init__(self):
        self.panic_patterns = {
            'unwrap': re.compile(r'\.unwrap\(\)'),
            'expect': re.compile(r'\.expect\('),
            'panic': re.compile(r'panic!\('),
            'unreachable': re.compile(r'unreachable!\('),
        }

    def is_test_attribute(self, line: str) -> bool:
        """Check if line is a test attribute."""
        stripped = line.strip()
        return (
            stripped.startswith('#[cfg(test)]') or
            stripped.startswith('#[test]') or
            stripped.startswith('#[tokio::test]') or
            stripped.startswith('#[async_test]')
        )

    def is_test_mod_start(self, line: str) -> bool:
        """Check if line starts a test module."""
        stripped = line.strip()
        # Match: mod tests {
        # Match: mod test {
        # Match: pub mod tests {
        return bool(re.match(r'(pub\s+)?mod\s+(tests?|test_\w+)\s*\{', stripped))

    def scan_file(self, file_path: Path) -> Dict[str, List[PanicSite]]:
        """
        Scan a single Rust file for panic sites.

        Returns dict with keys: 'production', 'test'
        """
        results = {'production': [], 'test': []}

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                lines = f.readlines()
        except Exception as e:
            print(f"Error reading {file_path}: {e}", file=sys.stderr)
            return results

        mode = ScanMode.PRODUCTION
        brace_depth = 0
        test_start_depth = None  # Track where test block started

        for line_num, line in enumerate(lines, start=1):
            # Check for test attributes
            if self.is_test_attribute(line):
                # Next non-empty line might be test function or module
                # Mark as test mode temporarily
                mode = ScanMode.TEST
                continue

            # Check for test module start
            if self.is_test_mod_start(line):
                mode = ScanMode.TEST
                test_start_depth = brace_depth
                # Count opening brace
                brace_depth += line.count('{') - line.count('}')
                continue

            # Track brace depth
            brace_depth += line.count('{') - line.count('}')

            # Exit test mode when we leave the test block
            if mode == ScanMode.TEST and test_start_depth is not None:
                if brace_depth <= test_start_depth:
                    mode = ScanMode.PRODUCTION
                    test_start_depth = None

            # Scan for panic patterns
            for panic_type, pattern in self.panic_patterns.items():
                if pattern.search(line):
                    site = PanicSite(
                        file_path=str(file_path),
                        line_number=line_num,
                        panic_type=panic_type,
                        context=line.strip()
                    )

                    if mode == ScanMode.PRODUCTION:
                        results['production'].append(site)
                    else:
                        results['test'].append(site)

        return results

    def scan_directory(self, root_dir: Path) -> Dict[str, Dict[str, int]]:
        """
        Scan entire directory recursively.

        Returns dict: {file_path: {unwrap: count, expect: count, ...}}
        """
        file_counts = {}

        for rust_file in root_dir.rglob('*.rs'):
            # Skip target directory
            if 'target' in rust_file.parts:
                continue

            results = self.scan_file(rust_file)
            production_sites = results['production']

            if production_sites:
                # Count by type
                counts = {
                    'unwrap': 0,
                    'expect': 0,
                    'panic': 0,
                    'unreachable': 0,
                }

                for site in production_sites:
                    counts[site.panic_type] += 1

                total = sum(counts.values())
                if total > 0:
                    file_counts[str(rust_file.relative_to(root_dir))] = counts

        return file_counts


def generate_heatmap_csv(file_counts: Dict[str, Dict[str, int]], output_path: Path):
    """Generate CSV heatmap from scan results."""
    rows = []

    for file_path, counts in file_counts.items():
        unwrap = counts.get('unwrap', 0)
        expect = counts.get('expect', 0)
        panic = counts.get('panic', 0)
        unreachable = counts.get('unreachable', 0)
        total = unwrap + expect + panic + unreachable

        rows.append({
            'file': file_path,
            'unwrap': unwrap,
            'expect': expect,
            'panic': panic,
            'unreachable': unreachable,
            'total': total,
        })

    # Sort by total (descending)
    rows.sort(key=lambda x: x['total'], reverse=True)

    # Write CSV
    with open(output_path, 'w') as f:
        f.write('File,Unwrap_Count,Expect_Count,Panic_Count,Unreachable_Count,Total_Panic_Points\n')
        for row in rows:
            f.write(f"{row['file']},{row['unwrap']},{row['expect']},{row['panic']},{row['unreachable']},{row['total']}\n")

    print(f"✅ Heatmap v2 generated: {output_path}")
    print(f"📊 Total files with production panics: {len(rows)}")
    if rows:
        print(f"🔥 Top 5 risky files:")
        for i, row in enumerate(rows[:5], start=1):
            print(f"   {i}. {row['file']} ({row['total']} panic sites)")


def main():
    """Main entry point."""
    # Determine source directory
    script_dir = Path(__file__).parent
    src_dir = script_dir.parent / 'src'

    if not src_dir.exists():
        print(f"Error: Source directory not found: {src_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"🔍 Scanning {src_dir} for PRODUCTION panic sites...")
    print(f"📋 Excluding: #[cfg(test)], mod tests {{}}, #[test] functions")

    scanner = RustScanner()
    file_counts = scanner.scan_directory(src_dir)

    # Generate CSV
    output_path = script_dir.parent / 'docs' / 'audit' / 'panic_heatmap_v2.csv'
    output_path.parent.mkdir(parents=True, exist_ok=True)
    generate_heatmap_csv(file_counts, output_path)

    # Summary statistics
    total_unwraps = sum(counts.get('unwrap', 0) for counts in file_counts.values())
    total_expects = sum(counts.get('expect', 0) for counts in file_counts.values())
    total_panics = sum(counts.get('panic', 0) for counts in file_counts.values())
    total_unreachables = sum(counts.get('unreachable', 0) for counts in file_counts.values())

    print(f"\n📈 Summary:")
    print(f"   Unwraps: {total_unwraps}")
    print(f"   Expects: {total_expects}")
    print(f"   Panics: {total_panics}")
    print(f"   Unreachables: {total_unreachables}")
    print(f"   TOTAL: {total_unwraps + total_expects + total_panics + total_unreachables}")


if __name__ == '__main__':
    main()
