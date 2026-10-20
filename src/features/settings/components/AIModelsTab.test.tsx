import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { resolveDefaultChatOption } from '@/utils/chatModelSelection';

import { AIModelsTab } from './AIModelsTab';
import { LlmSettingsProvider } from './AITab/LlmSettingsProvider';

let settings = makeAppSettings();
let models: DownloadedModel[];

function row(name: string) {
  return within(screen.getByText(name, { exact: true }).parentElement!.parentElement!);
}

async function openRowConfig(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(row(name).getByRole('button', { name: 'Configure model' }));
  return within(await screen.findByRole('dialog'));
}

async function closeRowConfig(user: ReturnType<typeof userEvent.setup>) {
  const dialog = screen.queryByRole('dialog');
  if (dialog) {
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  }
}

async function renderModels() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <LlmSettingsProvider><AIModelsTab /></LlmSettingsProvider>
    </QueryClientProvider>,
  );
  await screen.findByText('Local model', { exact: true });
  return userEvent.setup();
}

describe('Downloaded model chat selection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useToastStore.setState({ toasts: [] });
    settings = makeAppSettings();
    settings.llm.provider = 'local';
    settings.llm.llamaCpp = {
      url: 'https://llama.example.com', model: 'remote-model.gguf',
      authHeaderName: 'Authorization', authHeaderValue: 'Bearer test-credential',
    };
    models = [{
      id: 'local-1', model_id: 'local-model', model_name: 'Local model',
      file_path: '/models/local.gguf', file_size_bytes: 1024,
      model_type: 'chat', downloaded_at: '2026-10-01T00:00:00Z', last_used_at: null, use_count: 0,
      is_active_for_chat: true, is_active_for_embedding: false, is_active_for_utility: true,
      backend: 'local',
    }, {
      id: 'ollama', model_id: '__ollama_server__', model_name: 'Ollama',
      file_path: '', file_size_bytes: 0, model_type: 'chat',
      downloaded_at: '2026-10-01T00:00:00Z', last_used_at: null, use_count: 0,
      is_active_for_chat: false, is_active_for_embedding: false, is_active_for_utility: false,
      backend: 'ollama',
    }];
    vi.spyOn(VaultAPI, 'getSettings').mockImplementation(async () => ({ ok: true, data: settings }));
    vi.spyOn(VaultAPI, 'updateSettings').mockImplementation(async ({ updates }) => {
      settings = { ...settings, llm: { ...settings.llm, ...updates } };
      return { ok: true, data: settings };
    });
    VaultAPI.getDownloadedModels = vi.fn(async () => ({ ok: true as const, data: models }));
    VaultAPI.setActiveChatModel = vi.fn(async (id) => {
      models = models.map(model => ({ ...model, is_active_for_chat: model.model_id === id }));
      return { ok: true as const, data: undefined };
    });
    VaultAPI.clearActiveUtilityModel = vi.fn(async () => {
      models = models.map(model => ({ ...model, is_active_for_utility: false }));
      return { ok: true as const, data: undefined };
    });
  });

  it('switches from local to llama.cpp and back through the real provider setting, preserving utility and connections', async () => {
    const user = await renderModels();
    const connection = settings.llm.llamaCpp;
    const ollamaModel = settings.llm.model;
    expect(row('Local model').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
    expect(row('Local model').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');

    const llamaDialog = await openRowConfig(user, 'llama.cpp connection');
    await user.click(llamaDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('llamacpp'));
    expect(resolveDefaultChatOption(settings.llm, models)?.ref).toEqual({ kind: 'llama_cpp', model: 'remote-model.gguf' });
    expect(llamaDialog.getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    await closeRowConfig(user);

    expect(row('llama.cpp connection').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
    expect(row('Local model').queryByText('Chat')).not.toBeInTheDocument();
    expect(row('Local model').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');

    const localDialog = await openRowConfig(user, 'Local model');
    await user.click(localDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('local'));
    await closeRowConfig(user);

    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model');
    expect(resolveDefaultChatOption(settings.llm, models)?.ref).toEqual({ kind: 'local', model_id: 'local-model' });
    expect(row('Local model').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
    expect(row('llama.cpp connection').queryByText('Chat')).not.toBeInTheDocument();
    expect(settings.llm.llamaCpp).toEqual(connection);
    expect(settings.llm.model).toBe(ollamaModel);
    expect(models[0].is_active_for_utility).toBe(true);
  });

  it('allows assigning llama.cpp connection to Utility and Verifier', async () => {
    const user = await renderModels();
    expect(row('Local model').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');

    const llamaDialog = await openRowConfig(user, 'llama.cpp connection');
    await user.click(llamaDialog.getByRole('button', { name: 'Utility' }));
    await waitFor(() => expect(settings.llm.utilityModel).toEqual({ kind: 'llama_cpp', model: 'remote-model.gguf' }));
    expect(VaultAPI.clearActiveUtilityModel).toHaveBeenCalledOnce();

    await user.click(llamaDialog.getByRole('button', { name: 'Verifier' }));
    await waitFor(() => expect(settings.llm.verifierModel).toEqual({ kind: 'llama_cpp', model: 'remote-model.gguf' }));
    await closeRowConfig(user);

    expect(row('llama.cpp connection').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');
    expect(row('llama.cpp connection').getByText('Verifier')).toHaveAttribute('data-active-role', 'verifier');
    expect(row('Local model').queryByText('Utility')).not.toBeInTheDocument();
  });

  it('clears the llama.cpp Utility reference when a downloaded model takes Utility', async () => {
    settings.llm.utilityModel = { kind: 'llama_cpp', model: 'remote-model.gguf' };
    models[0] = { ...models[0], is_active_for_utility: false };
    VaultAPI.setActiveUtilityModel = vi.fn(async (id) => {
      models = models.map(model => ({ ...model, is_active_for_utility: model.model_id === id }));
      return { ok: true as const, data: undefined };
    });
    const user = await renderModels();
    expect(row('llama.cpp connection').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');

    const localDialog = await openRowConfig(user, 'Local model');
    await user.click(localDialog.getByRole('button', { name: 'Utility' }));
    await waitFor(() => expect(settings.llm.utilityModel).toBeNull());
    await closeRowConfig(user);

    expect(VaultAPI.setActiveUtilityModel).toHaveBeenCalledWith('local-model');
    expect(row('Local model').getByText('Utility')).toHaveAttribute('data-active-role', 'utility');
    expect(row('llama.cpp connection').queryByText('Utility')).not.toBeInTheDocument();
  });

  it('keeps both configured remote connections accessible when Ollama is selected', async () => {
    settings.llm.provider = 'ollama';
    settings.llm.ollamaConfigured = true;
    models[1].is_active_for_chat = true;
    const user = await renderModels();
    expect(row('Ollama connection').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');

    const llamaDialog = await openRowConfig(user, 'llama.cpp connection');
    await user.click(llamaDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('llamacpp'));
    await closeRowConfig(user);

    const ollamaDialog = await openRowConfig(user, 'Ollama connection');
    await user.click(ollamaDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('ollama'));
    await closeRowConfig(user);

    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('__ollama_server__');
    expect(resolveDefaultChatOption(settings.llm, models)?.ref).toEqual({ kind: 'ollama', model: settings.llm.model });
  });

  it.each(['url', 'model'] as const)('hides an incomplete llama.cpp connection with no %s', async field => {
    settings.llm.provider = 'llamacpp';
    settings.llm.llamaCpp[field] = ' ';
    await renderModels();
    expect(screen.queryByText('llama.cpp connection')).not.toBeInTheDocument();
  });

  it('provides a visible editor for a configured connection', async () => {
    const navigate = vi.fn();
    window.addEventListener('settings:navigate-tab', navigate);
    try {
      const user = await renderModels();
      await user.click(row('llama.cpp connection').getByRole('button', { name: 'Edit connection' }));
      expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ detail: { tab: 'chat' } }));
      expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener('settings:navigate-tab', navigate);
    }
  });

  it('hides built-in Ollama defaults even when its synthetic model row has role assignments', async () => {
    settings.llm.provider = 'auto';
    settings.llm.model = 'llama3.2:latest';
    models[1].is_active_for_chat = true;
    models[1].is_active_for_utility = true;
    await renderModels();
    expect(screen.getByText('llama.cpp connection')).toBeInTheDocument();
    expect(screen.queryByText('Ollama connection')).not.toBeInTheDocument();
  });

  it('shows neither remote row for untouched defaults', async () => {
    settings.llm = makeAppSettings().llm;
    settings.llm.model = 'llama3.2:latest';
    await renderModels();
    expect(screen.queryByText('llama.cpp connection')).not.toBeInTheDocument();
    expect(screen.queryByText('Ollama connection')).not.toBeInTheDocument();
  });

  it('keeps an explicitly saved localhost connection accessible with another provider selected', async () => {
    settings.llm.ollamaConfigured = true;
    settings.llm.model = 'llama3.2:latest';
    const user = await renderModels();
    expect(screen.getByText('Ollama connection')).toBeInTheDocument();
    const ollamaDialog = await openRowConfig(user, 'Ollama connection');
    expect(ollamaDialog.getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('keeps the current selection and reports a failed provider change', async () => {
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({ ok: false, error: 'Settings could not be saved' });
    const user = await renderModels();
    const llamaDialog = await openRowConfig(user, 'llama.cpp connection');
    await user.click(llamaDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ type: 'error', title: 'Failed to set chat role' }),
    ]));
    expect(llamaDialog.getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    await closeRowConfig(user);
    expect(row('Local model').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
    expect(settings.llm.provider).toBe('local');
  });

  it('does not change the provider when selecting a downloaded model fails', async () => {
    settings.llm.provider = 'llamacpp';
    vi.spyOn(VaultAPI, 'setActiveChatModel').mockResolvedValue({ ok: false, error: 'Model is not fully downloaded' });
    const user = await renderModels();
    const localDialog = await openRowConfig(user, 'Local model');
    await user.click(localDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model'));
    await closeRowConfig(user);
    expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    expect(settings.llm.provider).toBe('llamacpp');
    expect(row('llama.cpp connection').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
  });

  it('warms only the local utility assignment while llama.cpp is selected for chat', async () => {
    settings.llm.provider = 'llamacpp';
    VaultAPI.warmUpActiveChatModel = vi.fn(async () => ({ ok: true as const, data: undefined }));
    VaultAPI.warmUpActiveUtilityModel = vi.fn(async () => ({ ok: true as const, data: undefined }));
    const user = await renderModels();
    await user.click(row('Local model').getByRole('button', { name: 'Warm up' }));
    expect(VaultAPI.warmUpActiveUtilityModel).toHaveBeenCalledOnce();
    expect(VaultAPI.warmUpActiveChatModel).not.toHaveBeenCalled();
  });

  it('still shows llama.cpp as selected if the provider write fails after a local assignment succeeds', async () => {
    settings.llm.provider = 'llamacpp';
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({ ok: false, error: 'Settings could not be saved' });
    const user = await renderModels();
    const localDialog = await openRowConfig(user, 'Local model');
    await user.click(localDialog.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ type: 'error', title: 'Failed to set chat role' }),
    ]));
    await closeRowConfig(user);
    await screen.findByText('Local model', { exact: true });
    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model');
    expect(row('Local model').queryByText('Chat')).not.toBeInTheDocument();
    expect(row('llama.cpp connection').getByText('Chat')).toHaveAttribute('data-active-role', 'chat');
  });

  describe('Vision', () => {
    it('declares that the llama.cpp model sees, saved under its model reference', async () => {
      const user = await renderModels();
      const vision = screen.getByRole('combobox', { name: 'Vision' });
      expect(vision).toHaveValue('off');

      await user.selectOptions(vision, 'gemma_siglip');
      await waitFor(() => expect(settings.llm.visionDeclarations).toEqual({
        'llamacpp:remote-model.gguf': { enabled: true, geometry: { kind: 'fixed', side: 896, tokens: 256 } },
      }));
      expect(await screen.findByText('A 1280×960 photo costs about 256 tokens on this model.')).toBeInTheDocument();
    });

    it('lists the configured cloud model on its provider default', async () => {
      settings.llm.provider = 'google';
      settings.llm.model = 'gemini-test';
      await renderModels();
      expect(screen.getByText('Google connection')).toBeInTheDocument();
      expect(screen.getAllByRole('combobox', { name: 'Vision' }).map((select) => (select as HTMLSelectElement).value))
        .toEqual(['off', 'gemini']);
    });
  });

  describe('Learning', () => {
    it('says Learning is not set and fixes it with one explicit click on the chat model', async () => {
      const user = await renderModels();
      const banner = screen.getByText(/Learning: not set/).closest('[role="status"]') as HTMLElement;
      expect(banner).toHaveTextContent('Learning: not set — lessons, outlines and study decks will not run until you choose one');
      await user.click(within(banner).getByRole('button', { name: 'Use Local model (your chat model)' }));
      await waitFor(() => expect(settings.llm.learningModel).toEqual({ kind: 'local', model_id: 'local-model' }));
      expect(screen.queryByText(/Learning: not set/)).not.toBeInTheDocument();
      expect(row('Local model').getByText('Learning')).toHaveAttribute('data-active-role', 'learning');
      // Learning is a setting of its own: the chat model is untouched.
      expect(settings.llm.provider).toBe('local');
      expect(VaultAPI.setActiveChatModel).not.toHaveBeenCalled();
    });

    it('names a Learning model that is not downloaded any more, keeps the setting, and opens the configuration menu', async () => {
      settings.llm.learningModel = { kind: 'local', model_id: 'ornith-1.5-9b' };
      const user = await renderModels();
      expect(screen.getByText('Learning model ornith-1.5-9b is not downloaded any more')).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Choose another' }));
      const dialog = await screen.findByRole('dialog');
      expect(within(dialog).getByRole('button', { name: 'Learning' })).toBeInTheDocument();
      expect(settings.llm.learningModel).toEqual({ kind: 'local', model_id: 'ornith-1.5-9b' });
      expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    });

    it('says nothing when the Learning model is set and present', async () => {
      settings.llm.learningModel = { kind: 'local', model_id: 'local-model' };
      await renderModels();
      expect(screen.queryByText(/Learning: not set/)).not.toBeInTheDocument();
      expect(screen.queryByText(/not downloaded any more/)).not.toBeInTheDocument();
    });

    it('sets the model a connection serves as the Learning model', async () => {
      settings.llm.ollamaConfigured = true;
      settings.llm.model = 'qwen3:8b';
      const user = await renderModels();
      const llamaDialog = await openRowConfig(user, 'llama.cpp connection');
      await user.click(llamaDialog.getByRole('button', { name: 'Learning' }));
      await waitFor(() => expect(settings.llm.learningModel).toEqual({ kind: 'llama_cpp', model: 'remote-model.gguf' }));
      await waitFor(() => expect(llamaDialog.getByRole('button', { name: 'Learning' })).toHaveAttribute('aria-pressed', 'true'));
      await closeRowConfig(user);

      const ollamaDialog = await openRowConfig(user, 'Ollama connection');
      await user.click(ollamaDialog.getByRole('button', { name: 'Learning' }));
      await waitFor(() => expect(settings.llm.learningModel).toEqual({ kind: 'ollama', model: 'qwen3:8b' }));
      expect(settings.llm.provider).toBe('local');
    });
  });
});
