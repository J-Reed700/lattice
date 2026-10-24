import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PromptsTab } from './PromptsTab';

const h = vi.hoisted(() => ({
  llm: { llmSettings: null as null | Record<string, any>, isLoading: false, saveLlmUpdates: vi.fn() },
  toast: { success: vi.fn() },
}));
vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const prompts = {
  systemPrompt: 'You are a careful historian.',
  greetingPromptTemplate: '',
  ragPromptTemplate: 'Use these sources: {context}',
  noContextPromptTemplate: 'Answer from memory.',
};
const verification = { enabled: true, temperature: 0, topP: 1, topK: 1 };

const edit = (label: string, value: string) => {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.blur(screen.getByLabelText(label));
};

describe('PromptsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.llm.isLoading = false;
    h.llm.llmSettings = { prompts, verification };
    h.llm.saveLlmUpdates.mockResolvedValue(true);
  });

  it('waits for settings', () => {
    h.llm.llmSettings = null;
    render(<PromptsTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  it('starts from the saved prompts', () => {
    render(<PromptsTab />);
    expect(screen.getByLabelText('System prompt')).toHaveValue('You are a careful historian.');
    expect(screen.getByLabelText('Greeting')).toHaveValue('');
    expect(screen.getByLabelText('Without documents')).toHaveValue('Answer from memory.');
  });

  it('saves a changed template trimmed, and nothing for an unchanged one', async () => {
    render(<PromptsTab />);
    await act(async () => { edit('With documents', '  Cite every source: {context}  '); });
    expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ prompts: { ...prompts, ragPromptTemplate: 'Cite every source: {context}' } });
    await act(async () => { fireEvent.blur(screen.getByLabelText('System prompt')); });
    expect(h.llm.saveLlmUpdates).toHaveBeenCalledTimes(1);
  });

  describe('verification', () => {
    it('turns verification off and says so', async () => {
      render(<PromptsTab />);
      await act(async () => { fireEvent.click(screen.getByRole('switch', { name: 'Verify responses' })); });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ verification: { ...verification, enabled: false } });
      expect(h.toast.success).toHaveBeenCalledWith('Response verification disabled');
    });

    it('turns it back on from defaults when nothing was saved', async () => {
      h.llm.llmSettings = { prompts, verification: { ...verification, enabled: false } };
      const first = render(<PromptsTab />);
      expect(screen.getByLabelText('Temperature')).toBeDisabled();
      await act(async () => { fireEvent.click(screen.getByRole('switch', { name: 'Verify responses' })); });
      expect(h.toast.success).toHaveBeenCalledWith('Response verification enabled');
      first.unmount();
      h.llm.llmSettings = { prompts, verification: undefined };
      render(<PromptsTab />);
      expect(screen.getByRole('switch', { name: 'Verify responses' })).toBeChecked();
      expect(screen.getByLabelText('Top K')).toHaveValue(1);
    });

    it('stays quiet when the toggle is refused', async () => {
      h.llm.saveLlmUpdates.mockResolvedValueOnce(false);
      render(<PromptsTab />);
      await act(async () => { fireEvent.click(screen.getByRole('switch', { name: 'Verify responses' })); });
      expect(h.toast.success).not.toHaveBeenCalled();
    });

    it('saves the judge’s sampling, rounded and clamped', async () => {
      render(<PromptsTab />);
      await act(async () => { edit('Top K', '12.7'); });
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ verification: { ...verification, topK: 13 } });
      expect(h.toast.success).toHaveBeenCalledWith('Top K saved');
      await act(async () => { edit('Top P', '0.456'); });
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ verification: { ...verification, topP: 0.46 } });
    });

    it('saves nothing for an unchanged value, and from defaults when none were saved', async () => {
      const first = render(<PromptsTab />);
      await act(async () => { fireEvent.blur(screen.getByLabelText('Temperature')); });
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
      first.unmount();
      h.llm.llmSettings = { prompts, verification: undefined };
      h.llm.saveLlmUpdates.mockResolvedValueOnce(false);
      render(<PromptsTab />);
      await act(async () => { edit('Temperature', '0.3'); });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ verification: { ...verification, temperature: 0.3 } });
      expect(h.toast.success).not.toHaveBeenCalled();
    });
  });
});
