import { describe, expect, it } from 'vitest';

import {
  answer,
  answerSeconds,
  fromNow,
  interleaveByTopic,
  keyPoints,
  MAX_ATTEMPTS,
  prepPlan,
  RELEARN_GAP,
  startQueue,
  suggestedRating,
  summarize,
} from '@/features/learning/recall/flashcards/retrieval';
import type { StudyCardDto } from '@/lib/bindings';

function card(id: string, topic = 'Topic', extra: Partial<StudyCardDto> = {}): StudyCardDto {
  return {
    id, format: 'question_answer', deckId: 'deck', question: `Question ${id}`, answer: 'Answer', options: [], correctIndex: 0,
    explanation: '', topic, source: { chunkId: 'c', documentId: 'd', fileName: 'f', filePath: '/f', excerpt: 'e' },
    citations: [], dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0, ...extra,
  };
}

describe('retrieval session', () => {
  it('mixes topics in turn instead of blocking them', () => {
    const cards = [card('a1', 'A'), card('a2', 'A'), card('a3', 'A'), card('b1', 'B'), card('c1', 'C')];
    expect(interleaveByTopic(cards).map(item => item.id)).toEqual(['a1', 'b1', 'c1', 'a2', 'a3']);
  });

  it('brings a missed card back after a few others, with its saved review count, until it is recalled', () => {
    let queue = startQueue(['1', '2', '3', '4', '5'].map(id => card(id)));
    queue = answer(queue, { ...card('1'), reviewCount: 1 }, 'again', 'sure');
    expect(queue.pending.map(item => item.id)).toEqual(['2', '3', '4', '1', '5']);
    expect(queue.pending[RELEARN_GAP].reviewCount).toBe(1);
    for (const id of ['2', '3', '4']) queue = answer(queue, card(id), 'good', 'unsure');
    queue = answer(queue, { ...card('1'), reviewCount: 2 }, 'good', 'unsure');
    queue = answer(queue, card('5'), 'good', null);
    expect(queue.pending).toEqual([]);
    const summary = summarize(queue.attempts);
    expect(summary).toMatchObject({ cards: 5, recalledFirstTry: 4, relearned: 1, stillMissed: [], confidentErrors: ['Question 1'] });
  });

  it('stops relearning a card after the attempt limit and reports it as still shaky', () => {
    let queue = startQueue([card('1')]);
    for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
      expect(queue.pending.map(item => item.id)).toEqual(['1']);
      queue = answer(queue, card('1'), 'again', 'guess');
    }
    expect(queue.pending).toEqual([]);
    expect(summarize(queue.attempts)).toMatchObject({ recalledFirstTry: 0, relearned: 0, stillMissed: ['Question 1'], confidentErrors: [] });
  });

  it('compares first-try confidence with results', () => {
    let queue = startQueue([card('1'), card('2'), card('3')]);
    queue = answer(queue, card('1'), 'good', 'sure');
    queue = answer(queue, card('2'), 'again', 'sure');
    queue = answer(queue, card('3'), 'hard', 'guess');
    queue = answer(queue, card('2'), 'good', 'sure');
    expect(summarize(queue.attempts).calibration).toEqual([
      { confidence: 'sure', right: 1, total: 2 },
      { confidence: 'guess', right: 1, total: 1 },
    ]);
  });

  it('reads key points from a Markdown list and suggests a rating from those covered', () => {
    expect(keyPoints('- Bounded queues\n- Producer signalling\n* Metrics')).toEqual(['Bounded queues', 'Producer signalling', 'Metrics']);
    expect(keyPoints('1. One\n2) Two')).toEqual(['One', 'Two']);
    expect(keyPoints('Chlorophyll')).toEqual([]);
    expect(keyPoints('- Only one')).toEqual([]);
    expect(suggestedRating(4, 4)).toBe('good');
    expect(suggestedRating(2, 4)).toBe('hard');
    expect(suggestedRating(1, 4)).toBe('again');
    expect(suggestedRating(0, 0)).toBeNull();
  });

  it('times interview answers but not short facts', () => {
    expect(answerSeconds(card('1'))).toBe(0);
    expect(answerSeconds(card('1', 'Behavioral', { answer: '- Situation\n- Action\n- Result' }))).toBe(120);
    expect(answerSeconds(card('1', 'System design', { answer: '- Requirements\n- Data model' }))).toBe(300);
  });

  it('says when something is due in words', () => {
    expect(fromNow(1_000, 1_000)).toBe('now');
    expect(fromNow(25 * 60_000, 0)).toBe('in 25 min');
    expect(fromNow(5 * 3_600_000, 0)).toBe('in 5 h');
    expect(fromNow(3 * 86_400_000, 0)).toBe('in 3 days');
  });
});

describe('prep plan', () => {
  it('lays out a first pass, the next due session, a session before sleep and a warm-up', () => {
    const now = new Date(2026, 9, 26, 8, 0).getTime();
    const target = new Date(2026, 9, 27, 10, 0).getTime();
    const plan = prepPlan(now, target, now + 4 * 3_600_000);
    expect(plan.map(step => step.title)).toEqual(['First pass, now', 'Next session, when cards come due', 'Before you sleep', 'Warm-up', 'Target']);
    expect(new Date(plan[2].at).getHours()).toBe(21);
    expect(plan[3].at).toBe(target - 2 * 3_600_000);
    expect(prepPlan(target, target, null)).toEqual([]);
  });

  it('leaves out what no longer fits when the target is close', () => {
    const now = new Date(2026, 9, 27, 8, 30).getTime();
    const target = new Date(2026, 9, 27, 10, 0).getTime();
    expect(prepPlan(now, target, now + 10 * 60_000).map(step => step.title)).toEqual(['First pass, now', 'Target']);
  });

  it('folds cards due near bedtime into the session before sleep', () => {
    const now = new Date(2026, 9, 26, 8, 0).getTime();
    const target = new Date(2026, 9, 27, 10, 0).getTime();
    const nineAtNight = new Date(2026, 9, 26, 21, 0).getTime();
    expect(prepPlan(now, target, nineAtNight).map(step => step.title)).toEqual(['First pass, now', 'Before you sleep', 'Warm-up', 'Target']);
  });
});
