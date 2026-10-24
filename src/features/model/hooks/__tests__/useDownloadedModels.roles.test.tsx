import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';

import { reportWarmupFailure, useDownloadedModels, useDownloadedModelsListener } from '../useDownloadedModels';

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => unknown>(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  api: {
    getDownloadedModels: vi.fn(),
    setActiveChatModel: vi.fn(),
    setActiveEmbeddingModel: vi.fn(),
    setActiveUtilityModel: vi.fn(),
    clearActiveChatModel: vi.fn(),
    clearActiveEmbeddingModel: vi.fn(),
    clearActiveUtilityModel: vi.fn(),
    deleteDownloadedModel: vi.fn(),
    isModelDownloaded: vi.fn(),
    warmUpActiveChatModel: vi.fn(),
    warmUpActiveUtilityModel: vi.fn(),
    warmUpLearningModel: vi.fn(),
  },
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@/lib/api', () => ({ default: mocks.api, VaultAPI: mocks.api }));

const model = (id: string, overrides: Partial<DownloadedModel> = {}) => ({
  id, model_id: `org/${id}`, model_name: id, is_active_for_chat: false, is_active_for_embedding: false, is_active_for_utility: false, ...overrides,
} as DownloadedModel);
const models = [model('herb-chat', { is_active_for_chat: true }), model('herb-embed', { is_active_for_embedding: true }), model('spare')];

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  for (const fn of Object.values(mocks.api)) fn.mockReset().mockResolvedValue({ ok: true, data: undefined });
  mocks.api.getDownloadedModels.mockResolvedValue({ ok: true, data: models });
  mocks.handlers.clear();
  mocks.unlisten.mockReset();
  mocks.listen.mockReset().mockImplementation(async (name: string, handler: (event: { payload: unknown }) => unknown) => {
    mocks.handlers.set(name, handler);
    return mocks.unlisten;
  });
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('useDownloadedModels', () => {
  it('names the active chat and embedding models', async () => {
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await waitFor(() => expect(result.current.downloadedModels).toHaveLength(3));
    expect(result.current.activeModel?.id).toBe('herb-chat');
    expect(result.current.activeEmbeddingModel?.id).toBe('herb-embed');
    expect(result.current.downloadedModelMap.get('spare')?.model_id).toBe('org/spare');
    expect(result.current.getDownloadedModel('spare')?.id).toBe('spare');
    expect(result.current.getAllDownloadedModels()).toBe(result.current.downloadedModels);
    await expect(result.current.getActiveModel()).resolves.toMatchObject({ id: 'herb-chat' });
    await expect(result.current.getActiveEmbeddingModel()).resolves.toMatchObject({ id: 'herb-embed' });
  });

  it('has no active models when none is set, and reports a failed list', async () => {
    mocks.api.getDownloadedModels.mockResolvedValue({ ok: true, data: [model('spare')] });
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.activeModel).toBeNull();
    await expect(result.current.getActiveModel()).resolves.toBeNull();
    await expect(result.current.getActiveEmbeddingModel()).resolves.toBeNull();

    mocks.api.getDownloadedModels.mockResolvedValue({ ok: false, error: 'models folder missing' });
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const failed = renderHook(() => useDownloadedModels(), { wrapper });
    await waitFor(() => expect(failed.result.current.error).toBe('models folder missing'));
  });

  it.each([
    ['setActiveChatModel', 'setActiveChatModel', ['spare']],
    ['setActiveEmbeddingModel', 'setActiveEmbeddingModel', ['spare']],
    ['setActiveUtilityModel', 'setActiveUtilityModel', ['spare']],
    ['clearActiveChatModel', 'clearActiveChatModel', []],
    ['clearActiveEmbeddingModel', 'clearActiveEmbeddingModel', []],
    ['clearActiveUtilityModel', 'clearActiveUtilityModel', []],
  ] as const)('%s changes the role and re-reads the models', async (method, command, args) => {
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await waitFor(() => expect(result.current.downloadedModels).toHaveLength(3));
    await act(() => (result.current[method] as (...a: string[]) => Promise<void>)(...args));
    expect(mocks.api[command]).toHaveBeenCalledWith(...args);
    await waitFor(() => expect(mocks.api.getDownloadedModels).toHaveBeenCalledTimes(2));

    mocks.api[command].mockResolvedValueOnce({ ok: false, error: 'model is loading' });
    await expect(act(() => (result.current[method] as (...a: string[]) => Promise<void>)(...args))).rejects.toThrow('model is loading');
  });

  it('deletes a model and its file by default', async () => {
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await act(() => result.current.deleteDownloadedModel('spare'));
    expect(mocks.api.deleteDownloadedModel).toHaveBeenCalledWith('spare', true);
    await act(() => result.current.deleteDownloadedModel('spare', false));
    expect(mocks.api.deleteDownloadedModel).toHaveBeenLastCalledWith('spare', false);
  });

  it('asks the backend whether a model is downloaded', async () => {
    mocks.api.isModelDownloaded.mockResolvedValue({ ok: true, data: false });
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await expect(result.current.isModelDownloaded('org/spare')).resolves.toBe(false);
  });

  it.each(['warmUpActiveChatModel', 'warmUpActiveUtilityModel', 'warmUpLearningModel'] as const)('%s reports a model that would not load', async (method) => {
    const { result } = renderHook(() => useDownloadedModels(), { wrapper });
    await act(() => result.current[method]());
    expect(mocks.api[method]).toHaveBeenCalled();
    mocks.api[method].mockResolvedValueOnce({ ok: false, error: 'out of memory' });
    await expect(act(() => result.current[method]())).rejects.toThrow('out of memory');
  });
});

describe('reportWarmupFailure', () => {
  it('stays quiet on success and names the role on failure', () => {
    reportWarmupFailure({ ok: true, data: undefined }, 'Herb', 'chat');
    expect(toast.error).not.toHaveBeenCalled();
    reportWarmupFailure({ ok: false, error: 'GPU busy' }, 'Herb', 'utility');
    expect(toast.error).toHaveBeenCalledWith("Herb downloaded but couldn't start", { message: 'The utility model failed to load: GPU busy' });
  });
});

describe('useDownloadedModelsListener', () => {
  const completed = (modelName: string) => ({ modelId: `org/${modelName}`, modelName, totalSize: 10, fileCount: 2, timestamp: '2026-10-24T00:00:00Z' });

  async function mount() {
    const view = renderHook(() => useDownloadedModelsListener(), { wrapper });
    await waitFor(() => expect(mocks.handlers.has('download:completed')).toBe(true));
    return view;
  }

  it('refreshes on a finished download and warms up the roles it fills', async () => {
    mocks.api.getDownloadedModels.mockResolvedValue({ ok: true, data: [model('herb', { is_active_for_chat: true, is_active_for_utility: true })] });
    mocks.api.warmUpActiveChatModel.mockResolvedValue({ ok: false, error: 'no GPU' });
    await mount();
    await act(async () => { await mocks.handlers.get('download:completed')!({ payload: completed('herb') }); });
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('herb ready'));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("herb downloaded but couldn't start", { message: 'The chat model failed to load: no GPU' }));
    await waitFor(() => expect(mocks.api.warmUpActiveUtilityModel).toHaveBeenCalled());
  });

  it('does not warm up a model with no role', async () => {
    await mount();
    await act(async () => { await mocks.handlers.get('download:completed')!({ payload: completed('spare') }); });
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('spare ready'));
    expect(mocks.api.warmUpActiveChatModel).not.toHaveBeenCalled();
    expect(mocks.api.warmUpActiveUtilityModel).not.toHaveBeenCalled();
  });

  it('logs a refresh that failed and an event it cannot read', async () => {
    await mount();
    mocks.api.getDownloadedModels.mockResolvedValue({ ok: false, error: 'busy' });
    await act(async () => { await mocks.handlers.get('download:completed')!({ payload: completed('spare') }); });
    await waitFor(() => expect(console.error).toHaveBeenCalledWith('[useDownloadedModels] Failed to refresh after completion:', expect.any(Error)));
    act(() => { mocks.handlers.get('download:completed')!({ payload: { modelName: 3 } }); });
    expect(console.error).toHaveBeenCalledWith('[useDownloadedModels] Invalid completion event:', expect.anything());
  });

  it('stops listening on unmount, including a listener that attaches late', async () => {
    const { unmount } = await mount();
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);

    let attach!: (unlisten: () => void) => void;
    mocks.listen.mockImplementationOnce(() => new Promise((resolve) => { attach = resolve; }));
    const late = renderHook(() => useDownloadedModelsListener(), { wrapper });
    late.unmount();
    await act(async () => { attach(mocks.unlisten); });
    expect(mocks.unlisten).toHaveBeenCalledTimes(2);
  });

  it('logs a listener that could not be set up', async () => {
    mocks.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useDownloadedModelsListener(), { wrapper });
    await waitFor(() => expect(console.error).toHaveBeenCalledWith('[useDownloadedModels] Failed to setup event listener:', expect.any(Error)));
  });
});
