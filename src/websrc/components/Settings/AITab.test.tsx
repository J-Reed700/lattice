import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ChatTab } from './AITab/ChatTab';
import { VaultAPI } from '../../lib/api';

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
  reload: vi.fn(),
};

vi.mock('./AITab/useLlmSettings', () => ({
  useLlmSettings: () => mockLlmSettings,
}));

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
  beforeEach(() => {
    vi.clearAllMocks();
    mockLlmSettings.llmSettings!.provider = 'auto';
    mockLlmSettings.llmSettings!.llamaCpp = { url: 'http://localhost:8080', model: 'qwen.gguf', authHeaderName: '', authHeaderValue: '' };
  });

  it('renders one page header, a noun, with no explanatory subtitle', () => {
    render(<ChatTab />);
    expect(screen.getByRole('heading', { level: 1, name: 'Chat' })).toBeInTheDocument();
    expect(
      screen.queryByText('Provider connection, active models, and runtime options')
    ).not.toBeInTheDocument();
  });

  it('renders the section headings', () => {
    render(<ChatTab />);

    expect(screen.getByText('Provider')).toBeInTheDocument();
    expect(screen.getByText('Ollama server')).toBeInTheDocument();
    expect(screen.getByText('Active models')).toBeInTheDocument();
  });

  describe('Ollama Connection', () => {
    it('shows a model dropdown and connection test button', async () => {
      render(<ChatTab />);

      expect(await screen.findByRole('button', { name: 'Test connection' })).toBeInTheDocument();
      const ollamaModel = await screen.findByLabelText('Model');
      expect(ollamaModel.tagName).toBe('SELECT');
      expect(ollamaModel).toBeDisabled();
    });

    it('loads model options after a successful connection test', async () => {
      const user = userEvent.setup();
      const testConnectionMock = vi.fn().mockResolvedValueOnce({
        ok: true,
        data: {
          endpoint: '/api/tags',
          models: ['llama3.2:latest', 'qwen2.5:latest'],
        },
      });
      vi.spyOn(VaultAPI, 'testOllamaConnection').mockImplementation(testConnectionMock);

      render(<ChatTab />);
      const testConnection = await screen.findByRole('button', { name: 'Test connection' });
      await user.click(testConnection);

      expect(testConnectionMock).toHaveBeenCalled();
      const modelSelect = await screen.findByLabelText('Model');
      expect(within(modelSelect).getByRole('option', { name: 'llama3.2:latest' })).toBeInTheDocument();
      expect(within(modelSelect).getByRole('option', { name: 'qwen2.5:latest' })).toBeInTheDocument();
      expect(modelSelect).not.toBeDisabled();
    });
  });
  it('offers llama.cpp separately and keeps the saved Ollama connection when switching', async () => {
    const user = userEvent.setup();
    render(<ChatTab />);
    await user.selectOptions(screen.getByLabelText('Chat provider'), 'llamacpp');
    expect(mockLlmSettings.saveLlmUpdates).toHaveBeenCalledWith({ provider: 'llamacpp' });
    expect(mockLlmSettings.llmSettings!.ollamaUrl).toBe('http://localhost:11434');
  });

  it('tests llama.cpp through its own command and saves its independent connection', async () => {
    mockLlmSettings.llmSettings!.provider = 'llamacpp';
    vi.spyOn(VaultAPI, 'testLlamaCppConnection').mockResolvedValueOnce({
      ok: true, data: { endpoint: '/v1/chat/completions', models: ['qwen.gguf'] },
    });
    const user = userEvent.setup();
    render(<ChatTab />);
    const url = screen.getByLabelText('llama.cpp URL');
    await user.clear(url);
    await user.type(url, 'https://llama.example.com');
    await user.type(screen.getByLabelText('llama.cpp auth header name'), 'Authorization');
    await user.type(screen.getByLabelText('llama.cpp auth header value'), 'Basic test-token');
    await user.click(screen.getByRole('button', { name: 'Test llama.cpp connection' }));
    const connection = { url: 'https://llama.example.com', model: 'qwen.gguf', authHeaderName: 'Authorization', authHeaderValue: 'Basic test-token' };
    expect(VaultAPI.testLlamaCppConnection).toHaveBeenCalledWith(connection);
    await user.click(screen.getByRole('button', { name: 'Save connection' }));
    expect(mockLlmSettings.saveLlmUpdates).toHaveBeenCalledWith({ provider: 'llamacpp', llamaCpp: connection });
    expect(screen.getByLabelText('llama.cpp auth header value')).toHaveAttribute('type', 'password');
  });

  it('configures both remote providers in Auto without switching out of Auto', async () => {
    const user = userEvent.setup();
    render(<ChatTab />);
    expect(screen.getByText('llama.cpp server')).toBeInTheDocument();
    expect(screen.getByText('Ollama server')).toBeInTheDocument();
    expect(screen.getByLabelText('Chat provider')).toHaveValue('auto');
    await user.click(screen.getByRole('button', { name: 'Save connection' }));
    expect(mockLlmSettings.saveLlmUpdates).toHaveBeenCalledWith({ provider: 'auto', llamaCpp: mockLlmSettings.llmSettings!.llamaCpp });
  });

  it('does not save a partial llama.cpp authentication header', async () => {
    mockLlmSettings.llmSettings!.provider = 'llamacpp';
    const user = userEvent.setup();
    render(<ChatTab />);
    await user.type(screen.getByLabelText('llama.cpp auth header name'), 'Authorization');
    await user.click(screen.getByRole('button', { name: 'Save connection' }));
    expect(mockLlmSettings.saveLlmUpdates).not.toHaveBeenCalled();
  });

});
