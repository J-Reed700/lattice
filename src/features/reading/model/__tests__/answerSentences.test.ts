import { describe, expect, it } from 'vitest';

import {
  citationsIn,
  sentencesByOccurrence,
  sentencesCiting,
  splitAnswerSentences,
} from '@/features/reading/model/answerSentences';

const ANSWER = [
  '**Short version:** about 1.2 °C of daytime cooling per 10 points of canopy [1][2].',
  '',
  '### What the sources agree on',
  '',
  '- The meta-analysis pools 62 studies and puts the effect at 1.2 °C [1]. It measures at pedestrian height.',
  '- Your July transect fits inside that range [3].',
  '',
  'The heat-equity piece says thermal imagery overstates what residents feel [4, 1], so quote the air figure.',
].join('\n');

describe('splitting an answer into sentences', () => {
  it('takes each markdown block on its own and drops the markup', () => {
    const sentences = splitAnswerSentences(ANSWER);

    expect(sentences).toContain('What the sources agree on');
    expect(sentences).toContain('Your July transect fits inside that range [3].');
    expect(sentences.some((sentence) => sentence.includes('**'))).toBe(false);
    expect(sentences.some((sentence) => sentence.startsWith('- '))).toBe(false);
  });

  it('splits a bullet carrying two sentences into two', () => {
    const sentences = splitAnswerSentences(ANSWER);

    expect(sentences).toContain('The meta-analysis pools 62 studies and puts the effect at 1.2 °C [1].');
    expect(sentences).toContain('It measures at pedestrian height.');
  });
});

describe('reading citation markers', () => {
  it('reads both the forms answers write', () => {
    expect(citationsIn('Adjacent markers [1][3].')).toEqual([1, 3]);
    expect(citationsIn('A list inside one marker [4, 1].')).toEqual([4, 1]);
    expect(citationsIn('No markers here.')).toEqual([]);
  });
});

describe('picking the sentences that cite a source', () => {
  it('keeps every sentence naming that number, in whichever form', () => {
    expect(sentencesCiting(ANSWER, 1)).toEqual([
      'Short version: about 1.2 °C of daytime cooling per 10 points of canopy.',
      'The meta-analysis pools 62 studies and puts the effect at 1.2 °C.',
      'The heat-equity piece says thermal imagery overstates what residents feel, so quote the air figure.',
    ]);
  });

  it('leaves the markers out of what gets matched against the page', () => {
    expect(sentencesCiting(ANSWER, 3)).toEqual(['Your July transect fits inside that range.']);
  });

  it('returns nothing for a number the answer never cites', () => {
    expect(sentencesCiting(ANSWER, 9)).toEqual([]);
    expect(sentencesCiting('', 1)).toEqual([]);
  });
});

describe('which sentence a clicked mark sits in', () => {
  const REPEATED = [
    'Potatoes give the most calories per square foot indoors [6].',
    '',
    '- Microgreens are ready in ten days [2][6]. They need no deep soil.',
    '',
    '```',
    'yield[6] = 4',
    '```',
    '',
    'Skip `grid[6]` lookups; dwarf tomatoes want eight hours of light [6, 2] and a five-gallon pot [6].',
  ].join('\n');

  it('answers each mark of a source with its own sentence, in reading order', () => {
    expect(sentencesByOccurrence(REPEATED, 6)).toEqual([
      'Potatoes give the most calories per square foot indoors.',
      'Microgreens are ready in ten days.',
      'Skip lookups; dwarf tomatoes want eight hours of light and a five-gallon pot.',
    ]);
  });

  it('counts only what is drawn as a chip: not code, and not the comma form', () => {
    // `yield[6]` is in a fence, `grid[6]` is inline code and `[6, 2]` is never a chip.
    expect(sentencesByOccurrence(REPEATED, 6)).toHaveLength(3);
    expect(sentencesByOccurrence(REPEATED, 2)).toEqual(['Microgreens are ready in ten days.']);
  });

  it('repeats a sentence that carries the same mark twice, so the count stays in step', () => {
    expect(sentencesByOccurrence('Kale [3] outgrows chard [3]. Both bolt in heat [3].', 3)).toEqual([
      'Kale outgrows chard.',
      'Kale outgrows chard.',
      'Both bolt in heat.',
    ]);
  });
});
