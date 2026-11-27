import type { SearchResult } from '@/types';

const INTERNAL_LATTICE_PREFIX = /(?:^|[\\/])\.lattice[\\/]files[\\/][^\\/]+[\\/]/i;

const CODE_EXTENSIONS = new Set([
  'ts',
  'tsx',
  'js',
  'jsx',
  'mjs',
  'cjs',
  'rs',
  'py',
  'go',
  'java',
  'kt',
  'swift',
  'c',
  'cpp',
  'cc',
  'h',
  'hpp',
  'cs',
  'rb',
  'php',
  'css',
  'scss',
  'html',
  'xml',
  'json',
  'toml',
  'yaml',
  'yml',
  'sql',
  'sh',
  'bash',
  'zsh',
]);

const MARKDOWN_EXTENSIONS = new Set(['md', 'mdx', 'markdown']);
const TEXT_EXTENSIONS = new Set(['txt', 'rtf', 'org', 'rst']);
const DOC_EXTENSIONS = new Set(['doc', 'docx', 'odt', 'pages', 'epub']);
const DATA_EXTENSIONS = new Set(['csv', 'tsv', 'xlsx', 'xls', 'parquet']);

export interface SearchFacetOption {
  value: string;
  count: number;
}

export interface SearchFacets {
  types: SearchFacetOption[];
  sources: SearchFacetOption[];
}

/**
 * Strip internal `.lattice/files/<id>/` storage prefixes so UI never leaks raw
 * vault implementation paths.
 */
export function cleanSearchPath(path: string | undefined): string | null {
  if (!path) return null;
  const trimmed = path.trim();
  if (!trimmed) return null;
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }
  const stripped = trimmed.replace(INTERNAL_LATTICE_PREFIX, '');
  return stripped || trimmed;
}

/**
 * Human-readable location breadcrumb for a search result:
 * "Papers › Halvorsen" or "example.org".
 */
export function describeSearchLocation(path: string | undefined): string | null {
  if (!path) return null;
  const trimmed = path.trim();
  if (!trimmed) return null;
  if (/^https?:\/\//i.test(trimmed)) {
    try {
      return new URL(trimmed).hostname.replace(/^www\./, '');
    } catch {
      return trimmed;
    }
  }
  const hadInternalPrefix = INTERNAL_LATTICE_PREFIX.test(trimmed);
  const cleaned = trimmed.replace(INTERNAL_LATTICE_PREFIX, '');
  const parts = cleaned.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 1) {
    return hadInternalPrefix ? 'Library' : null;
  }
  return parts.slice(Math.max(0, parts.length - 3), -1).join(' › ');
}

function getExtension(value: string | undefined): string | null {
  if (!value) return null;
  const clean = value.split(/[?#]/)[0] ?? value;
  const match = /\.([a-z0-9]{1,8})$/i.exec(clean.trim());
  return match ? match[1].toLowerCase() : null;
}

export function inferSearchResultType(result: SearchResult): string {
  const path = result.path || (result.metadata.path as string | undefined) || '';
  if (/^https?:\/\//i.test(path)) {
    return 'Web';
  }

  const metaType = typeof result.metadata.fileType === 'string'
    ? result.metadata.fileType.trim().toLowerCase()
    : '';

  const ext =
    getExtension(path) ||
    getExtension(typeof result.metadata.filename === 'string' ? result.metadata.filename : undefined) ||
    getExtension(result.title);

  if (ext === 'pdf' || metaType === 'pdf') return 'PDF';
  if ((ext && MARKDOWN_EXTENSIONS.has(ext)) || metaType === 'markdown' || metaType === 'md') {
    return 'Markdown';
  }
  if ((ext && CODE_EXTENSIONS.has(ext)) || metaType === 'code') return 'Code';
  if ((ext && TEXT_EXTENSIONS.has(ext)) || metaType === 'text' || metaType === 'txt') return 'Text';
  if ((ext && DATA_EXTENSIONS.has(ext)) || metaType === 'csv' || metaType === 'data') return 'Data';
  if ((ext && DOC_EXTENSIONS.has(ext)) || metaType === 'document') return 'Document';

  return 'Document';
}

export function inferSearchResultSource(result: SearchResult): string | null {
  const path = result.path || (result.metadata.path as string | undefined);
  if (!path) return null;
  if (/^https?:\/\//i.test(path)) {
    try {
      return new URL(path).hostname.replace(/^www\./, '');
    } catch {
      return 'Web';
    }
  }
  const hadInternalPrefix = INTERNAL_LATTICE_PREFIX.test(path);
  const cleaned = path.replace(INTERNAL_LATTICE_PREFIX, '');
  const parts = cleaned.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 1) {
    return hadInternalPrefix ? 'Library' : null;
  }
  return parts[parts.length - 2] ?? null;
}

function toSortedOptions(counts: Map<string, number>): SearchFacetOption[] {
  return Array.from(counts.entries())
    .map(([value, count]) => ({ value, count }))
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));
}

export function extractSearchFacets(results: SearchResult[]): SearchFacets {
  const typeCounts = new Map<string, number>();
  const sourceCounts = new Map<string, number>();

  for (const result of results) {
    const type = inferSearchResultType(result);
    typeCounts.set(type, (typeCounts.get(type) ?? 0) + 1);

    const source = inferSearchResultSource(result);
    if (source) {
      sourceCounts.set(source, (sourceCounts.get(source) ?? 0) + 1);
    }
  }

  return {
    types: toSortedOptions(typeCounts),
    sources: toSortedOptions(sourceCounts).slice(0, 6),
  };
}

export function filterSearchResults(
  results: SearchResult[],
  activeType: string | null,
  activeSource: string | null,
): SearchResult[] {
  if (!activeType && !activeSource) return results;
  return results.filter((result) => {
    if (activeType && inferSearchResultType(result) !== activeType) {
      return false;
    }
    if (activeSource && inferSearchResultSource(result) !== activeSource) {
      return false;
    }
    return true;
  });
}
