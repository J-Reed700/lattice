import { useState } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { LLMSettings } from '@/types/api/settings';

import { RuntimeTuningSection, type RuntimeDraft } from './RuntimeTuningSection';

const DRAFT: RuntimeDraft = {
  temperature: 0.7,
  topP: 0.8,
  topK: 20,
  repeatPenalty: 1,
  presencePenalty: 1.5,
  maxTokens: 8192,
  contextWindow: 131072,
  localContextWindow: null,
  timeoutSeconds: 30,
};

function renderSection(localContextWindow: number | null = null) {
  const saveLlmUpdates = vi.fn().mockResolvedValue(true);
  function Harness() {
    const [runtimeDraft, setRuntimeDraft] = useState<RuntimeDraft>({ ...DRAFT, localContextWindow });
    return (
      <RuntimeTuningSection
        llmSettings={{ localContextWindow } as LLMSettings}
        runtimeDraft={runtimeDraft}
        setRuntimeDraft={setRuntimeDraft}
        saveLlmUpdates={saveLlmUpdates}
      />
    );
  }
  render(<Harness />);
  return saveLlmUpdates;
}

const modelMax = () => screen.getByRole('checkbox', { name: "Model's max" });
const windowField = () => screen.getByLabelText('Local model context window');

describe('RuntimeTuningSection local model context window', () => {
  it('unticking "Model\'s max" saves nothing and leaves the field empty for the user', async () => {
    const save = renderSection();

    await userEvent.setup().click(modelMax());

    expect(save).not.toHaveBeenCalled();
    expect(modelMax()).not.toBeChecked();
    expect(windowField()).toBeEnabled();
    expect(windowField()).toHaveValue(null);
  });

  it('saves the window the user types', async () => {
    const save = renderSection();

    await userEvent.setup().click(modelMax());
    fireEvent.change(windowField(), { target: { value: '60416' } });
    fireEvent.blur(windowField());

    expect(save).toHaveBeenCalledWith({ localContextWindow: 60_416 });
    expect(modelMax()).not.toBeChecked();
  });

  it('leaving the field empty keeps every model at its maximum', async () => {
    const save = renderSection();

    await userEvent.setup().click(modelMax());
    fireEvent.blur(windowField());

    expect(save).not.toHaveBeenCalled();
    expect(modelMax()).toBeChecked();
    expect(windowField()).toBeDisabled();
  });

  it('clearing a saved window goes back to each model\'s maximum', () => {
    const save = renderSection(32_768);

    fireEvent.change(windowField(), { target: { value: '' } });
    fireEvent.blur(windowField());

    expect(save).toHaveBeenCalledWith({ localContextWindow: null });
    expect(modelMax()).toBeChecked();
  });
});
