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

describe('an answer sentence that condenses a whole section', () => {
  // The shape of a real page: a numbered section whose facts sit five
  // sentences apart, and an answer that put them all in one bullet.
  const SECTIONS = [
    '5. Radishes',
    'Radishes have shallow roots and are one of the easiest crops for beginners.',
    'Sow seeds every 2 weeks for a continuous harvest of roots and leaves.',
    'Seed to harvest: 24 to 30 days',
    '6. Beets',
    'Beets can be grown indoors for their roots as well as their leafy greens.',
    'The greens look and taste very similar to swiss chard.',
    'A beet seed actually comprises a cluster of tiny seeds.',
    'When they germinate, they need to be thinned 3 to 6 inches apart.',
    'Treat the thinnings as microgreens and put them in salads.',
    'Seed to harvest: 40 days for baby beets, 50 to 65 days to maturity',
    '7. Carrots',
    'Round carrots like Tonda di Parigi are perfect for pots.',
  ].join('\n\n');
  const BEETS =
    'Beets — double yield: roots plus chard-like greens you can eat; thinned seedlings work as microgreens; baby beets in about 40 days.';

  it('finds the section when no three sentences of it carry enough', () => {
    const [passage, ...rest] = findSourcedPassages(SECTIONS, [BEETS]);
    expect(rest).toEqual([]);
    const text = SECTIONS.slice(passage!.start, passage!.end);
    expect(text).toContain('swiss chard');
    expect(text).toContain('40 days for baby beets');
    expect(text).not.toContain('Radishes');
    expect(text).not.toContain('Tonda di Parigi');
  });

  it('still prefers the tight window when one is enough', () => {
    const [passage] = findSourcedPassages(SECTIONS, [
      'Radishes — sow every 2 weeks for a continuous harvest; 24 to 30 days.',
    ]);
    const text = SECTIONS.slice(passage!.start, passage!.end);
    expect(text).toContain('Sow seeds every 2 weeks');
    expect(text).toContain('24 to 30 days');
    expect(text).not.toContain('Beets');
  });

  it('holds the wider window to the same bar', () => {
    expect(
      findSourcedPassages(SECTIONS, [
        'Salad greens, radishes, beets, carrots, scallions, garlic greens, bush beans, peas, potatoes, chives, mint and micro tomatoes.',
      ])
    ).toEqual([]);
  });
});

describe('an answer sentence that credits two sources', () => {
  // Each page backs one clause of it, and neither holds half of the whole.
  const CHIVES =
    '- **Chives & mint** — tolerate lower light and irregular watering better than almost any other herb; chives from seed or transplant in a spot with 6–8 hours of bright light, growing within ~2 weeks [1][9].';

  const SECOND_PAGE = [
    'Basil grows in any south- or east-facing window.',
    'To grow chives from seed, fill a small container with pre-moistened potting mix.',
    'Locate in an area with six to eight hours of bright light.',
    'Chives should start growing within two weeks.',
    'Parsley is slower and wants a deeper pot than most people give it.',
  ].join('\n\n');

  it('finds the clause the second page backs', () => {
    const [passage] = findSourcedPassages(SECOND_PAGE, [CHIVES]);
    const text = SECOND_PAGE.slice(passage!.start, passage!.end);

    expect(text).toContain('six to eight hours of bright light');
    expect(text).not.toContain('Parsley');
  });

  it('reads a spelled-out number as the number', () => {
    const page = 'Herbs are forgiving.\n\nGive seedlings six to eight hours of bright light for two weeks.';
    const sentence = 'Seedlings want 6–8 hours of bright light for 2 weeks.';

    expect(findSourcedPassages(page, [sentence])).toHaveLength(1);
  });

  it('holds a clause to the same bar as a sentence', () => {
    const page = 'Chives are a herb.\n\nMint spreads quickly; keep it in a pot of its own.';

    expect(findSourcedPassages(page, [CHIVES])).toEqual([]);
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
