#!/usr/bin/env python3
"""
Smart Panic Scanner V3 - TRUE Production Unwraps Only

Scans Rust codebase for panic sites while EXCLUDING:
- Test code (#[cfg(test)], mod tests)
- Mock implementations (MockXXX structs/impls)
- Test utility files (mocks.rs, test_utils.rs, tests/ directory)

Oracle Directive: Operation Sniper - Precision targeting of real production panics
"""

import re
import sys
from pathlib import Path
from typing import Dict, List, Set
from dataclasses import dataclass
from enum import Enum


class ScanMode(Enum):
    PRODUCTION = "production"
    TEST = "test"
    MOCK = "mock"


@dataclass
class PanicSite:
    file_path: str
    line_number: int
    panic_type: str
    context: str


class RustScannerV3:
    """
    Context-aware Rust scanner that excludes test code AND mock implementations.

    V3 Improvements over V2:
    - Excludes mocks.rs, test_utils.rs files entirely
    - Detects Mock struct/impl blocks
    - Excludes /tests/ and /e2e/ directories
    """

    def __init__(self):
        self.panic_patterns = {
            'unwrap': re.compile(r'\.unwrap\(\)'),
            'expect': re.compile(r'\.expect\('),
            'panic': re.compile(r'panic!\('),
            'unreachable': re.compile(r'unreachable!\('),
        }

    def should_exclude_file(self, file_path: Path) -> bool:
        """Check if entire file should be excluded."""
        file_name = file_path.name
        path_str = str(file_path)

        # Exclude mock files
        if file_name in ('mocks.rs', 'test_utils.rs', 'test_helpers.rs'):
            return True

        # Exclude test directories
        if '/tests/' in path_str or '/e2e/' in path_str:
            return True

        return False

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
        return bool(re.match(r'(pub\s+)?mod\s+(tests?|test_\w+)\s*\{', stripped))

    def is_mock_struct_or_impl(self, line: str) -> bool:
        """Check if line starts a Mock struct or impl block."""
        stripped = line.strip()

        # Match: struct MockXXX {
        # Match: pub struct MockXXX {
        if re.match(r'(pub\s+)?struct\s+Mock\w+', stripped):
            return True

        # Match: impl MockXXX {
        # Match: impl<T> MockXXX<T> {
        if re.match(r'impl(<[^>]+>)?\s+Mock\w+', stripped):
            return True

        # Match: impl XXX for MockYYY {
        if re.search(r'for\s+Mock\w+', stripped):
            return True

        return False

    def scan_file(self, file_path: Path) -> Dict[str, List[PanicSite]]:
        """
        Scan a single Rust file for panic sites.

        Returns dict with keys: 'production', 'test', 'mock'
        """
        results = {'production': [], 'test': [], 'mock': []}

        # Check if entire file should be excluded
        if self.should_exclude_file(file_path):
            return results  # Skip this file entirely

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                lines = f.readlines()
        except Exception as e:
            print(f"Error reading {file_path}: {e}", file=sys.stderr)
            return results

        mode = ScanMode.PRODUCTION
        brace_depth = 0
        block_start_depth: Dict[ScanMode, int] = {}

        for line_num, line in enumerate(lines, start=1):
            # Check for test attributes
            if self.is_test_attribute(line):
                mode = ScanMode.TEST
                continue

            # Check for test module start
            if self.is_test_mod_start(line):
                mode = ScanMode.TEST
                block_start_depth[ScanMode.TEST] = brace_depth
                brace_depth += line.count('{') - line.count('}')
                continue

            # Check for Mock struct/impl start
            if self.is_mock_struct_or_impl(line):
                mode = ScanMode.MOCK
                block_start_depth[ScanMode.MOCK] = brace_depth
                brace_depth += line.count('{') - line.count('}')
                continue

            # Track brace depth
            brace_depth += line.count('{') - line.count('}')

            # Exit test/mock mode when we leave the block
            for scan_mode in [ScanMode.TEST, ScanMode.MOCK]:
                if mode == scan_mode and scan_mode in block_start_depth:
                    if brace_depth <= block_start_depth[scan_mode]:
                        mode = ScanMode.PRODUCTION
                        del block_start_depth[scan_mode]

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
                    elif mode == ScanMode.MOCK:
                        results['mock'].append(site)
                    else:  # TEST
                        results['test'].append(site)

        return results

    def scan_directory(self, root_dir: Path) -> Dict[str, Dict[str, int]]:
        """
        Scan entire directory recursively.

        Returns dict: {file_path: {unwrap: count, expect: count, ...}}
        Only includes files with production panics.
        """
        file_counts = {}

        for rust_file in root_dir.rglob('*.rs'):
            # Skip target directory
            if 'target' in rust_file.parts:
                continue

            # Scan file
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

    print(f"✅ Heatmap v3 generated: {output_path}")
    print(f"📊 Total files with TRUE production panics: {len(rows)}")

    if rows:
        print(f"\n🎯 Top 10 TRUE production risks:")
        for i, row in enumerate(rows[:10], start=1):
            print(f"   {i}. {row['file']} ({row['total']} panic sites)")
    else:
        print("\n🎉 ZERO production panics found! Codebase is production-safe!")

    return rows


def main():
    """Main entry point."""
    script_dir = Path(__file__).parent
    src_dir = script_dir.parent / 'src'

    if not src_dir.exists():
        print(f"Error: Source directory not found: {src_dir}", file=sys.stderr)
        sys.exit(1)

    print("🎯 OPERATION SNIPER: Scanning for TRUE production panic sites...")
    print("📋 Excluding:")
    print("   - #[cfg(test)], mod tests {}, #[test] functions")
    print("   - Mock implementations (struct MockXXX, impl MockXXX)")
    print("   - Mock files (mocks.rs, test_utils.rs)")
    print("   - Test directories (/tests/, /e2e/)")
    print()

    scanner = RustScannerV3()
    file_counts = scanner.scan_directory(src_dir)

    # Generate CSV
    output_path = script_dir.parent / 'docs' / 'audit' / 'panic_heatmap_v3.csv'
    output_path.parent.mkdir(parents=True, exist_ok=True)
    rows = generate_heatmap_csv(file_counts, output_path)

    # Summary statistics
    total_unwraps = sum(counts.get('unwrap', 0) for counts in file_counts.values())
    total_expects = sum(counts.get('expect', 0) for counts in file_counts.values())
    total_panics = sum(counts.get('panic', 0) for counts in file_counts.values())
    total_unreachables = sum(counts.get('unreachable', 0) for counts in file_counts.values())
    total_all = total_unwraps + total_expects + total_panics + total_unreachables

    print(f"\n📈 TRUE Production Panic Summary:")
    print(f"   Unwraps: {total_unwraps}")
    print(f"   Expects: {total_expects}")
    print(f"   Panics: {total_panics}")
    print(f"   Unreachables: {total_unreachables}")
    print(f"   ═══════════════════════")
    print(f"   TOTAL: {total_all}")
    print()

    # Comparison with v2
    v2_total = 88  # From previous scan
    reduction = ((v2_total - total_all) / v2_total * 100) if v2_total > 0 else 0
    print(f"🔍 Heatmap Comparison:")
    print(f"   v1 (with tests): 2,698 sites")
    print(f"   v2 (no tests): 88 sites")
    print(f"   v3 (no tests/mocks): {total_all} sites")
    print(f"   Mock noise eliminated: {v2_total - total_all} sites ({reduction:.1f}%)")


if __name__ == '__main__':
    main()
