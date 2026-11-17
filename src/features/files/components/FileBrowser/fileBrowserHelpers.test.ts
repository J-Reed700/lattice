import { describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import {
  canSearchByContent,
  errorMessage,
  isAbsolutePath,
  normalizeSearchToken,
  plural,
} from './fileBrowserHelpers';

const doc = (overrides: Partial<DocumentMetadata>): DocumentMetadata => ({
  id: 'd1',
  fileName: 'notes.md',
  filePath: '/notes/notes.md',
  fileType: 'md',
  category: 'document',
  language: 'en',
  modifiedAt: '2026-01-01',
  indexedAt: '2026-01-01',
  ...overrides,
} as DocumentMetadata);

describe('fileBrowserHelpers', () => {
  it.each([
    ['/Users/ada/notes.md', true],
    ['C:\\herbarium\\ferns.txt', true],
    ['d:/recipes/bread.md', true],
    ['notes/relative.md', false],
    ['https://example.org/a', false],
  ])('treats %s as absolute: %s', (path, expected) => {
    expect(isAbsolutePath(path)).toBe(expected);
  });

  it('normalizes search tokens', () => {
    expect(normalizeSearchToken('  Photosynthesis ')).toBe('photosynthesis');
  });

  describe('canSearchByContent', () => {
    it('reads text files', () => {
      expect(canSearchByContent(doc({ fileType: 'md' }))).toBe(true);
      expect(canSearchByContent(doc({ fileType: undefined as never }))).toBe(true);
    });

    it('skips media, archives and web pages', () => {
      expect(canSearchByContent(doc({ fileType: ' PNG ' }))).toBe(false);
      expect(canSearchByContent(doc({ fileType: 'mp3' }))).toBe(false);
      expect(canSearchByContent(doc({ fileType: 'zip' }))).toBe(false);
      expect(canSearchByContent(doc({ filePath: 'http://example.org/a' }))).toBe(false);
    });
  });

  it('reads an error message from anything thrown', () => {
    expect(errorMessage(new Error('disk full'))).toBe('disk full');
    expect(errorMessage('plain')).toBe('plain');
    expect(errorMessage(42)).toBe('42');
  });

  it('pluralizes with a formatted count', () => {
    expect(plural(1, 'file')).toBe('1 file');
    expect(plural(0, 'file')).toBe('0 files');
    expect(plural(1200, 'passage')).toBe(`${(1200).toLocaleString()} passages`);
  });
});
