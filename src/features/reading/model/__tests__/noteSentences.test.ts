import { describe, expect, it } from 'vitest';

import { findSourcedPassages } from '@/features/reading/model/sourcedPassages';
import { findNoteMarks } from '@/utils/noteCitations';

import { noteSentencesByOccurrence, noteSentencesCiting } from '../noteSentences';

const NOTE = [
  'Kale outgrows chard in cool weather [1][2].',
  '',
  '- Chard bolts in heat [2]. Both want shade.',
  '',
  'Skip `grid[2]` lookups; beets want a deep pot [2].',
].join('\n');

describe('the notes citation marker', () => {
  it('finds each [n] with its number and place', () => {
    const marks = findNoteMarks('Kale [1] and chard [22].');

    expect(marks.map((mark) => mark.number)).toEqual([1, 22]);
    expect(marks.map((mark) => mark.index)).toEqual([5, 20]);
    expect(findNoteMarks('No markers [x] or [] here.')).toEqual([]);
  });

  it('gives the sentences that cite a source with the markers taken out', () => {
    expect(noteSentencesCiting(NOTE, 1)).toEqual(['Kale outgrows chard in cool weather.']);
    expect(noteSentencesCiting(NOTE, 2)).toEqual([
      'Kale outgrows chard in cool weather.',
      'Chard bolts in heat.',
      'Beets want a deep pot.',
    ]);
    expect(noteSentencesCiting(NOTE, 9)).toEqual([]);
  });

  it('answers each marker of a source with its own sentence, in reading order', () => {
    expect(noteSentencesByOccurrence(NOTE, 2)).toEqual([
      'Kale outgrows chard in cool weather.',
      'Chard bolts in heat.',
      'Beets want a deep pot.',
    ]);
  });

  it('does not let a marker count as a word when a sentence is matched against a page', () => {
    const [sentence] = noteSentencesCiting('Kale outgrows chard in cool weather [12].', 12);
    const page = 'Kale outgrows chard in cool weather.\n\nThe item numbered 12 is unrelated pantry stock.';

    const passages = findSourcedPassages(page, [sentence!]);

    expect(passages).toHaveLength(1);
    expect(page.slice(passages[0]!.start, passages[0]!.end)).toBe('Kale outgrows chard in cool weather.');
  });
});
