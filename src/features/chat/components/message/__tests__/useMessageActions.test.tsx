import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useMessageActions } from '@/features/chat/components/message/useMessageActions';
import { VaultAPI } from '@/lib/api';
import type { ChatModelChoice } from '@/lib/bindings';
import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { useToastStore } from '@/stores/toastStore';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { stubGlobalChatModelWrites } from '@/tests/fixtures/globalChatModelWrites';
import type { DisplayMessage } from '@/types/conversation';
import type { DownloadedModel } from '@/types/downloadedModels';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

const state = vi.hoisted(() => ({
  conversations: [] as Array<{ id: string; modelChoice?: ChatModelChoice }>,
  regenerateResponse: vi.fn(),
  setConversationModelChoice: vi.fn(),
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
  downloaded_at: '2026-01-01T00:00:00Z', last_used_at: null, use_count: 0, is_active_for_chat: true,
  is_active_for_embedding: false, is_active_for_utility: false, backend: 'local', model_type: 'chat',
  ...overrides,
});

const message = { id: 'answer', conversationId: 'chat-a', role: 'assistant', content: 'An answer.' } as DisplayMessage;

const render = () =>
  renderHook(() =>
    useMessageActions({
      message,
      messageId: 'answer',
      conversationId: 'chat-a',
      conversationSpaceId: null,
      previousMessageId: 'question',
      activeConversationId: 'chat-a',
      isMessageBookmarked: false,
      normalizedMarkdownContent: 'An answer.',
    })
  ).result;

const ORNITH = { kind: 'local', model_id: 'ornith-1.5-9b' } as const;
let expectNoGlobalWrite: () => void;

describe('Try with another model', () => {
  beforeEach(() => {
    useToastStore.setState({ toasts: [] });
    const settings = makeAppSettings();
    settings.llm.provider = 'local';
    appSettings.current = settings;
    models.current = [row({ id: 'big' }), row({ id: 'small', model_id: 'ornith-1.5-9b', model_name: 'Ornith-1.5-9B', is_active_for_chat: false })];
    state.conversations = [{ id: 'chat-a', modelChoice: { kind: 'default' } }];
    state.regenerateResponse = vi.fn().mockResolvedValue('answered' satisfies GenerationOutcome);
    state.setConversationModelChoice = vi.fn().mockResolvedValue({});
    expectNoGlobalWrite = stubGlobalChatModelWrites(api);
  });

  it('answers once with the picked model and changes no setting', async () => {
    const result = render();
    await act(() => result.current.handleTryWithModel('ornith-1.5-9b', 'Ornith-1.5-9B'));

    expect(state.regenerateResponse).toHaveBeenCalledWith('chat-a', { model: ORNITH });
    expect(state.setConversationModelChoice).not.toHaveBeenCalled();
    expectNoGlobalWrite();
  });

  it('offers to keep the model for this chat, and saves it only when asked', async () => {
    const result = render();
    await act(() => result.current.handleTryWithModel('ornith-1.5-9b', 'Ornith-1.5-9B'));

    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe('Answered with Ornith-1.5-9B');
    expect(toast.action?.label).toBe('Use Ornith-1.5-9B for this chat');
    expect(state.setConversationModelChoice).not.toHaveBeenCalled();

    await act(async () => { toast.action?.onClick(); });
    expect(state.setConversationModelChoice).toHaveBeenCalledWith('chat-a', { kind: 'explicit', model: ORNITH });
  });

  it('offers nothing to keep when the chat already answers with that model', async () => {
    state.conversations = [{ id: 'chat-a', modelChoice: { kind: 'explicit', model: ORNITH } }];
    const result = render();
    await act(() => result.current.handleTryWithModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe('Answered with Ornith-1.5-9B');
    expect(toast.action).toBeUndefined();
  });

  it('claims no answer when none came back, and offers no way back from a switch that never happened', async () => {
    state.regenerateResponse = vi.fn().mockResolvedValue('failed' satisfies GenerationOutcome);
    const result = render();
    await act(() => result.current.handleTryWithModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe("Couldn't regenerate");
    expect(toast.action).toBeUndefined();
  });

  it('says a stopped answer left the chat as it was', async () => {
    state.regenerateResponse = vi.fn().mockResolvedValue('cancelled' satisfies GenerationOutcome);
    const result = render();
    await act(() => result.current.handleTryWithModel('ornith-1.5-9b', 'Ornith-1.5-9B'));
    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe('Stopped Ornith-1.5-9B');
    expect(toast.message).toBe("You stopped the answer. This chat's model is unchanged.");
    expect(toast.action).toBeUndefined();
  });
});
