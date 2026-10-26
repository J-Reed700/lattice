import { describe, expect, it } from 'vitest';

import { answerWarmUp, startWarmUp } from '@/features/learning/lessons/active/LessonWarmUp';
import { MAX_ATTEMPTS } from '@/features/learning/recall/flashcards/retrieval';

describe('the warm-up before a lesson', () => {
  it('brings a missed card back after the others until it is recalled', () => {
    let warmUp = startWarmUp(['a', 'b', 'c']);
    warmUp = answerWarmUp(warmUp, 'again');
    expect(warmUp.pending).toEqual(['b', 'c', 'a']);
    warmUp = answerWarmUp(warmUp, 'good');
    warmUp = answerWarmUp(warmUp, 'good');
    expect(warmUp.pending).toEqual(['a']);
    expect(warmUp.tries.a).toBe(1);
    warmUp = answerWarmUp(warmUp, 'good');
    expect(warmUp.pending).toEqual([]);
    expect(warmUp.tries).toEqual({ a: 2, b: 1, c: 1 });
  });

  it('moves on from a card missed on every attempt, leaving it for the next review', () => {
    let warmUp = startWarmUp(['a']);
    for (let attempt = 1; attempt < MAX_ATTEMPTS; attempt += 1) {
      warmUp = answerWarmUp(warmUp, 'again');
      expect(warmUp.pending).toEqual(['a']);
    }
    warmUp = answerWarmUp(warmUp, 'again');
    expect(warmUp.pending).toEqual([]);
    expect(warmUp.tries.a).toBe(MAX_ATTEMPTS);
  });

  it('counts a hard recall as recalled, as the study session does', () => {
    expect(answerWarmUp(startWarmUp(['a', 'b']), 'hard').pending).toEqual(['b']);
  });
});
