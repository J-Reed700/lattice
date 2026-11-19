import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { useCorpusIdentity } from './useCorpusIdentity';

const BUCKETS: Record<string, string> = { pdf: 'PDF', md: 'Markdown', html: 'HTML' };

const doc = (fileType: string, filePath = `/notes/x.${fileType}`, web = filePath.startsWith('https://')) =>
  ({
    id: filePath, fileName: 'x', filePath, fileType, category: 'document', web,
    typeBucket: web ? 'Web' : BUCKETS[fileType] ?? fileType.toUpperCase(),
  }) as DocumentMetadata;

describe('useCorpusIdentity', () => {
  it('counts documents per type, largest first, ties by name', () => {
    const documents = [doc('pdf'), doc('pdf'), doc('md'), doc('epub'), doc('html', 'https://example.org/bread')];
    const { result } = renderHook(() => useCorpusIdentity(documents));
    expect(result.current.totalCount).toBe(5);
    expect(result.current.buckets[0]).toEqual({ label: 'PDF', count: 2 });
    const rest = result.current.buckets.slice(1).map((bucket) => bucket.label);
    expect(rest).toEqual([...rest].sort((a, b) => a.localeCompare(b)));
    expect(rest).toContain('Web');
  });

  it('counts each document in the bucket the backend assigned it', () => {
    const documents = [
      doc('md', 'C:/data/articles/x/article.md', true),
      doc('md', '/notes/a.md', false),
      { ...doc('pdf', 'https://example.org/a.pdf', false), typeBucket: 'PDF' },
    ];
    const { result } = renderHook(() => useCorpusIdentity(documents));
    expect(result.current.buckets).toEqual([
      { label: 'Markdown', count: 1 },
      { label: 'PDF', count: 1 },
      { label: 'Web', count: 1 },
    ]);
  });

  it('is empty for an empty library', () => {
    const { result } = renderHook(() => useCorpusIdentity([]));
    expect(result.current).toEqual({ totalCount: 0, buckets: [] });
  });
});
