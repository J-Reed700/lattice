import { describe, expect, it } from 'vitest';

import type { CorpusShapeDto } from '@/types';

import { composeIngestSummary } from './ingestSummary';


const shape = (total: number, byType: Array<[string, number]>): CorpusShapeDto => ({
  total,
  byType: byType.map(([type, count]) => ({ type, count })),
  grownLast7Days: 0,
});

describe('composeIngestSummary', () => {
  it('counts what landed', () => {
    expect(composeIngestSummary('file', 1, null).title).toBe('Added 1 file');
    expect(composeIngestSummary('file', 47, null).title).toBe('Added 47 files');
    expect(composeIngestSummary('URL', 3, null).title).toBe('Added 3 URLs');
    expect(composeIngestSummary('item', 2, null).title).toBe('Added 2 items');
  });

  it('says nothing about shape when the shape is unknown', () => {
    expect(composeIngestSummary('file', 47, null).message).toBeUndefined();
  });

  it('says nothing about shape on a tiny library', () => {
    expect(composeIngestSummary('file', 2, shape(9, [['PDF', 9]])).message).toBeUndefined();
  });

  it('stays quiet just under the threshold', () => {
    expect(
      composeIngestSummary('file', 5, shape(100, [['PDF', 59], ['Markdown', 41]])).message
    ).toBeUndefined();
  });

  it('speaks at exactly the threshold', () => {
    expect(
      composeIngestSummary('file', 5, shape(100, [['PDF', 60], ['Markdown', 40]])).message
    ).toBe('Your library is mostly PDFs.');
  });

  it('picks the top bucket, not the first', () => {
    expect(
      composeIngestSummary('file', 5, shape(100, [['Markdown', 20], ['PDF', 80]])).message
    ).toBe('Your library is mostly PDFs.');
  });

  it('says each bucket in English rather than appending an "s"', () => {
    const phraseFor = (label: string) =>
      composeIngestSummary('file', 5, shape(100, [[label, 80]])).message;
    expect(phraseFor('Markdown')).toBe('Your library is mostly Markdown notes.');
    expect(phraseFor('Text')).toBe('Your library is mostly text files.');
    expect(phraseFor('Code')).toBe('Your library is mostly code.');
    expect(phraseFor('Web')).toBe('Your library is mostly web pages.');
    expect(phraseFor('Word')).toBe('Your library is mostly Word documents.');
    expect(phraseFor('Spreadsheet')).toBe('Your library is mostly spreadsheets.');
    expect(phraseFor('HTML')).toBe('Your library is mostly HTML files.');
    // An unknown bucket is an uppercased extension, which pluralises cleanly.
    expect(phraseFor('EPUB')).toBe('Your library is mostly EPUBs.');
  });

  it('stays quiet when the dominant bucket is "Other"', () => {
    // "Your library is mostly Others." characterises nothing.
    expect(composeIngestSummary('file', 5, shape(100, [['Other', 80]])).message).toBeUndefined();
  });
});
