import { describe, expect, it } from 'vitest';

import { toJournalCitationSource } from '@/features/journal/model/journalCitationSources';
import type { SourceDto } from '@/lib/bindings';


describe('toJournalCitationSource', () => {
  it('preserves the citation-specific immutable web snapshot', () => {
    const snapshot = {
      url: 'https://example.org/article',
      title: 'Captured title',
      text: 'Captured article text.',
      fetchedAt: '2026-09-20T00:00:00.000Z',
      truncated: false,
    };
    const source = {
      documentId: 'web:article',
      chunkId: 'web-content-1',
      content: 'Search excerpt.',
      score: 1,
      path: null,
      position: null,
      fileName: 'Article',
      filePath: snapshot.url,
      mimeType: 'text/html',
      category: 'Web Article',
      fileSizeBytes: 0,
      modifiedAt: '',
      webSnapshot: snapshot,
    } as unknown as SourceDto;

    expect(toJournalCitationSource(source).webSnapshot).toEqual(snapshot);
  });
});
