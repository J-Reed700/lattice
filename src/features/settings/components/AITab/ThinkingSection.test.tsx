import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { LLMSettings } from '@/types/api/settings';

import { ThinkingSection } from './ThinkingSection';

const THINKING = { temperature: 1, topP: 0.95, topK: 20, presencePenalty: 1.5, repeatPenalty: 1 };

function renderSection(overrides: Partial<LLMSettings> = {}) {
  const saveLlmUpdates = vi.fn().mockResolvedValue(true);
  const llmSettings = {
    answerThinking: false,
    thinkingOverride: 'auto',
    thinkingSampling: THINKING,
    speculativeDecoding: { mode: 'off' },
    ...overrides,
  } as LLMSettings;
  render(<ThinkingSection llmSettings={llmSettings} saveLlmUpdates={saveLlmUpdates} />);
  return saveLlmUpdates;
}

describe('ThinkingSection', () => {
  it('turns answer thinking on', async () => {
    const save = renderSection();
    await userEvent.setup().click(screen.getByRole('switch', { name: 'Think before answering' }));
    expect(save).toHaveBeenCalledWith({ answerThinking: true });
  });

  it('pins thinking for every request when the user overrides it', () => {
    const save = renderSection();
    fireEvent.change(screen.getByLabelText('Thinking for every request'), {
      target: { value: 'on' },
    });
    expect(save).toHaveBeenCalledWith({ thinkingOverride: 'on' });
  });

  it('keeps the whole thinking profile when one field changes, clamped to its bounds', () => {
    const save = renderSection();
    const temperature = screen.getByLabelText('Thinking temperature');
    fireEvent.change(temperature, { target: { value: '5' } });
    fireEvent.blur(temperature);
    expect(save).toHaveBeenCalledWith({ thinkingSampling: { ...THINKING, temperature: 2 } });
  });

  it('offers MTP drafting as an opt-in with the server default draft length', async () => {
    const save = renderSection();
    expect(screen.getByLabelText('Draft tokens')).toBeDisabled();
    await userEvent.setup().click(screen.getByRole('switch', { name: "Draft with the model's MTP head" }));
    expect(save).toHaveBeenCalledWith({ speculativeDecoding: { mode: 'mtpDraft', nMax: 3 } });
  });

  it('saves a new draft length only while drafting is on', () => {
    const save = renderSection({ speculativeDecoding: { mode: 'mtpDraft', nMax: 3 } });
    const tokens = screen.getByLabelText('Draft tokens');
    fireEvent.change(tokens, { target: { value: '12' } });
    fireEvent.blur(tokens);
    expect(save).toHaveBeenCalledWith({ speculativeDecoding: { mode: 'mtpDraft', nMax: 8 } });
  });
});
