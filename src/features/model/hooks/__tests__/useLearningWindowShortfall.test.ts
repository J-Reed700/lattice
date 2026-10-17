import { describe, expect, it } from 'vitest';

import type { LearningWindowShortfallDto } from '@/lib/bindings';

import { learningWindowShortfallText } from '../useLearningWindowShortfall';

const ornith = { kind: 'local', model_id: 'ornith-1.5-9b' } as const;
const qwen = { kind: 'local', model_id: 'qwen3.5-27b' } as const;

function shortfall(windowIsSaved: boolean, windowTokens = 8_704): LearningWindowShortfallDto {
  return { model: ornith, needed_tokens: 20_000, window_tokens: windowTokens, window_is_saved: windowIsSaved };
}

describe('learningWindowShortfallText', () => {
  it('names both windows while the saved window is still too small', () => {
    expect(learningWindowShortfallText(shortfall(true), ornith, 8_704)).toBe(
      'Learning work needs a context window of 20,000 tokens; this model has 8,704.'
    );
    expect(learningWindowShortfallText(shortfall(true), ornith, 16_384)).toBe(
      'Learning work needs a context window of 20,000 tokens; this model has 16,384.'
    );
  });

  it('says nothing once the saved window covers what the work needed', () => {
    expect(learningWindowShortfallText(shortfall(true), ornith, 20_000)).toBeNull();
  });

  it('says nothing when a saved window was moved to Auto, until work runs again', () => {
    expect(learningWindowShortfallText(shortfall(true), ornith, null)).toBeNull();
  });

  it("uses the window Auto or the server gave when none is saved", () => {
    expect(learningWindowShortfallText(shortfall(false, 12_288), ornith, null)).toBe(
      'Learning work needs a context window of 20,000 tokens; this model has 12,288.'
    );
  });

  it('belongs only to the model the refusal named', () => {
    expect(learningWindowShortfallText(shortfall(true), qwen, 8_704)).toBeNull();
    expect(learningWindowShortfallText(null, ornith, 8_704)).toBeNull();
  });
});
