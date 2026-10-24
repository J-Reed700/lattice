import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatSwitchActions } from '@/features/chat/hooks/useChatSwitchActions';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';

const h = vi.hoisted(() => ({
  state: {} as Record<string, any>,
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (state: unknown) => unknown) => selector(h.state),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: undefined }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const render = (activeConversationId: string | null = 'c1') => {
  const applyFocusDocuments = vi.fn();
  const { result } = renderHook(() => useChatSwitchActions({ activeConversationId, applyFocusDocuments }));
  return { actions: result.current, applyFocusDocuments };
};

describe('useChatSwitchActions spaces and defaults', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.state = {
      conversations: [{ id: 'c1', modelChoice: { kind: 'explicit', model: { kind: 'local', model_id: 'gemma' } } }],
      moveConversationToSpace: vi.fn().mockResolvedValue(undefined),
      setConversationModelChoice: vi.fn().mockResolvedValue({}),
      createConversation: vi.fn().mockResolvedValue('n1'),
    };
  });

  it('moves the chat to a space, unpinning documents first, and names it', async () => {
    const { actions, applyFocusDocuments } = render();
    await act(() => actions.handleChangeSpace('s-hist', 'History'));
    expect(applyFocusDocuments).toHaveBeenCalledWith([]);
    expect(h.state.moveConversationToSpace).toHaveBeenCalledWith('c1', 's-hist');
    expect(h.toast.success).toHaveBeenCalledWith('Now searching History', { message: 'Answers from here on use the documents filed there.' });
  });

  it('moves the chat to General', async () => {
    const { actions } = render();
    await act(() => actions.handleSearchGeneral());
    expect(h.state.moveConversationToSpace).toHaveBeenCalledWith('c1', GENERAL_SPACE_ID);
  });

  it('moves nothing with no chat open', async () => {
    const { actions } = render(null);
    await act(() => actions.handleChangeSpace('s-hist', 'History'));
    expect(h.state.moveConversationToSpace).not.toHaveBeenCalled();
  });

  it('puts the chat back on the default, explaining when Settings has none', async () => {
    const { actions } = render();
    await act(() => actions.handleUseDefault());
    expect(h.state.setConversationModelChoice).toHaveBeenCalledWith('c1', { kind: 'default' });
    expect(h.toast.success).toHaveBeenCalledWith('This chat now uses the app default', { message: 'No chat model is selected in Settings → Models.' });
  });

  it('says why the default could not be restored', async () => {
    h.state.setConversationModelChoice.mockRejectedValueOnce('offline');
    const { actions } = render();
    await act(() => actions.handleUseDefault());
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't change this chat's model", { message: 'offline' });
    expect(h.toast.success).not.toHaveBeenCalled();
  });

  it('does nothing with no chat open, or one already on the default', async () => {
    const none = render(null);
    await act(() => none.actions.handleUseDefault());
    h.state.conversations = [{ id: 'c1' }];
    const onDefault = render();
    await act(() => onDefault.actions.handleUseDefault());
    expect(h.state.setConversationModelChoice).not.toHaveBeenCalled();
  });
});
