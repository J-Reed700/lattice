import type { ReactNode } from 'react';

import { act, render, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelRolesProvider, useModelRoles } from './ModelRolesContext';

const h = vi.hoisted(() => ({
  settings: undefined as unknown,
  updateSettings: vi.fn(),
  models: {
    fetchDownloadedModels: vi.fn(),
    setActiveChatModel: vi.fn(),
    setActiveEmbeddingModel: vi.fn(),
    setActiveUtilityModel: vi.fn(),
    clearActiveChatModel: vi.fn(),
    clearActiveEmbeddingModel: vi.fn(),
    clearActiveUtilityModel: vi.fn(),
  },
  toast: { info: vi.fn(), error: vi.fn() },
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: h.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: h.updateSettings }),
}));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => h.models }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const row = (model_id: string, backend: 'local' | 'ollama' = 'local') => ({ id: model_id, model_id, model_name: model_id, backend });
const wrapper = ({ children }: { children: ReactNode }) => <ModelRolesProvider>{children}</ModelRolesProvider>;

const setup = async () => {
  const hook = renderHook(() => useModelRoles(), { wrapper });
  await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
  return hook;
};

describe('ModelRolesProvider roles', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = { llm: { provider: 'local', model: 'llama3:8b', utilityModel: null } };
    h.models.fetchDownloadedModels.mockResolvedValue([row('qwen-9b'), row('mistral', 'ollama')]);
    for (const fn of Object.values(h.models)) if (fn !== h.models.fetchDownloadedModels) fn.mockResolvedValue(undefined);
    h.updateSettings.mockResolvedValue(undefined);
  });

  it('refuses to be used outside its provider', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow('useModelRoles must be used inside <ModelRolesProvider>');
    error.mockRestore();
  });

  it('splits local and Ollama models and reports a failed read', async () => {
    const { result } = await setup();
    expect(result.current.localModels.map((m) => m.model_id)).toEqual(['qwen-9b']);
    expect(result.current.ollamaModels.map((m) => m.model_id)).toEqual(['mistral']);
    h.models.fetchDownloadedModels.mockRejectedValueOnce(new Error('models dir missing'));
    await act(() => result.current.refresh());
    expect(result.current.error).toBe('models dir missing');
  });

  describe('chat', () => {
    it('sets an Ollama model and switches the provider to it', async () => {
      const { result } = await setup();
      await act(() => result.current.assignRole('mistral', 'chat'));
      expect(h.models.setActiveChatModel).toHaveBeenCalledWith('mistral');
      expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { provider: 'ollama' } });
    });

    it('keeps the provider when it already matches', async () => {
      const { result } = await setup();
      await act(() => result.current.assignRole('qwen-9b', 'chat'));
      expect(h.updateSettings).not.toHaveBeenCalled();
    });

    it('clears the chat model, stopping an Ollama provider too', async () => {
      h.settings = { llm: { provider: 'ollama', model: '' } };
      const { result } = await setup();
      await act(() => result.current.assignRole(null, 'chat'));
      expect(h.models.clearActiveChatModel).toHaveBeenCalled();
      expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { provider: 'local' } });
    });

    it('reports a missing model or settings that have not loaded', async () => {
      const { result } = await setup();
      await act(() => result.current.assignRole('gone', 'chat'));
      expect(h.toast.error).toHaveBeenCalledWith('Failed to set chat role', { message: 'Model is no longer available. Please refresh and try again.' });
      h.settings = undefined;
      const second = await setup();
      await act(() => second.result.current.assignRole('qwen-9b', 'chat'));
      expect(h.toast.error).toHaveBeenLastCalledWith('Failed to set chat role', { message: 'Chat settings have not loaded. Please try again.' });
    });
  });

  it('sets and clears the embedding model', async () => {
    const { result } = await setup();
    await act(() => result.current.assignRole('qwen-9b', 'embedding'));
    expect(h.toast.info).toHaveBeenCalledWith('Embedding model prepared. Restart Lattice to use its search index.');
    await act(() => result.current.assignRole(null, 'embedding'));
    expect(h.models.clearActiveEmbeddingModel).toHaveBeenCalled();
  });

  it('sets and clears the utility model, dropping a connection reference', async () => {
    h.settings = { llm: { provider: 'local', model: '', utilityModel: { kind: 'ollama', model: 'phi' } } };
    const { result } = await setup();
    await act(() => result.current.assignRole('qwen-9b', 'utility'));
    expect(h.models.setActiveUtilityModel).toHaveBeenCalledWith('qwen-9b');
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { utilityModel: null } });
    await act(() => result.current.assignRole(null, 'utility'));
    expect(h.models.clearActiveUtilityModel).toHaveBeenCalled();
  });

  it('names learning and verifier models by reference, or clears them', async () => {
    const { result } = await setup();
    await act(() => result.current.assignRole('qwen-9b', 'learning'));
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { learningModel: { kind: 'local', model_id: 'qwen-9b' } } });
    await act(() => result.current.assignRole('mistral', 'verifier'));
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { verifierModel: { kind: 'ollama', model: 'mistral' } } });
    await act(() => result.current.assignRole(null, 'verifier'));
    expect(h.updateSettings).toHaveBeenLastCalledWith({ category: 'llm', updates: { verifierModel: null } });
    await act(() => result.current.assignRole('gone', 'learning'));
    expect(h.toast.error).toHaveBeenCalledWith('Failed to set learning role', expect.anything());
  });

  it('needs the Ollama chat tag before the Ollama server can verify', async () => {
    h.settings = { llm: { provider: 'ollama', model: '  ' } };
    h.models.fetchDownloadedModels.mockResolvedValue([row('__ollama_server__', 'ollama')]);
    const { result } = await setup();
    await act(() => result.current.assignRole('__ollama_server__', 'verifier'));
    expect(h.toast.error).toHaveBeenCalledWith('Failed to set verifier role', { message: 'Set the Ollama chat tag in Chat settings first.' });
  });

  it('uses a connection as the utility model, reporting a failure', async () => {
    const { result } = await setup();
    await act(() => result.current.assignUtilityReference({ kind: 'ollama', model: 'phi' } as never));
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { utilityModel: { kind: 'ollama', model: 'phi' } } });
    expect(h.models.clearActiveUtilityModel).toHaveBeenCalled();
    await act(() => result.current.assignUtilityReference(null));
    expect(h.models.clearActiveUtilityModel).toHaveBeenCalledTimes(1);
    h.updateSettings.mockRejectedValueOnce('settings locked');
    await act(() => result.current.assignUtilityReference(null));
    expect(h.toast.error).toHaveBeenCalledWith('Failed to set utility role', { message: 'settings locked' });
  });
});

function Probe() {
  useModelRoles();
  return null;
}
