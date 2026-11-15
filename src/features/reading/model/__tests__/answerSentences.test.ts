import { describe, expect, it } from 'vitest';

import {
  sentencesByOccurrence,
  sentencesCiting,
  splitAnswerSentences,
} from '@/features/reading/model/answerSentences';
import { reply } from '@/tests/replyFixtures';

const PLAIN = [
  '**Short version:** about 1.2 °C of daytime cooling per 10 points of canopy.',
  '',
  '### What the sources agree on',
  '',
  '- The meta-analysis pools 62 studies and puts the effect at 1.2 °C. It measures at pedestrian height.',
  '- Your July transect fits inside that range.',
].join('\n');

const ANSWER = reply(
  '**Short version:** about 1.2 °C of daytime cooling per 10 points of canopy ', 1, 2, '.\n\n',
  '### What the sources agree on\n\n',
  '- The meta-analysis pools 62 studies and puts the effect at 1.2 °C ', 1, '. It measures at pedestrian height.\n',
  '- Your July transect fits inside that range ', 3, '.\n\n',
  'The heat-equity piece says thermal imagery overstates what residents feel ', 4, 1,
  ', so quote the air figure.',
);

describe('splitting an answer into sentences', () => {
  it('takes each markdown block on its own and drops the markup', () => {
    const sentences = splitAnswerSentences(PLAIN);

    expect(sentences).toContain('What the sources agree on');
    expect(sentences).toContain('Your July transect fits inside that range.');
    expect(sentences.some((sentence) => sentence.includes('**'))).toBe(false);
    expect(sentences.some((sentence) => sentence.startsWith('- '))).toBe(false);
  });

  it('splits a bullet carrying two sentences into two', () => {
    const sentences = splitAnswerSentences(PLAIN);

    expect(sentences).toContain('The meta-analysis pools 62 studies and puts the effect at 1.2 °C.');
    expect(sentences).toContain('It measures at pedestrian height.');
  });
});

describe('picking the sentences that cite a source', () => {
  it('keeps every sentence citing that number, alone or among others', () => {
    expect(sentencesCiting(ANSWER, 1)).toEqual([
      'Short version: about 1.2 °C of daytime cooling per 10 points of canopy.',
      'The meta-analysis pools 62 studies and puts the effect at 1.2 °C.',
      'The heat-equity piece says thermal imagery overstates what residents feel, so quote the air figure.',
    ]);
  });

  it('leaves the references out of what gets matched against the page', () => {
    expect(sentencesCiting(ANSWER, 3)).toEqual(['Your July transect fits inside that range.']);
  });

  it('returns nothing for a number the answer never cites', () => {
    expect(sentencesCiting(ANSWER, 9)).toEqual([]);
    expect(sentencesCiting([], 1)).toEqual([]);
  });

  it('counts bracketed text that is not a reference as plain text', () => {
    const answer = reply('Step [3] is ordinary text, and the claim ', 5, '.');

    expect(sentencesCiting(answer, 3)).toEqual([]);
    expect(sentencesCiting(answer, 5)).toEqual(['Step [3] is ordinary text, and the claim.']);
  });
});

describe('which sentence a clicked mark sits in', () => {
  const REPEATED = reply(
    'Potatoes give the most calories per square foot indoors ', 6, '.\n\n',
    '- Microgreens are ready in ten days ', 2, 6, '. They need no deep soil.\n\n',
    '```\nyield[6] = 4\n```\n\n',
    'Skip `grid[6]` lookups; dwarf tomatoes want eight hours of light ', 6, 2, ' and a five-gallon pot ', 6, '.',
  );

  it('answers each mark of a source with its own sentence, in reading order', () => {
    expect(sentencesByOccurrence(REPEATED, 6)).toEqual([
      'Potatoes give the most calories per square foot indoors.',
      'Microgreens are ready in ten days.',
      'Skip grid[6] lookups; dwarf tomatoes want eight hours of light and a five-gallon pot.',
      'Skip grid[6] lookups; dwarf tomatoes want eight hours of light and a five-gallon pot.',
    ]);
  });

  it('counts only references: not text in code, and every reference of a group', () => {
    expect(sentencesByOccurrence(REPEATED, 6)).toHaveLength(4);
    expect(sentencesByOccurrence(REPEATED, 2)).toEqual([
      'Microgreens are ready in ten days.',
      'Skip grid[6] lookups; dwarf tomatoes want eight hours of light and a five-gallon pot.',
    ]);
  });

  it('repeats a sentence that carries the same mark twice, so the count stays in step', () => {
    const answer = reply('Kale ', 3, ' outgrows chard ', 3, '. Both bolt in heat ', 3, '.');

    expect(sentencesByOccurrence(answer, 3)).toEqual([
      'Kale outgrows chard.',
      'Kale outgrows chard.',
      'Both bolt in heat.',
    ]);
  });
});
