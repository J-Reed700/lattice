import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { ChatTab } from './ChatTab';

const h = vi.hoisted(() => ({
  llm: { llmSettings: null as null | Record<string, any>, isLoading: false, saveLlmUpdates: vi.fn(), reload: vi.fn() },
  models: { activeModel: null as null | Record<string, string>, activeEmbeddingModel: null as null | Record<string, string> },
  probe: vi.fn(),
  setCloudApiKey: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => h.models }));
vi.mock('@/features/settings/hooks/connectionProbes', () => ({ probeOllamaServer: h.probe }));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useCloudApiKeyMutation: () => ({ setCloudApiKey: h.setCloudApiKey }) }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('./LlamaCppConnection', () => ({ LlamaCppConnection: () => <section aria-label="llama.cpp connection" /> }));

const settings = (overrides: Record<string, unknown> = {}) => ({
  provider: 'ollama',
  ollamaUrl: 'http://garden.local:11434',
  model: 'mistral',
  ollamaUtilityModel: '',
  ollamaAuthHeaderName: '',
  ollamaAuthHeaderValue: '',
  boundedConversationMemory: true,
  llamaCpp: { url: '', model: '' },
  router: { model: '  tiny-router  ' },
  ...overrides,
});

const connected = async (models = ['mistral', 'phi']) => {
  h.probe.mockResolvedValueOnce({ ok: true, data: { models, endpoint: 'http://garden.local:11434/api/tags' } });
  fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
  await waitFor(() => expect(h.toast.success).toHaveBeenCalled());
};

describe('ChatTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.llm.isLoading = false;
    h.llm.llmSettings = settings();
    h.llm.saveLlmUpdates.mockResolvedValue(true);
    h.models = { activeModel: null, activeEmbeddingModel: null };
  });

  it('shows loading, then a retry when settings cannot be read', () => {
    h.llm.isLoading = true;
    h.llm.llmSettings = null;
    const first = render(<ChatTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    first.unmount();
    h.llm.isLoading = false;
    render(<ChatTab />);
    expect(screen.getByText("Couldn't read chat settings.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(h.llm.reload).toHaveBeenCalled();
    expect(screen.queryByText('Ollama server')).not.toBeInTheDocument();
  });

  describe('provider', () => {
    it.each([
      ['openai', { provider: 'openai', model: 'gpt-6-astra', maxTokens: 8192 }],
      ['anthropic', { provider: 'anthropic', model: 'claude-opus-5', maxTokens: 8192 }],
      ['google', { provider: 'google', model: 'gemini-2.5-flash', maxTokens: 8192 }],
      ['local', { provider: 'local' }],
    ])('switching to %s saves its defaults', (value, update) => {
      render(<ChatTab />);
      fireEvent.change(screen.getByLabelText('Chat provider'), { target: { value } });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith(update);
    });

    it('shows llama.cpp for its provider, Auto, or a saved llama.cpp server', () => {
      const first = render(<ChatTab />);
      expect(screen.queryByRole('region', { name: 'llama.cpp connection' })).not.toBeInTheDocument();
      first.unmount();
      h.llm.llmSettings = settings({ provider: 'local', llamaCpp: { url: 'http://box:8080', model: '' } });
      const second = render(<ChatTab />);
      expect(screen.getByRole('region', { name: 'llama.cpp connection' })).toBeInTheDocument();
      expect(screen.queryByText('Ollama server')).not.toBeInTheDocument();
      second.unmount();
      h.llm.llmSettings = settings({ provider: undefined });
      render(<ChatTab />);
      expect(screen.getByLabelText('Chat provider')).toHaveValue('auto');
      expect(screen.getByRole('region', { name: 'llama.cpp connection' })).toBeInTheDocument();
      expect(screen.getByText('Ollama server')).toBeInTheDocument();
    });
  });

  describe('cloud connection', () => {
    beforeEach(() => {
      h.llm.llmSettings = settings({ provider: 'anthropic', model: 'claude-opus-5' });
    });

    it('saves a changed model name on blur, never a blank one', () => {
      render(<ChatTab />);
      const model = screen.getByLabelText('Model');
      fireEvent.change(model, { target: { value: '  claude-sonnet-5 ' } });
      fireEvent.blur(model);
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ model: 'claude-sonnet-5' });
      fireEvent.change(model, { target: { value: '  ' } });
      fireEvent.blur(model);
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledTimes(1);
    });

    it('stores the API key in the keychain and clears the field', async () => {
      const pending = deferred<{ ok: true }>();
      h.setCloudApiKey.mockReturnValueOnce(pending.promise);
      render(<ChatTab />);
      expect(screen.getByRole('button', { name: 'Save key' })).toBeDisabled();
      fireEvent.change(screen.getByLabelText('API key'), { target: { value: ' sk-test ' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save key' }));
      expect(screen.getByRole('button', { name: 'Saving…' })).toBeDisabled();
      expect(h.setCloudApiKey).toHaveBeenCalledWith({ provider: 'anthropic', key: 'sk-test' });
      await act(async () => { pending.resolve({ ok: true }); await pending.promise; });
      expect(h.toast.success).toHaveBeenCalledWith('API key saved');
      expect(screen.getByLabelText('API key')).toHaveValue('');
    });

    it('reports a key the keychain refused', async () => {
      h.setCloudApiKey.mockResolvedValueOnce({ ok: false, error: 'keychain locked' });
      render(<ChatTab />);
      fireEvent.change(screen.getByLabelText('API key'), { target: { value: 'sk-test' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save key' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Could not save API key', { message: 'keychain locked' }));
      expect(screen.getByLabelText('API key')).toHaveValue('sk-test');
    });

    it('names the cloud model as the active chat model', () => {
      render(<ChatTab />);
      expect(screen.getAllByText('claude-opus-5').length).toBeGreaterThan(0);
      expect(screen.getByText('anthropic')).toBeInTheDocument();
    });
  });

  describe('Ollama server', () => {
    it('saves a changed URL on blur only', () => {
      render(<ChatTab />);
      const url = screen.getByLabelText('Server URL');
      fireEvent.blur(url);
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
      fireEvent.change(url, { target: { value: ' http://kitchen.local:11434 ' } });
      fireEvent.blur(url);
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ ollamaUrl: 'http://kitchen.local:11434' });
    });

    it('needs a URL before testing', () => {
      render(<ChatTab />);
      fireEvent.change(screen.getByLabelText('Server URL'), { target: { value: '  ' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      expect(h.toast.error).toHaveBeenCalledWith('Enter an Ollama server URL first');
      expect(h.probe).not.toHaveBeenCalled();
    });

    it('lists models after a test and lets you pick chat and utility models', async () => {
      render(<ChatTab />);
      expect(screen.getByLabelText('Model')).toBeDisabled();
      await connected();
      expect(h.toast.success).toHaveBeenCalledWith('Connected. Loaded 2 model(s) from http://garden.local:11434/api/tags.');
      expect(screen.getByText('2 model(s) from http://garden.local:11434/api/tags')).toBeInTheDocument();
      fireEvent.change(screen.getByLabelText('Model'), { target: { value: 'phi' } });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ model: 'phi', ollamaConfigured: true });
      fireEvent.change(screen.getByLabelText('Model'), { target: { value: '' } });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledTimes(1);
      fireEvent.change(screen.getByLabelText('Utility model'), { target: { value: 'phi' } });
      expect(h.llm.saveLlmUpdates).toHaveBeenLastCalledWith({ ollamaUtilityModel: 'phi' });
      fireEvent.change(screen.getByLabelText('Utility model'), { target: { value: '' } });
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledTimes(2);
    });

    it('drops the chosen model when the server does not have it', async () => {
      render(<ChatTab />);
      await connected(['llama3']);
      expect(screen.getByLabelText('Model')).toHaveValue('');
      expect(screen.getByRole('button', { name: 'Save Ollama connection' })).toBeDisabled();
    });

    it('reports an unreachable server, preferring the detailed reason', async () => {
      render(<ChatTab />);
      h.probe.mockResolvedValueOnce({ ok: false, error: 'connect failed', details: { details: 'connection refused on 11434' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't reach the Ollama server", { message: 'connection refused on 11434' }));
      h.probe.mockResolvedValueOnce({ ok: false, error: 'timeout' });
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't reach the Ollama server", { message: 'timeout' }));
      expect(screen.getByLabelText('Model')).toBeDisabled();
    });

    it('shows a test in progress', async () => {
      const pending = deferred<unknown>();
      h.probe.mockReturnValueOnce(pending.promise);
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      expect(screen.getByRole('button', { name: 'Testing…' })).toBeDisabled();
      await act(async () => { pending.resolve({ ok: false, error: 'x' }); await pending.promise; });
    });

    it('saves the whole connection', async () => {
      const pending = deferred<boolean>();
      h.llm.saveLlmUpdates.mockReturnValueOnce(pending.promise);
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Save Ollama connection' }));
      expect(screen.getByRole('button', { name: 'Save Ollama connection' })).toHaveTextContent('Saving…');
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({
        ollamaConfigured: true, ollamaUrl: 'http://garden.local:11434', model: 'mistral',
        ollamaUtilityModel: '', ollamaAuthHeaderName: '', ollamaAuthHeaderValue: '',
      });
      await act(async () => { pending.resolve(true); await pending.promise; });
      expect(h.toast.success).toHaveBeenCalledWith('Ollama settings saved');
    });

    it('stays quiet when the connection save is refused', async () => {
      h.llm.saveLlmUpdates.mockResolvedValueOnce(false);
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Save Ollama connection' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Save Ollama connection' })).toHaveTextContent('Save connection'));
      expect(h.toast.success).not.toHaveBeenCalled();
    });
  });

  describe('auth', () => {
    it('opens by itself when a header is saved, and toggles', () => {
      h.llm.llmSettings = settings({ ollamaAuthHeaderName: 'X-Key', ollamaAuthHeaderValue: 'abc' });
      render(<ChatTab />);
      expect(screen.getByLabelText('Auth header name')).toHaveValue('X-Key');
      fireEvent.click(screen.getByRole('button', { name: 'Hide auth' }));
      expect(screen.queryByLabelText('Auth header name')).not.toBeInTheDocument();
    });

    it('saves a complete changed header pair on blur', () => {
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Auth' }));
      fireEvent.change(screen.getByLabelText('Auth header name'), { target: { value: ' X-Key ' } });
      fireEvent.blur(screen.getByLabelText('Auth header name'));
      expect(screen.getByText('Fill in both name and value, or clear both.')).toBeInTheDocument();
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
      fireEvent.change(screen.getByLabelText('Auth header value'), { target: { value: 'secret' } });
      fireEvent.blur(screen.getByLabelText('Auth header value'));
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ ollamaAuthHeaderName: 'X-Key', ollamaAuthHeaderValue: 'secret' });
    });

    it('does not resave an unchanged header', () => {
      h.llm.llmSettings = settings({ ollamaAuthHeaderName: 'X-Key', ollamaAuthHeaderValue: 'abc' });
      render(<ChatTab />);
      fireEvent.blur(screen.getByLabelText('Auth header value'));
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });

    it('refuses to test or save with half a header', () => {
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Auth' }));
      fireEvent.change(screen.getByLabelText('Auth header value'), { target: { value: 'secret' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      fireEvent.click(screen.getByRole('button', { name: 'Save Ollama connection' }));
      expect(h.toast.error).toHaveBeenCalledTimes(2);
      expect(h.toast.error).toHaveBeenCalledWith('Set both the header name and value, or clear both');
      expect(h.probe).not.toHaveBeenCalled();
      expect(h.llm.saveLlmUpdates).not.toHaveBeenCalled();
    });

    it('refuses to test with only a header name', () => {
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Auth' }));
      fireEvent.change(screen.getByLabelText('Auth header name'), { target: { value: 'X-Key' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
      expect(h.toast.error).toHaveBeenCalledWith('Set both the header name and value, or clear both');
    });

    it('needs a URL and a model to save', () => {
      render(<ChatTab />);
      fireEvent.change(screen.getByLabelText('Server URL'), { target: { value: ' ' } });
      // The button is disabled, but the handler guards too.
      expect(screen.getByRole('button', { name: 'Save Ollama connection' })).toBeDisabled();
    });

    it('builds a basic auth header from a username and password', () => {
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Auth' }));
      fireEvent.click(screen.getByRole('button', { name: 'Set header' }));
      expect(h.toast.error).toHaveBeenCalledWith('Enter a username for basic auth');
      fireEvent.change(screen.getByLabelText('Basic auth username'), { target: { value: 'ada' } });
      fireEvent.change(screen.getByLabelText('Basic auth password'), { target: { value: 'lovelace' } });
      fireEvent.click(screen.getByRole('button', { name: 'Set header' }));
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({
        ollamaAuthHeaderName: 'Authorization',
        ollamaAuthHeaderValue: `Basic ${btoa('ada:lovelace')}`,
      });
    });
  });

  describe('memory', () => {
    it('turns conversation memory off and says what that means', async () => {
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('switch', { name: 'Remember requirements in a conversation' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Conversation memory off', expect.anything()));
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({ boundedConversationMemory: false });
    });

    it('turns it on, and stays quiet when the save is refused', async () => {
      h.llm.llmSettings = settings({ boundedConversationMemory: false });
      render(<ChatTab />);
      fireEvent.click(screen.getByRole('switch', { name: 'Remember requirements in a conversation' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Conversation memory on', expect.anything()));
      h.llm.saveLlmUpdates.mockResolvedValueOnce(false);
      fireEvent.click(screen.getByRole('switch', { name: 'Remember requirements in a conversation' }));
      await waitFor(() => expect(h.llm.saveLlmUpdates).toHaveBeenCalledTimes(2));
      expect(h.toast.success).toHaveBeenCalledTimes(1);
    });
  });

  describe('active models', () => {
    it('names the local chat and embedding models and the router', () => {
      h.llm.llmSettings = settings({ provider: 'local' });
      h.models = {
        activeModel: { model_name: 'Qwen 9B', model_id: 'qwen-9b' },
        activeEmbeddingModel: { model_name: 'BGE small', model_id: 'bge-small' },
      };
      render(<ChatTab />);
      expect(screen.getByText('Qwen 9B')).toBeInTheDocument();
      expect(screen.getByText('qwen-9b')).toBeInTheDocument();
      expect(screen.getByText('BGE small')).toBeInTheDocument();
      expect(screen.getByText('tiny-router')).toBeInTheDocument();
    });

    it('names the llama.cpp model, and says Not set when nothing is chosen', () => {
      h.llm.llmSettings = settings({ provider: 'llamacpp', llamaCpp: { url: 'http://box', model: 'gemma-27b' }, router: { model: ' ' } });
      render(<ChatTab />);
      expect(screen.getByText('gemma-27b')).toBeInTheDocument();
      expect(screen.getByText('llamacpp')).toBeInTheDocument();
      expect(screen.getAllByText('Not set')).toHaveLength(2);
    });

    it('says Not set for every model before settings load', () => {
      h.llm.llmSettings = null;
      render(<ChatTab />);
      expect(screen.getAllByText('Not set')).toHaveLength(3);
    });
  });
});
