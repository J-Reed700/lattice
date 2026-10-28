import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TuningTab } from './TuningTab';

const h = vi.hoisted(() => ({
  llm: { llmSettings: null as null | Record<string, any>, isLoading: false, saveLlmUpdates: vi.fn() },
}));

vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('./ThinkingSection', () => ({ ThinkingSection: () => <section aria-label="Thinking" /> }));

const router = {
  enabled: true,
  preferLastDocument: false,
  model: 'qwen-0.5b',
  timeoutMs: 400,
  maxTokens: 64,
  temperature: 0.1,
  ambiguityThreshold: 0.5,
  promptTemplate: 'Route: {question}',
  clarifyPromptTemplate: 'Ask which: {options}',
};

const toolOutput = {
  excerptChars: 600,
  highlightTermsMax: 8,
  templates: {
    defaultTemplate: '{title}',
    getDocumentTemplate: '{body}',
    semanticSearchTemplate: '{passages}',
  },
};

const settings = () => ({
  temperature: 0.7,
  topP: 0.8,
  topK: 20,
  repeatPenalty: 1,
  presencePenalty: 1.5,
  maxTokens: 4096,
  contextWindow: 32768,
  timeoutSeconds: 30,
  streamResponses: true,
  router,
  toolOutput,
});

const field = (label: string) => screen.getByLabelText(label);
const edit = (label: string, value: string) => {
  fireEvent.change(field(label), { target: { value } });
  fireEvent.blur(field(label));
};

describe('TuningTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.llm.isLoading = false;
    h.llm.llmSettings = settings();
    h.llm.saveLlmUpdates.mockResolvedValue(true);
  });

  it('waits for settings', () => {
    h.llm.llmSettings = null;
    render(<TuningTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  it('starts every field from the saved settings', () => {
    render(<TuningTab />);
    expect(screen.getAllByLabelText('Temperature').map((input) => (input as HTMLInputElement).value)).toEqual(['0.7', '0.1']);
    expect(field('Router model')).toHaveValue('qwen-0.5b');
    expect(field('Excerpt characters')).toHaveValue(600);
    expect(screen.getByRole('region', { name: 'Thinking' })).toBeInTheDocument();
  });

  describe('model runtime', () => {
    it('saves each field clamped to its range', () => {
      render(<TuningTab />);
      const [runtimeTemperature] = screen.getAllByLabelText('Temperature');
      fireEvent.change(runtimeTemperature, { target: { value: '3' } });
      fireEvent.blur(runtimeTemperature);
      edit('Top P', '-1');
      edit('Top K', '40.6');
      edit('Repeat penalty', '0.1');
      edit('Presence penalty', '9');
      edit('Stall timeout', '5');
      const saved = h.llm.saveLlmUpdates.mock.calls.map(([update]) => update);
      expect(saved).toEqual([
        { temperature: 2 },
        { topP: 0 },
        { topK: 41 },
        { repeatPenalty: 0.5 },
        { presencePenalty: 2 },
        { timeoutSeconds: 15 },
      ]);
    });

    it('keeps the saved value when a number field is left empty', () => {
      render(<TuningTab />);
      fireEvent.change(field('Top P'), { target: { value: '' } });
      fireEvent.blur(field('Top P'));
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ topP: 0.8 });
    });

    it('keeps the answer length within the context window', () => {
      render(<TuningTab />);
      const maxTokens = screen.getAllByLabelText('Max tokens')[0];
      fireEvent.change(maxTokens, { target: { value: '99999' } });
      fireEvent.blur(maxTokens);
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ maxTokens: 32768 });
    });

    it('switches streaming', () => {
      render(<TuningTab />);
      fireEvent.click(screen.getByRole('switch', { name: 'Stream responses' }));
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ streamResponses: false });
    });
  });

  describe('router', () => {
    it('saves switches straight away', () => {
      render(<TuningTab />);
      fireEvent.click(screen.getByRole('switch', { name: 'Route follow-up questions' }));
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, enabled: false } });
      expect(screen.getByRole('switch', { name: 'Route follow-up questions' })).not.toBeChecked();
      fireEvent.click(screen.getByRole('switch', { name: 'Prefer the last document' }));
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, preferLastDocument: true } });
    });

    it('saves text and numbers on blur, clamped and trimmed', () => {
      render(<TuningTab />);
      edit('Router model', '  gemma-1b  ');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, model: 'gemma-1b' } });
      edit('Timeout', '10');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, timeoutMs: 50 } });
      fireEvent.change(screen.getAllByLabelText('Max tokens')[1], { target: { value: '5000' } });
      fireEvent.blur(screen.getAllByLabelText('Max tokens')[1]);
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, maxTokens: 1024 } });
      const routerTemperature = screen.getAllByLabelText('Temperature')[1];
      fireEvent.change(routerTemperature, { target: { value: '1.5' } });
      fireEvent.blur(routerTemperature);
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, temperature: 1 } });
      edit('Ambiguity threshold', '0.75');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, ambiguityThreshold: 0.75 } });
      edit('Router prompt', 'Classify: {question}');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, promptTemplate: 'Classify: {question}' } });
      edit('Clarifying question prompt', 'Which one?');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ router: { ...router, clarifyPromptTemplate: 'Which one?' } });
    });

    it('saves nothing when a field leaves unchanged', () => {
      render(<TuningTab />);
      fireEvent.blur(field('Router model'));
      fireEvent.blur(field('Timeout'));
      fireEvent.blur(field('Router prompt'));
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });
  });

  describe('tool output', () => {
    it('saves sizes clamped to range', () => {
      render(<TuningTab />);
      edit('Excerpt characters', '50');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ toolOutput: { ...toolOutput, excerptChars: 100 } });
      edit('Highlight terms', '99');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ toolOutput: { ...toolOutput, highlightTermsMax: 32 } });
    });

    it('saves each template on blur', () => {
      render(<TuningTab />);
      edit('Default template', '{title} — {source}');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ toolOutput: { ...toolOutput, templates: { ...toolOutput.templates, defaultTemplate: '{title} — {source}' } } });
      edit('Get document template', '# {title}\n{body}');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ toolOutput: { ...toolOutput, templates: { ...toolOutput.templates, getDocumentTemplate: '# {title}\n{body}' } } });
      edit('Semantic search template', '- {passage}');
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ toolOutput: { ...toolOutput, templates: { ...toolOutput.templates, semanticSearchTemplate: '- {passage}' } } });
    });

    it('saves nothing for an unchanged field', () => {
      render(<TuningTab />);
      fireEvent.blur(field('Excerpt characters'));
      fireEvent.blur(field('Default template'));
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });
  });
});
