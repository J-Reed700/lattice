import { describe, expect, it } from 'vitest';

import { fromLearningSourceVersion } from '@/features/reading/model/sourceAdapter';
import type { LearningSourceVersionSummaryDto } from '@/lib/bindings';

function version(overrides: Partial<LearningSourceVersionSummaryDto> = {}): LearningSourceVersionSummaryDto {
  return {
    id: 'version-1',
    versionNumber: 1,
    title: 'Tides explained',
    publisher: null,
    resolvedUrl: 'https://example.com/tides',
    excerpt: 'Short excerpt',
    contentSha256: 'abc',
    wordCount: 10,
    truncated: false,
    extractionVersion: '1',
    acquiredAt: Date.parse('2026-09-20T00:00:00.000Z'),
    snapshotHash: null,
    ...overrides,
  };
}

const detail = (v: LearningSourceVersionSummaryDto) => ({ sourceId: 'source-1', fullText: '  The saved capture.  ', version: v });

describe('fromLearningSourceVersion', () => {
  it('reads a web version as course material, with its address kept only to name the site', () => {
    const { source, locator } = fromLearningSourceVersion(detail(version()), {
      quote: ' Tides follow the moon. ',
      number: 3,
      readerText: 'The text as it should be read.',
    });

    expect(source.documentId).toBe('course:version-1');
    expect(source.filePath).toBe('');
    expect(source.mimeType).toBe('text/html');
    expect(source.category).toBe('web');
    expect(source.content).toBe('Tides follow the moon.');
    expect(source.excerpt).toBe('Tides follow the moon.');
    expect(source.webSnapshot?.text).toBe('The text as it should be read.');
    expect(source.webSnapshot?.url).toBe('https://example.com/tides');
    expect(source.citationId).toBe(3);
    expect(locator).toEqual({ text: 'Tides follow the moon.', chunkId: source.chunkId });
  });

  it('reads pasted text as course material with no address', () => {
    const { source, locator } = fromLearningSourceVersion(detail(version({ resolvedUrl: null, title: '' })), { number: 2 });

    expect(source.documentId).toBe('course:version-1');
    expect(source.filePath).toBe('');
    expect(source.mimeType).toBe('text/html');
    expect(source.category).toBe('saved text');
    expect(source.content).toBe('');
    expect(source.excerpt).toBeUndefined();
    expect(source.webSnapshot?.text).toBe('  The saved capture.  ');
    expect(source.webSnapshot?.url).toBe('');
    expect(source.citationId).toBe(2);
    expect(locator).toBeNull();
  });

  it('gives two quotes from one version two chunk ids', () => {
    const first = fromLearningSourceVersion(detail(version()), { quote: 'First words' });
    const second = fromLearningSourceVersion(detail(version()), { quote: 'Other words' });
    const whole = fromLearningSourceVersion(detail(version()));

    expect(first.source.chunkId).not.toBe(second.source.chunkId);
    expect(first.locator?.chunkId).toBe(first.source.chunkId);
    expect(whole.source.chunkId).toBe('version-1#0');
  });

  it('keeps the reader text exactly as given, so a byte span of the saved version lands on the same words', () => {
    const { source } = fromLearningSourceVersion(detail(version()), { readerText: '\n\n  Tides follow the moon.\n' });

    expect(source.webSnapshot?.text).toBe('\n\n  Tides follow the moon.\n');
  });
});
