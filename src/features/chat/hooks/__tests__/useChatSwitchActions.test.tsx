import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatSwitchActions } from '@/features/chat/hooks/useChatSwitchActions';
import { VaultAPI } from '@/lib/api';
import type { ChatModelChoice } from '@/lib/bindings';
import { useToastStore } from '@/stores/toastStore';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { stubGlobalChatModelWrites } from '@/tests/fixtures/globalChatModelWrites';
import type { DownloadedModel } from '@/types/downloadedModels';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

const state = vi.hoisted(() => ({
  conversations: [] as Array<{ id: string; modelChoice?: ChatModelChoice }>,
  setConversationModelChoice: vi.fn(),
  createConversation: vi.fn(),
  moveConversationToSpace: vi.fn(),
}));
const appSettings = vi.hoisted(() => ({ current: undefined as ReturnType<typeof makeAppSettings> | undefined }));
const models = vi.hoisted(() => ({ current: [] as DownloadedModel[] }));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_s: typeof state) => unknown) => selector(state),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: appSettings.current }),
}));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ downloadedModels: models.current }),
}));

const row = (overrides: Partial<DownloadedModel>): DownloadedModel => ({
  id: 'row', model_name: 'Qwen3.5-27B', model_id: 'qwen3.5-27b', file_path: '/m.gguf', file_size_bytes: 1,
  downloaded_at: '2026-01-01T00:00:00Z', last_used_at: null, use_count: 0, is_active_for_chat: false,
  is_active_for_embedding: false, is_active_for_utility: false, backend: 'local', model_type: 'chat',
  ...overrides,
});

const render = (activeConversationId: string | null = 'chat-b') =>
  renderHook(() => useChatSwitchActions({ activeConversationId, applyFocusDocuments: vi.fn() })).result;

let expectNoGlobalWrite: () => void;
const toastTitles = () => useToastStore.getState().toasts.map((toast) => toast.title);

describe('useChatSwitchActions', () => {
  beforeEach(() => {
    useToastStore.setState({ toasts: [] });
    const settings = makeAppSettings();
    settings.llm.provider = 'llamacpp';
    settings.llm.llamaCpp = { url: 'http://localhost:8080', model: 'served.gguf', authHeaderName: '', authHeaderValue: '' };
    appSettings.current = settings;
    models.current = [row({ id: 'big', is_active_for_chat: true }), row({ id: 'small', model_id: 'ornith-1.5-9b', model_name: 'Ornith-1.5-9B' })];
    state.conversations = [{ id: 'chat-b', modelChoice: { kind: 'default' } }];
    state.setConversationModelChoice = vi.fn().mockResolvedValue({});
    state.createConversation = vi.fn().mockResolvedValue('new-chat');
    // The global switches a chat must never reach.
    expectNoGlobalWrite = stubGlobalChatModelWrites(api);
  });

  it("changes only this chat's model, never Settings or the model rows", async () => {
    const result = render();
    await act(() => result.current.handleChooseChatModel('ornith-1.5-9b', 'Ornith-1.5-9B'));

    expect(state.setConversationModelChoice).toHaveBeenCalledWith('chat-b', {
      kind: 'explicit', model: { kind: 'local', model_id: 'ornith-1.5-9b' },
    });
    expectNoGlobalWrite();
    expect(toastTitles()).toEqual(['This chat now uses Ornith-1.5-9B']);
  });

  it('names a connection row by the model it serves', async () => {
    const result = render();
    await act(() => result.current.handleChooseChatModel('__llamacpp_server__', 'llama.cpp · served.gguf'));
    expect(state.setConversationModelChoice).toHaveBeenCalledWith('chat-b', {
      kind: 'explicit', model: { kind: 'llama_cpp', model: 'served.gguf' },
    });
  });

  it('makes the chat first when none is open, then sets its model', async () => {
    const result = render(null);
    await act(() => result.current.handleChooseChatModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    expect(state.createConversation).toHaveBeenCalledTimes(1);
    expect(state.setConversationModelChoice).toHaveBeenCalledWith('new-chat', expect.objectContaining({ kind: 'explicit' }));
  });

  it('does nothing when the chat already uses the picked model', async () => {
    state.conversations = [{ id: 'chat-b', modelChoice: { kind: 'explicit', model: { kind: 'local', model_id: 'ornith-1.5-9b' } } }];
    const result = render();
    await act(() => result.current.handleChooseChatModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    expect(state.setConversationModelChoice).not.toHaveBeenCalled();
    expect(toastTitles()).toEqual([]);
  });

  it('puts the chat back on the default and names it', async () => {
    state.conversations = [{ id: 'chat-b', modelChoice: { kind: 'explicit', model: { kind: 'local', model_id: 'ornith-1.5-9b' } } }];
    const result = render();
    await act(() => result.current.handleUseDefault());
    expect(state.setConversationModelChoice).toHaveBeenCalledWith('chat-b', { kind: 'default' });
    expect(toastTitles()).toEqual(['This chat now uses the app default (llama.cpp · served.gguf)']);
    expectNoGlobalWrite();
  });

  it('shows the reason when the model cannot be used, and claims nothing', async () => {
    state.setConversationModelChoice = vi.fn().mockRejectedValue(new Error('No Ollama connection is configured. Add one in Settings → Chat.'));
    const result = render();
    await act(() => result.current.handleChooseChatModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe("Couldn't change this chat's model");
    expect(toast.message).toBe('No Ollama connection is configured. Add one in Settings → Chat.');
  });
});
