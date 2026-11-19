import { describe, it, expect } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import { createCitationMap } from '../citations';

describe('createCitationMap', () => {
  const mockSources: SourceWithMetadata[] = [
    {
      documentId: 'doc1',
      chunkId: 'chunk1',
      fileName: 'file1.pdf',
      filePath: '/path/to/file1.pdf',
      mimeType: 'application/pdf',
      category: 'document',
      content: 'Content of first source',
      score: 0.95,
      fileSizeBytes: 1024,
      modifiedAt: '2025-01-01T00:00:00Z',
    },
    {
      documentId: 'doc2',
      chunkId: 'chunk2',
      fileName: 'file2.txt',
      filePath: '/path/to/file2.txt',
      mimeType: 'text/plain',
      category: 'text',
      content: 'Content of second source',
      score: 0.85,
      fileSizeBytes: 512,
      modifiedAt: '2025-01-02T00:00:00Z',
    },
  ];

  it('maps sources to 1-indexed numbers', () => {
    const map = createCitationMap(mockSources);

    expect(map.get(1)?.fileName).toBe('file1.pdf');
    expect(map.get(2)?.fileName).toBe('file2.txt');
  });

  it('returns Map with correct size', () => {
    const map = createCitationMap(mockSources);

    expect(map.size).toBe(2);
  });

  it('preserves all source metadata', () => {
    const map = createCitationMap(mockSources);
    const source1 = map.get(1);

    expect(source1).toEqual(mockSources[0]);
    expect(source1?.documentId).toBe('doc1');
    expect(source1?.score).toBe(0.95);
    expect(source1?.fileSizeBytes).toBe(1024);
  });

  it('handles empty source array', () => {
    const map = createCitationMap([]);

    expect(map.size).toBe(0);
  });

  it('handles single source', () => {
    const map = createCitationMap([mockSources[0]]);

    expect(map.size).toBe(1);
    expect(map.get(1)?.fileName).toBe('file1.pdf');
  });

  it('does not use 0-based indexing', () => {
    const map = createCitationMap(mockSources);

    expect(map.get(0)).toBeUndefined();
    expect(map.get(1)).toBeDefined();
  });

  it('handles many sources', () => {
    const manySources: SourceWithMetadata[] = Array.from({ length: 50 }, (_, i) => ({
      documentId: `doc${i}`,
      chunkId: `chunk${i}`,
      fileName: `file${i}.pdf`,
      filePath: `/path/to/file${i}.pdf`,
      mimeType: 'application/pdf',
      category: 'document',
      content: `Content ${i}`,
      score: 0.9 - i * 0.01,
      fileSizeBytes: 1024,
      modifiedAt: '2025-01-01T00:00:00Z',
    }));

    const map = createCitationMap(manySources);

    expect(map.size).toBe(50);
    expect(map.get(1)?.fileName).toBe('file0.pdf');
    expect(map.get(50)?.fileName).toBe('file49.pdf');
  });

  it('keys a source by the number it was given, not by its position', () => {
    const map = createCitationMap([
      { ...mockSources[0]!, citationId: 12 },
      { ...mockSources[1]!, citationId: 31 },
    ]);

    expect(map.get(12)?.fileName).toBe('file1.pdf');
    expect(map.get(31)?.fileName).toBe('file2.txt');
    expect(map.get(1)).toBeUndefined();
  });
});
