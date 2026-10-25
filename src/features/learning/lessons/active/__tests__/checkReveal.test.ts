import { describe, expect, it } from 'vitest';

import { checkCardContent, checkCardId, clip, splitCheck } from '@/features/learning/lessons/active/checkReveal';

const CASE = 'A sealed jar of air is carried from the beach up a mountain. Predict what happens to its lid, and say why.';
const REVEAL = 'The lid bulges.\n\nOutside pressure falls with altitude while the air inside keeps its pressure. If you said it caves in, you reversed which side lost pressure.';

describe('a check’s reveal', () => {
  it('splits at exactly one “Check your thinking” heading, as lesson_rules.rs does', () => {
    expect(splitCheck(`${CASE}\n\n### Check your thinking\n\n${REVEAL}`)).toEqual({ question: CASE, reveal: REVEAL });
    expect(splitCheck(`${CASE}\n#### check your thinking\n${REVEAL}`)).toEqual({ question: CASE, reveal: REVEAL });
    expect(splitCheck(`${CASE} ${REVEAL}`)).toBeNull();
    expect(splitCheck(`${CASE}\n### Check your thinking\n${REVEAL}\n### Check your thinking\nAgain.`)).toBeNull();
    expect(splitCheck(`### Check your thinking\n${REVEAL}`)).toBeNull();
    expect(splitCheck(`# Check your thinking\n${REVEAL}`)).toBeNull();
  });

  it('becomes a review card within the recall card’s bounds: case, answer, then reasoning', () => {
    expect(checkCardContent({ question: CASE, reveal: REVEAL })).toEqual({
      prompt: CASE,
      answer: 'The lid bulges.',
      explanation: 'Outside pressure falls with altitude while the air inside keeps its pressure. If you said it caves in, you reversed which side lost pressure.',
      options: [], correctOptionIndex: null, language: null, clozeDeletions: [],
    });
    const long = checkCardContent({ question: 'Why? '.repeat(600), reveal: `${'An answer sentence. '.repeat(80)}\n\n${'Reasoning. '.repeat(400)}` });
    expect(long.prompt.length).toBeLessThanOrEqual(2000);
    expect(long.answer.length).toBeLessThanOrEqual(1000);
    expect(long.explanation.length).toBeLessThanOrEqual(3000);
    expect(long.answer.endsWith('.…')).toBe(true);
  });

  it('clips at a sentence end, never mid-word', () => {
    expect(clip('Short.', 10)).toBe('Short.');
    expect(clip('First sentence here. Second sentence that is long.', 30)).toBe('First sentence here.…');
  });

  it('names the same card for the same check, and a new one when the check is rewritten', async () => {
    const id = await checkCardId('lesson-1', 3, 'body');
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(await checkCardId('lesson-1', 3, 'body')).toBe(id);
    expect(await checkCardId('lesson-1', 3, 'rewritten body')).not.toBe(id);
    expect(await checkCardId('lesson-1', 4, 'body')).not.toBe(id);
  });
});
