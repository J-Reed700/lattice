import { describe, expect, it } from 'vitest';

import { SourcesArraySchema } from '../conversation';

describe('SourceWithMetadataSchema', () => {
  it('keeps a source whose page text, excerpt or URL is long', () => {
    // A fetched page carried a 4000-character excerpt; a length cap dropped the
    // source, so its [11] chip went dead and it vanished from the list.
    const url = `https://example.com/${'a'.repeat(3000)}`;
    const parsed = SourcesArraySchema.safeParse([{
      documentId: `web:${url}`,
      chunkId: 'web-content-1',
      fileName: 'Windowsill Survival',
      filePath: url,
      mimeType: 'text/html',
      category: 'Web Article',
      content: 'x'.repeat(20_000),
      excerpt: 'y'.repeat(4000),
      highlights: Array.from({ length: 20 }, (_, i) => `term${i}`),
      score: 1,
      fileSizeBytes: 877,
      modifiedAt: '2026-09-23T06:39:10Z',
      citationId: 11,
    }]);
    expect(parsed.success).toBe(true);
  });

  it('accepts an optional saved web snapshot with nullable title and capture time', () => {
    const parsed = SourcesArraySchema.safeParse([{
      documentId: 'web:https://example.com/article',
      chunkId: 'web-content-1',
      fileName: 'Article',
      filePath: 'https://example.com/article',
      mimeType: 'text/html',
      category: 'Web Article',
      content: 'A source excerpt.',
      webSnapshot: {
        url: 'https://example.com/article',
        title: null,
        text: 'The captured page text.',
        fetchedAt: null,
        truncated: true,
      },
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: '',
    }]);

    expect(parsed.success).toBe(true);
  });
});
