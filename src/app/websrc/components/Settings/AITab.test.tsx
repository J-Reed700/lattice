import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ChatTab } from './AITab/ChatTab';
import { PromptsTab } from './AITab/PromptsTab';
import { VaultAPI } from '../../lib/api';
import { useSettingsStore } from '../../stores/settingsStore';

import type { LlmSettingsContextValue } from './AITab/useLlmSettings';

const mockLlmSettings: LlmSettingsContextValue = {
  llmSettings: {
    provider: 'auto',
    ollamaUrl: 'http://localhost:11434',
    model: '',
    ollamaAuthHeaderName: '',
    ollamaAuthHeaderValue: '',
    externalModelDirectories: [],
    temperature: 0.7,
    topP: 0.9,
    topK: 40,
    repeatPenalty: 1.1,
    maxTokens: 131072,
    contextWindow: 131072,
    timeoutSeconds: 30,
    streamResponses: true,
    prompts: {
      systemPrompt: '',
      greetingPromptTemplate: '',
      ragPromptTemplate: '',
      noContextPromptTemplate: '',
      toolFollowupPromptTemplate: '',
    },
    router: {
      enabled: true,
      model: '',
      timeoutMs: 2000,
      maxTokens: 256,
      temperature: 0.1,
      ambiguityThreshold: 0.3,
      preferLastDocument: false,
      promptTemplate: '',
      clarifyPromptTemplate: '',
    },
    toolOutput: {
      maxChars: 50000,
      excerptChars: 500,
      maxResults: 10,
      highlightTermsMax: 8,
      templates: {
        defaultTemplate: '',
        getDocumentTemplate: '',
        semanticSearchTemplate: '',
      },
    },
    verification: { enabled: true },
    customTools: [],
  } as never,
  isLoading: false,
  saveLlmUpdates: vi.fn().mockResolvedValue(true),
};

vi.mock('./AITab/useLlmSettings', () => ({
  useLlmSettings: () => mockLlmSettings,
}));

vi.mock('../../stores/settingsStore');
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));
vi.mock('../../hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({
    getActiveModel: vi.fn().mockResolvedValue(undefined),
    getActiveEmbeddingModel: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('./ModelCatalog', () => ({
  ModelCatalogBrowser: () => <div data-testid="model-catalog-mock">Mocked Catalog</div>,
}));

describe('ChatTab', () => {
  const mockUpdateAI = vi.fn();
  const mockSettings = {
    embeddingModel: 'bge-m3' as const,
    ocrModel: 'qwen2.5-vl-2b' as const,
    useQuantization: true,
    enableAgenticRAG: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useSettingsStore).mockImplementation((selector) => {
      const state = {
        settings: { ai: mockSettings },
        updateAI: mockUpdateAI,
      };
      return selector(state as never);
    });
  });

  it('renders Chat settings header', () => {
    render(<ChatTab />);
    expect(screen.getByText('Chat')).toBeInTheDocument();
    expect(screen.getByText('Provider connection, active models, and runtime options')).toBeInTheDocument();
  });

  describe('Performance Options', () => {
    it('displays quantization checkbox with current state', () => {
      render(<ChatTab />);

      const checkbox = screen.getByLabelText(/Use Quantization/) as HTMLInputElement;
      expect(checkbox).toBeChecked();
    });

    it('toggles quantization when checkbox clicked', async () => {
      const user = userEvent.setup();
      render(<ChatTab />);

      const checkbox = screen.getByLabelText(/Use Quantization/);
      await user.click(checkbox);

      expect(mockUpdateAI).toHaveBeenCalledWith({ useQuantization: false });
    });

    it('displays quantization description', () => {
      render(<ChatTab />);

      expect(
        screen.getByText(/Reduce model size and improve speed with minimal quality loss/)
      ).toBeInTheDocument();
    });
  });

  it('renders section headers with icons', () => {
    render(<ChatTab />);

    expect(screen.getByText('Chat Provider')).toBeInTheDocument();
    expect(screen.getByText('Options')).toBeInTheDocument();
  });

  describe('Ollama Connection', () => {
    it('shows a model dropdown and connection test button', async () => {
      render(<ChatTab />);

      expect(await screen.findByRole('button', { name: 'Test Connection' })).toBeInTheDocument();
      const ollamaModel = await screen.findByLabelText('Model');
      expect(ollamaModel.tagName).toBe('SELECT');
      expect(ollamaModel).toBeDisabled();
    });

    it('loads model options after a successful connection test', async () => {
      const user = userEvent.setup();
      const testConnectionMock = vi.fn().mockResolvedValueOnce({
        ok: true,
        data: {
          endpoint: '/v1/models',
          models: ['llama3.2:latest', 'qwen2.5:latest'],
        },
      });
      vi.spyOn(VaultAPI, 'testOllamaConnection').mockImplementation(testConnectionMock);

      render(<ChatTab />);
      const testConnection = await screen.findByRole('button', { name: 'Test Connection' });
      await user.click(testConnection);

      expect(testConnectionMock).toHaveBeenCalled();
      expect(await screen.findByRole('option', { name: 'llama3.2:latest' })).toBeInTheDocument();
      expect(await screen.findByRole('option', { name: 'qwen2.5:latest' })).toBeInTheDocument();
      expect(await screen.findByLabelText('Model')).not.toBeDisabled();
    });
  });
});

describe('PromptsTab', () => {
  const mockUpdateAI = vi.fn();
  const mockSettings = {
    embeddingModel: 'bge-m3' as const,
    ocrModel: 'qwen2.5-vl-2b' as const,
    useQuantization: true,
    enableAgenticRAG: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useSettingsStore).mockImplementation((selector) => {
      const state = {
        settings: { ai: mockSettings },
        updateAI: mockUpdateAI,
      };
      return selector(state as never);
    });
  });

  describe('Agentic RAG Option', () => {
    it('displays agentic RAG checkbox with current state', () => {
      render(<PromptsTab />);

      const checkbox = screen.getByLabelText(/Agentic RAG/) as HTMLInputElement;
      expect(checkbox).not.toBeChecked();
    });

    it('toggles agentic RAG when checkbox clicked', async () => {
      const user = userEvent.setup();
      render(<PromptsTab />);

      const checkbox = screen.getByLabelText(/Agentic RAG/);
      await user.click(checkbox);

      expect(mockUpdateAI).toHaveBeenCalledWith({ enableAgenticRAG: true });
    });

    it('shows experimental badge for agentic RAG', () => {
      render(<PromptsTab />);

      expect(screen.getByText('Experimental')).toBeInTheDocument();
    });

    it('displays agentic RAG description', () => {
      render(<PromptsTab />);

      expect(
        screen.getByText(/AI agents autonomously refine searches/)
      ).toBeInTheDocument();
    });
  });
});
