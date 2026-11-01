import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelWindowControls } from './ModelWindowControls';

const claude = { kind: 'cloud', provider: 'anthropic', model: 'claude-haiku-5-5' } as const;
const key = 'cloud:anthropic/claude-haiku-5-5';

const state = vi.hoisted(() => ({
  settings: { llm: { contextWindow: 131072, maxTokens: 128000, modelWindows: {} as Record<string, unknown> } },
  limits: { data: { contextWindow: 1000000 as number | null, maxOutputTokens: 128000 as number | null }, isLoading: false, isError: false },
  update: vi.fn(),
}));

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: state.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: state.update }),
}));

vi.mock('@/features/model/hooks/useModelLimits', () => ({
  useModelLimits: () => state.limits,
}));

describe('ModelWindowControls', () => {
  beforeEach(() => {
    state.update.mockReset().mockResolvedValue(undefined);
    state.settings.llm.modelWindows = {};
    state.limits = { data: { contextWindow: 1000000, maxOutputTokens: 128000 }, isLoading: false, isError: false };
  });

  it('keeps the default window under the provider\'s ceiling until one is chosen', () => {
    render(<ModelWindowControls modelRef={claude} />);
    expect(screen.getByLabelText('Context window')).toHaveValue(131072);
    expect(screen.getByLabelText('Context window Slider')).toHaveAttribute('max', '1000000');
    expect(screen.getByLabelText('Max output')).toHaveValue(128000);
    expect(screen.queryByRole('button', { name: 'Use the default' })).not.toBeInTheDocument();
  });

  it('shows a model smaller than the default at its own limit', () => {
    state.limits = { data: { contextWindow: 64000, maxOutputTokens: 8192 }, isLoading: false, isError: false };
    render(<ModelWindowControls modelRef={claude} />);
    expect(screen.getByLabelText('Context window')).toHaveValue(64000);
  });

  it('saves a choice for this model alone', async () => {
    state.settings.llm.modelWindows = { 'cloud:openai/other': { contextWindow: 5000, maxOutputTokens: null } };
    render(<ModelWindowControls modelRef={claude} />);
    const field = screen.getByLabelText('Context window');
    fireEvent.change(field, { target: { value: '500000' } });
    fireEvent.blur(field);
    await waitFor(() => expect(state.update).toHaveBeenCalledWith({
      category: 'llm',
      updates: {
        modelWindows: {
          'cloud:openai/other': { contextWindow: 5000, maxOutputTokens: null },
          [key]: { contextWindow: 500000, maxOutputTokens: null },
        },
      },
    }));
  });

  it('holds a choice to what the provider reports', async () => {
    state.settings.llm.modelWindows = { [key]: { contextWindow: 64000, maxOutputTokens: null } };
    render(<ModelWindowControls modelRef={claude} />);
    const field = screen.getByLabelText('Context window');
    fireEvent.change(field, { target: { value: '2000000' } });
    fireEvent.blur(field);
    await waitFor(() => expect(state.update).toHaveBeenCalledWith({
      category: 'llm',
      updates: { modelWindows: { [key]: { contextWindow: 1000000, maxOutputTokens: null } } },
    }));
  });

  it('saves a slider when it is let go, not while it moves', async () => {
    render(<ModelWindowControls modelRef={claude} />);
    const slider = screen.getByLabelText('Context window Slider');
    fireEvent.change(slider, { target: { value: '200192' } });
    expect(state.update).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    await waitFor(() => expect(state.update).toHaveBeenCalledWith({
      category: 'llm',
      updates: { modelWindows: { [key]: { contextWindow: 200192, maxOutputTokens: null } } },
    }));
  });

  it('saves a larger window, up to what is reported', async () => {
    render(<ModelWindowControls modelRef={claude} />);
    const field = screen.getByLabelText('Context window');
    fireEvent.change(field, { target: { value: '1000000' } });
    fireEvent.blur(field);
    await waitFor(() => expect(state.update).toHaveBeenCalledWith({
      category: 'llm',
      updates: { modelWindows: { [key]: { contextWindow: 1000000, maxOutputTokens: null } } },
    }));
  });

  it('does not save a value nobody changed', () => {
    render(<ModelWindowControls modelRef={claude} />);
    fireEvent.blur(screen.getByLabelText('Context window'));
    expect(state.update).not.toHaveBeenCalled();
  });

  it('goes back to the default, leaving no entry behind', async () => {
    state.settings.llm.modelWindows = { [key]: { contextWindow: 64000, maxOutputTokens: null } };
    render(<ModelWindowControls modelRef={claude} />);
    expect(screen.getByLabelText('Context window')).toHaveValue(64000);
    fireEvent.click(screen.getByRole('button', { name: 'Use the default' }));
    await waitFor(() => expect(state.update).toHaveBeenCalledWith({ category: 'llm', updates: { modelWindows: {} } }));
  });

  it('leaves a model whose provider reports nothing as a plain number', () => {
    state.limits = { data: { contextWindow: null, maxOutputTokens: null }, isLoading: false, isError: false };
    render(<ModelWindowControls modelRef={{ kind: 'cloud', provider: 'openai', model: 'gpt-5' }} />);
    expect(screen.getByLabelText('Context window')).toHaveValue(131072);
    expect(screen.queryByLabelText('Context window Slider')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('OpenAI does not report');
  });

  it('offers an Ollama model its trained length, and no output ceiling', () => {
    state.limits = { data: { contextWindow: 262144, maxOutputTokens: null }, isLoading: false, isError: false };
    render(<ModelWindowControls modelRef={{ kind: 'ollama', model: 'qwen:27b' }} />);
    expect(screen.getByLabelText('Context window')).toHaveValue(131072);
    expect(screen.getByLabelText('Context window Slider')).toHaveAttribute('max', '262144');
    expect(screen.queryByLabelText('Max output')).not.toBeInTheDocument();
  });

  it('says so when the provider could not be asked', () => {
    state.limits = { data: undefined as never, isLoading: false, isError: true };
    render(<ModelWindowControls modelRef={claude} />);
    expect(screen.getByRole('status')).toHaveTextContent('could not be asked');
  });
});
