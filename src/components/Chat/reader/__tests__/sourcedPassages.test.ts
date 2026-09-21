import { describe, expect, it } from 'vitest';

import {
  findQuoteSpan,
  findSourcedPassages,
  mergePassages,
} from '../sourcedPassages';

const PAGE = [
  'Mapping heat equity, block by block',
  '',
  'The team combined summer Landsat passes with block-group census data to see which streets carry the heat. Readers wrote in asking what the maps leave out.',
  '',
  'Satellite surface temperature overstates the gap residents feel. Air temperature differences between the leafiest and barest blocks were closer to 3 °C than the 9 °C the thermal imagery suggests.',
  '',
  'None of this is a reason to stop planting. It is a reason to be careful about which number goes in the headline.',
].join('\n');

describe('finding the passage an answer sentence matches', () => {
  it('finds a paraphrase that shares names and numbers', () => {
    const [passage] = findSourcedPassages(PAGE, [
      'Thermal imagery overstates what residents feel: air temperature differences were nearer 3 °C than the 9 °C the imagery suggests.',
    ]);

    expect(passage).toBeDefined();
    expect(PAGE.slice(passage!.start, passage!.end)).toContain('closer to 3 °C than the 9 °C');
    expect(passage!.sentenceIndex).toBe(0);
    expect(passage!.score).toBeGreaterThanOrEqual(0.5);
  });

  it('ignores a sentence that shares only stopwords', () => {
    expect(
      findSourcedPassages(PAGE, ['It is one of the things that they have to do with it.'])
    ).toEqual([]);
  });

  it('says nothing rather than pointing at a paragraph it is unsure of', () => {
    // Every content word here is absent from the page bar "temperature".
    expect(
      findSourcedPassages(PAGE, [
        'Night-time minima drive heat mortality in coastal cities, where humidity keeps temperature high after dark.',
      ])
    ).toEqual([]);
  });

  it('gives one highlight when two sentences match the same span', () => {
    const passages = findSourcedPassages(PAGE, [
      'Satellite surface temperature overstates the gap residents feel.',
      'Air temperature differences between the leafiest and barest blocks were closer to 3 °C.',
    ]);

    expect(passages).toHaveLength(1);
    expect(PAGE.slice(passages[0]!.start, passages[0]!.end)).toContain('Satellite surface');
  });

  it('returns offsets that slice the page exactly', () => {
    const [passage] = findSourcedPassages(PAGE, [
      'The team combined summer Landsat passes with block-group census data.',
    ]);

    expect(passage).toBeDefined();
    expect(PAGE.slice(passage!.start, passage!.end)).toBe(
      'The team combined summer Landsat passes with block-group census data to see which streets carry the heat.'
    );
  });

  it('keeps offsets exact through multibyte text', () => {
    const page = 'Résumé du rapport 🌳 sur la canopée.\n\nLa température de l’air baissait de 1,2 °C pour chaque tranche de 10 points de couverture arborée mesurée.';
    const [passage] = findSourcedPassages(page, [
      'La température de l’air baissait de 1,2 °C par tranche de 10 points de couverture arborée.',
    ]);

    expect(passage).toBeDefined();
    expect(page.slice(passage!.start, passage!.end)).toBe(
      'La température de l’air baissait de 1,2 °C pour chaque tranche de 10 points de couverture arborée mesurée.'
    );
  });

  it('returns passages in page order, whatever order the sentences come in', () => {
    const passages = findSourcedPassages(PAGE, [
      'Choosing which number goes in the headline is a reason to be careful, not to stop planting.',
      'Summer Landsat passes were combined with block-group census data to see which streets carry heat.',
    ]);

    expect(passages.length).toBeGreaterThan(1);
    expect(passages[0]!.start).toBeLessThan(passages[1]!.start);
  });

  it('has nothing to say about empty inputs', () => {
    expect(findSourcedPassages('', ['Anything at all about canopy cover.'])).toEqual([]);
    expect(findSourcedPassages(PAGE, [])).toEqual([]);
    expect(findSourcedPassages(PAGE, [''])).toEqual([]);
  });

  it('does not let a citation marker count as a word the page shares', () => {
    // `[4]` tokenizes to the number 4, which weighs as much as any other
    // number; left in, a marker would prop up a sentence that matched nothing.
    expect(findSourcedPassages('Chapter 4 of the report.', ['A different thing entirely [4].'])).toEqual([]);
  });
});

describe('merging passages', () => {
  it('folds overlapping spans together and keeps the stronger match', () => {
    expect(
      mergePassages([
        { start: 10, end: 40, sentenceIndex: 0, score: 0.6 },
        { start: 30, end: 55, sentenceIndex: 1, score: 0.9 },
        { start: 80, end: 90, sentenceIndex: 2, score: 0.7 },
      ])
    ).toEqual([
      { start: 10, end: 55, sentenceIndex: 1, score: 0.9 },
      { start: 80, end: 90, sentenceIndex: 2, score: 0.7 },
    ]);
  });
});

describe('locating a quoted passage', () => {
  it('finds a quote whose whitespace and quote marks were rewritten', () => {
    const span = findQuoteSpan(PAGE, 'Satellite surface   temperature overstates\nthe gap residents feel');

    expect(span).not.toBeNull();
    expect(PAGE.slice(span!.start, span!.end)).toBe(
      'Satellite surface temperature overstates the gap residents feel'
    );
  });

  it('finds nothing when the quote was reworded', () => {
    expect(findQuoteSpan(PAGE, 'Satellite temperature exaggerates the gap that residents feel')).toBeNull();
  });
});
