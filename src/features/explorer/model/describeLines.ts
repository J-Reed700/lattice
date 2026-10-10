import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';

/** "line 12" or "lines 10–24". */
export function describeLines(range: ExplorerLineRange): string {
  return range.startLine === range.endLine ? `line ${range.startLine}` : `lines ${range.startLine}–${range.endLine}`;
}

/** The last segment of a scope-relative path. */
export function baseName(path: string): string {
  return path.split('/').pop() ?? path;
}
