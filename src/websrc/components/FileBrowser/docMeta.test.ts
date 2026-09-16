import { describe, expect, it } from 'vitest';

import { TYPE_BUCKETS, typeBucket } from './docMeta';

import type { DocumentMetadata } from '../../types/fileBrowser';

const doc = (overrides: Partial<DocumentMetadata>): DocumentMetadata => ({
  id: 'doc-1',
  fileName: 'file.txt',
  filePath: '/vault/file.txt',
  fileType: 'txt',
  category: 'document',
  language: 'en',
  modifiedAt: '2026-01-01',
  indexedAt: '2026-01-01',
  wordCount: 10,
  ...overrides,
});

describe('typeBucket', () => {
  it('maps the known extensions', () => {
    expect(typeBucket(doc({ fileType: 'pdf' }))).toBe('PDF');
    expect(typeBucket(doc({ fileType: 'md' }))).toBe('Markdown');
    expect(typeBucket(doc({ fileType: 'markdown' }))).toBe('Markdown');
    expect(typeBucket(doc({ fileType: 'txt' }))).toBe('Text');
    expect(typeBucket(doc({ fileType: 'rtf' }))).toBe('Text');
    expect(typeBucket(doc({ fileType: 'docx' }))).toBe('Word');
    expect(typeBucket(doc({ fileType: 'csv' }))).toBe('Spreadsheet');
    expect(typeBucket(doc({ fileType: 'html' }))).toBe('HTML');
    for (const ext of ['ts', 'py', 'rs']) {
      expect(typeBucket(doc({ fileType: ext }))).toBe('Code');
    }
  });

  it('uppercases an unknown extension', () => {
    expect(typeBucket(doc({ fileType: 'epub' }))).toBe('EPUB');
  });

  it('calls a missing extension Other', () => {
    expect(typeBucket(doc({ fileType: '' }))).toBe('Other');
  });

  it('calls a web document Web regardless of extension', () => {
    expect(typeBucket(doc({ filePath: 'https://example.com/a.pdf', fileType: 'pdf' }))).toBe('Web');
    expect(
      typeBucket(doc({ filePath: '/vault/.lattice/web-archive/x.html', fileType: 'html' }))
    ).toBe('Web');
    expect(typeBucket(doc({ category: 'web article', fileType: 'pdf' }))).toBe('Web');
  });

  it('only ever produces a declared bucket or an uppercased extension', () => {
    const inputs = [
      'pdf', 'md', 'markdown', 'txt', 'rtf', 'doc', 'docx', 'xls', 'xlsx', 'csv',
      'html', 'htm', 'json', 'js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', '',
    ];
    for (const fileType of inputs) {
      const label = typeBucket(doc({ fileType }));
      const isDeclared = (TYPE_BUCKETS as readonly string[]).includes(label);
      expect(isDeclared || label === fileType.toUpperCase()).toBe(true);
    }
    expect((TYPE_BUCKETS as readonly string[]).includes(typeBucket(doc({ category: 'web' })))).toBe(true);
  });
});
