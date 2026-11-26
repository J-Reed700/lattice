import { describe, expect, it } from 'vitest';

import type { AnchorDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';

import { anchoredCardDetails, anchoredEntry, anchoredText, isAnchored } from '../source';

const TEXT = ['# Guide', 'Intro.', '## Methods', 'First measured passage.', '## Results', 'Second measured passage.'].join('\n\n');

function anchor(exact: string): AnchorDto {
  const start = TEXT.indexOf(exact);
  const bytes = (index: number) => new TextEncoder().encode(TEXT.slice(0, index)).length;
  return { exact, span: { start: bytes(start), end: bytes(start + exact.length) } };
}

const baseSource: SourceWithMetadata = {
  documentId: 'course:v1',
  chunkId: 'v1#0',
  fileName: 'Guide',
  filePath: '',
  mimeType: 'text/markdown',
  category: 'Saved source',
  content: 'First measured passage.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 1,
  webSnapshot: { url: '', title: 'Guide', text: TEXT, fetchedAt: null, truncated: false },
  anchoredBy: [
    { occurrence: 0, relation: 'backs', anchors: [] },
    { occurrence: 1, relation: 'quotes', anchors: [anchor('First measured passage.')] },
    { occurrence: 2, relation: 'cites', anchors: [anchor('Second measured passage.')] },
  ],
};

describe('anchoredCardDetails', () => {
  it('picks the first anchored entry with anchors when occurrence is null', () => {
    const details = anchoredCardDetails(baseSource, null);

    expect(details.entry?.occurrence).toBe(1);
    expect(details.relation).toBe('quotes');
    expect(details.excerpts).toEqual(['First measured passage.']);
    expect(details.trail).toEqual(['Guide', 'Methods']);
  });

  it('picks the requested 0-based occurrence and computes its heading trail', () => {
    expect(anchoredEntry(baseSource, null)).toBeNull();
    expect(isAnchored(baseSource)).toBe(true);
    expect(anchoredText(baseSource)).toBe(TEXT);

    const zero = anchoredCardDetails(baseSource, 0);
    expect(zero).toEqual({ entry: { occurrence: 0, relation: 'backs', anchors: [] }, relation: 'backs', excerpts: [], trail: [] });

    const second = anchoredCardDetails(baseSource, 2);
    expect(second.relation).toBe('cites');
    expect(second.excerpts).toEqual(['Second measured passage.']);
    expect(second.trail).toEqual(['Guide', 'Results']);
  });

  it('returns empty trail when text is absent and empty details when unanchored', () => {
    const noText: SourceWithMetadata = {
      ...baseSource,
      webSnapshot: undefined,
      excerpt: undefined,
      content: '',
      anchoredBy: [{ occurrence: 0, relation: 'quotes', anchors: [{ exact: 'Missing text', span: { start: 10, end: 22 } }] }],
    };
    expect(anchoredCardDetails(noText, 0)).toEqual({
      entry: { occurrence: 0, relation: 'quotes', anchors: [{ exact: 'Missing text', span: { start: 10, end: 22 } }] },
      relation: 'quotes',
      excerpts: ['Missing text'],
      trail: [],
    });
    expect(anchoredCardDetails({ ...baseSource, anchoredBy: undefined }, null)).toEqual({
      entry: null,
      relation: null,
      excerpts: [],
      trail: [],
    });
  });
});
