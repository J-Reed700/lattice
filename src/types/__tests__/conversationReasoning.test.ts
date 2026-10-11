import { describe, expect, it } from 'vitest';

import { mergeStep } from '@/hooks/conversations/messageMetadata';
import { TurnStepSchema } from '@/types/conversation';

const step = {
  id: 'thinking-1', kind: 'generate', label: 'Thinking',
  state: 'running', startedAtMs: 0,
};

describe('reasoning step events', () => {
  it('accepts older steps with no reasoning', () => {
    expect(TurnStepSchema.parse(step).reasoning).toBeUndefined();
  });

  it('matches the backend character limit for non-BMP text', () => {
    expect(TurnStepSchema.safeParse({ ...step, reasoning: '🧠'.repeat(24_000) }).success).toBe(true);
    expect(TurnStepSchema.safeParse({ ...step, reasoning: '🧠'.repeat(24_001) }).success).toBe(false);
    expect(TurnStepSchema.safeParse({ ...step, reasoning: 'a'.repeat(24_001) }).success).toBe(false);
  });

  it('retains streamed reasoning when the running step is replaced and completed', () => {
    const started = TurnStepSchema.parse(step);
    const progress = TurnStepSchema.parse({ ...step, reasoning: 'Comparing sources.' });
    const complete = TurnStepSchema.parse({ ...step, state: 'done', durationMs: 1200, reasoning: 'Comparing sources. The saved passage supports this.' });
    const running = mergeStep([started], progress);
    expect(running).toEqual([progress]);
    expect(mergeStep(running, complete)).toEqual([complete]);
  });
});
