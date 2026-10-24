import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { useCorpusIdentity } from './useCorpusIdentity';

const doc = (fileType: string, filePath = `/notes/x.${fileType}`) =>
  ({ id: filePath, fileName: 'x', filePath, fileType, category: 'document' }) as DocumentMetadata;

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

  it('is empty for an empty library', () => {
    const { result } = renderHook(() => useCorpusIdentity([]));
    expect(result.current).toEqual({ totalCount: 0, buckets: [] });
  });
});
