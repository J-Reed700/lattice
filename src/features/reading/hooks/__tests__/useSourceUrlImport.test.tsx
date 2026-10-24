import type { ReactNode } from 'react';

import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import { deferred } from '@/tests/deferred';

import { useSourceUrlImport } from '../useSourceUrlImport';

const api = vi.hoisted(() => ({ ingestWebUrl: vi.fn(), openFileById: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const URL_TO_IMPORT = 'https://example.org/medieval-guilds';
const spaces = [{ id: 'history', name: 'History' }, { id: 'cooking', name: 'Cooking' }];

function setup(url: string | null, conversationId: string | null, overrides: Partial<{ selectedSpaceId: string | null; spaces: typeof spaces; conversations: { id: string; spaceId: string | null }[] }> = {}) {
  const store = {
    conversations: [{ id: 'chat-1', spaceId: 'cooking' }, { id: 'chat-2', spaceId: null }],
    spaces, selectedSpaceId: null, loadConversationLinkedDocuments: vi.fn().mockResolvedValue(undefined), ...overrides,
  };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ConversationSnapshotProvider value={store as unknown as ConversationsState}>{children}</ConversationSnapshotProvider>
  );
  return { store, ...renderHook(() => useSourceUrlImport(url, conversationId), { wrapper }) };
}

beforeEach(() => {
  api.ingestWebUrl.mockReset().mockResolvedValue({ ok: true, data: { title: 'Medieval guilds', wordCount: 12500, documentId: 'doc-9' } });
  api.openFileById.mockReset().mockResolvedValue({ ok: true, data: null });
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
});
afterEach(() => vi.restoreAllMocks());

describe('useSourceUrlImport', () => {
  it('imports into the open conversation’s space and refreshes its linked documents', async () => {
    const { result, store } = setup(URL_TO_IMPORT, 'chat-1');
    expect(result.current.targetImportSpaceId).toBe('cooking');
    expect(result.current.allowUnscopedImport).toBe(false);
    await act(() => result.current.handleImportSourceUrl());
    expect(api.ingestWebUrl).toHaveBeenCalledWith(URL_TO_IMPORT, { spaceId: 'cooking', conversationId: 'chat-1' });
    expect(toast.success).toHaveBeenCalledWith('Source imported', expect.objectContaining({
      message: `Medieval guilds (${(12500).toLocaleString()} words) · Cooking`,
    }));
    expect(store.loadConversationLinkedDocuments).toHaveBeenCalledWith('chat-1');
    expect(result.current.isImportingUrl).toBe(false);
  });

  it('defaults to the selected space, or the first space for a chat with none', () => {
    expect(setup(URL_TO_IMPORT, null, { selectedSpaceId: 'history' }).result.current.targetImportSpaceId).toBe('history');
    expect(setup(URL_TO_IMPORT, 'chat-2').result.current.targetImportSpaceId).toBe('history');
    const unscoped = setup(URL_TO_IMPORT, null);
    expect(unscoped.result.current.targetImportSpaceId).toBe('');
    expect(unscoped.result.current.allowUnscopedImport).toBe(true);
    expect(setup(URL_TO_IMPORT, 'chat-2', { spaces: [] }).result.current.targetImportSpaceId).toBe('');
  });

  it('imports without a space or conversation when none applies', async () => {
    const { result, store } = setup(URL_TO_IMPORT, null);
    await act(() => result.current.handleImportSourceUrl());
    expect(api.ingestWebUrl).toHaveBeenCalledWith(URL_TO_IMPORT, { spaceId: undefined, conversationId: undefined });
    expect(toast.success).toHaveBeenCalledWith('Source imported', expect.objectContaining({ message: `Medieval guilds (${(12500).toLocaleString()} words)` }));
    expect(store.loadConversationLinkedDocuments).not.toHaveBeenCalled();
  });

  it('uses a space the user picked', async () => {
    const { result } = setup(URL_TO_IMPORT, 'chat-1');
    act(() => result.current.setTargetImportSpaceId('history'));
    await act(() => result.current.handleImportSourceUrl());
    expect(api.ingestWebUrl).toHaveBeenCalledWith(URL_TO_IMPORT, { spaceId: 'history', conversationId: 'chat-1' });
  });

  it('does nothing without a URL, and ignores a second click while importing', async () => {
    setup(null, 'chat-1').result.current.handleImportSourceUrl();
    expect(api.ingestWebUrl).not.toHaveBeenCalled();

    const pending = deferred<{ ok: true; data: { title: string; wordCount: number; documentId: string } }>();
    api.ingestWebUrl.mockReturnValue(pending.promise);
    const { result } = setup(URL_TO_IMPORT, 'chat-1');
    act(() => { void result.current.handleImportSourceUrl(); });
    await waitFor(() => expect(result.current.isImportingUrl).toBe(true));
    await act(() => result.current.handleImportSourceUrl());
    expect(api.ingestWebUrl).toHaveBeenCalledTimes(1);
    await act(async () => { pending.resolve({ ok: true, data: { title: 'Guilds', wordCount: 1, documentId: 'd' } }); await pending.promise; });
    expect(result.current.isImportingUrl).toBe(false);
  });

  it('says why an import failed and clears the busy state', async () => {
    api.ingestWebUrl.mockResolvedValue({ ok: false, error: 'page returned 404' });
    const { result } = setup(URL_TO_IMPORT, 'chat-1');
    await act(() => result.current.handleImportSourceUrl());
    expect(toast.error).toHaveBeenCalledWith("Couldn't import URL", { message: 'page returned 404' });
    expect(toast.success).not.toHaveBeenCalled();
    expect(result.current.isImportingUrl).toBe(false);
  });

  it('opens the imported document from the toast, reporting a failure to open', async () => {
    const { result } = setup(URL_TO_IMPORT, 'chat-1');
    await act(() => result.current.handleImportSourceUrl());
    const { action } = vi.mocked(toast.success).mock.calls[0][1]!;
    action!.onClick();
    await waitFor(() => expect(api.openFileById).toHaveBeenCalledWith('doc-9'));
    expect(toast.error).not.toHaveBeenCalled();

    api.openFileById.mockResolvedValue({ ok: false, error: 'no viewer' });
    action!.onClick();
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Saved, but couldn't open", { message: 'no viewer' }));
  });
});
