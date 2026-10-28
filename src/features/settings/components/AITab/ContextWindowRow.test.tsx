import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { LLMSettings } from '@/types/api/settings';

import { ContextWindowRow } from './ContextWindowRow';

function renderRow(overrides: Partial<LLMSettings> = {}) {
  const saveLlmUpdates = vi.fn().mockResolvedValue(true);
  const llmSettings = { ...makeAppSettings().llm, contextWindow: 32_768, maxTokens: 16_384, ...overrides };
  render(<ContextWindowRow llmSettings={llmSettings} saveLlmUpdates={saveLlmUpdates} />);
  return saveLlmUpdates;
}

const field = () => screen.getByLabelText('Context window');
const edit = (value: string) => {
  fireEvent.change(field(), { target: { value } });
  fireEvent.blur(field());
};

describe('ContextWindowRow', () => {
  it('saves the window within its bounds and keeps the answer length inside it', () => {
    const save = renderRow({ provider: 'llamacpp' });

    edit('8192');
    expect(save).toHaveBeenLastCalledWith({ contextWindow: 8192, maxTokens: 8192 });
    edit('100');
    expect(save).toHaveBeenLastCalledWith({ contextWindow: 512, maxTokens: 512 });
    expect(field()).toHaveValue(512);
  });

  it('saves nothing when the window is left as it was', () => {
    const save = renderRow({ provider: 'ollama' });

    edit('32768');

    expect(save).not.toHaveBeenCalled();
  });

  it('is shown whatever the chat provider, since cloud models in other roles use it', () => {
    renderRow({ provider: 'local' });

    expect(field()).toHaveValue(32_768);
    expect(screen.getByText(/whether they chat, prepare lessons or do background work/)).toBeInTheDocument();
  });
});
