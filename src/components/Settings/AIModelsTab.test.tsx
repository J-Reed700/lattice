import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AIModelsTab } from './AIModelsTab';
import { VaultAPI } from '../../lib/api';
import { useToastStore } from '../../stores/toastStore';
import { makeAppSettings } from '../../tests/fixtures/appSettings';
import { resolveChatModel } from '../../utils/chatModelSelection';

import type { DownloadedModel } from '../../types/downloadedModels';

let settings = makeAppSettings();
let models: DownloadedModel[];

function row(name: string) {
  return within(screen.getByText(name, { exact: true }).parentElement!.parentElement!);
}

async function renderModels() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(<QueryClientProvider client={client}><AIModelsTab /></QueryClientProvider>);
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
  });

  it('switches from local to llama.cpp and back through the real provider setting, preserving utility and connections', async () => {
    const user = await renderModels();
    const connection = settings.llm.llamaCpp;
    const ollamaModel = settings.llm.model;
    expect(row('Local model').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');

    await user.click(row('llama.cpp connection').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('llamacpp'));
    expect(resolveChatModel(settings.llm, 'local-model')).toBe('remote-model.gguf');
    expect(row('llama.cpp connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    expect(row('Local model').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    expect(row('Local model').getByRole('button', { name: 'Utility' })).toHaveAttribute('aria-pressed', 'true');

    await user.click(row('Local model').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('local'));
    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model');
    expect(resolveChatModel(settings.llm, 'local-model')).toBe('local-model');
    expect(row('Local model').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    expect(row('llama.cpp connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    expect(settings.llm.llamaCpp).toEqual(connection);
    expect(settings.llm.model).toBe(ollamaModel);
    expect(models[0].is_active_for_utility).toBe(true);
  });

  it('keeps both configured remote connections accessible when Ollama is selected', async () => {
    settings.llm.provider = 'ollama';
    settings.llm.ollamaConfigured = true;
    const user = await renderModels();
    expect(row('Ollama connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(row('llama.cpp connection').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('llamacpp'));
    await user.click(row('Ollama connection').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(settings.llm.provider).toBe('ollama'));
    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('__ollama_server__');
    expect(resolveChatModel(settings.llm, 'local-model')).toBe(settings.llm.model);
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
    await renderModels();
    expect(screen.getByText('Ollama connection')).toBeInTheDocument();
    expect(row('Ollama connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('keeps the current selection and reports a failed provider change', async () => {
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({ ok: false, error: 'Settings could not be saved' });
    const user = await renderModels();
    await user.click(row('llama.cpp connection').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ type: 'error', title: 'Failed to set chat role' }),
    ]));
    expect(row('Local model').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    expect(row('llama.cpp connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    expect(settings.llm.provider).toBe('local');
  });

  it('does not change the provider when selecting a downloaded model fails', async () => {
    settings.llm.provider = 'llamacpp';
    vi.spyOn(VaultAPI, 'setActiveChatModel').mockResolvedValue({ ok: false, error: 'Model is not fully downloaded' });
    const user = await renderModels();
    await user.click(row('Local model').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model'));
    expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    expect(settings.llm.provider).toBe('llamacpp');
    expect(row('llama.cpp connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
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
    await user.click(row('Local model').getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ type: 'error', title: 'Failed to set chat role' }),
    ]));
    await screen.findByText('Local model', { exact: true });
    expect(VaultAPI.setActiveChatModel).toHaveBeenCalledWith('local-model');
    expect(row('Local model').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    expect(row('llama.cpp connection').getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
  });
});
