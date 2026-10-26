import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';


import { TooltipProvider } from '@/components/ui/tooltip';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import VaultAPI from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { NEW_ITEM_EVENT } from '@/shared/newItemEvent';
import { deferred } from '@/tests/deferred';

import { ChatView } from './ChatView';

const h = vi.hoisted(() => ({
  store: {} as Record<string, unknown>,
  fetchDownloadedModels: vi.fn(),
  linkDocumentToSpace: vi.fn(),
  fetchDocumentFileName: vi.fn(),
  uiSetState: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn() },
  scrollToMessage: vi.fn(),
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ fetchDownloadedModels: h.fetchDownloadedModels }),
}));
vi.mock('@/features/chat/hooks/useDocumentLinkRequest', () => ({
  useLinkDocumentToSpaceMutation: () => ({ mutateAsync: h.linkDocumentToSpace }),
  fetchDocumentFileName: h.fetchDocumentFileName,
}));
vi.mock('@/features/chat/stores/conversationUiStore', () => ({ conversationUiStore: { setState: h.uiSetState } }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/utils/chatMessageNavigation', () => ({ scrollToMessage: h.scrollToMessage }));
vi.mock('@/utils/conversationTitles', () => ({ createDefaultConversationTitle: () => 'New chat' }));
vi.mock('@/features/chat/components/ChatPanel', () => ({ ChatPanel: () => <div data-testid="chat-panel" /> }));
vi.mock('@/features/chat/components/ConversationSidebar', () => ({
  ConversationSidebar: ({ onCollapse }: { onCollapse: () => void }) => (
    <div style={{ width: 0 }}><button type="button" onClick={onCollapse}>Collapse sidebar</button></div>
  ),
}));
vi.mock('@/features/chat/components/ConversationSpotlight', () => ({
  ConversationSpotlight: ({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) =>
    isOpen ? <div role="dialog" aria-label="Spotlight"><button type="button" onClick={onClose}>Close spotlight</button></div> : null,
}));
vi.mock('@/features/chat/components/reader/ChatReaderPane', () => ({
  ChatReaderPane: ({ rowWidth, availableWidth }: { rowWidth: number; availableWidth: number }) => (
    <div data-testid="reader" data-row={rowWidth} data-available={availableWidth} />
  ),
}));

function Location() {
  const location = useLocation();
  return <output data-testid="location">{location.search}</output>;
}

let queryClient: QueryClient;

const renderView = (url = '/chat') => {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <MemoryRouter initialEntries={[url]}>{children}<Location /></MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>
  );
  return render(<ChatView />, { wrapper });
};

const search = () => screen.getByTestId('location').textContent;

const makeStore = (overrides: Record<string, unknown> = {}) => ({
  loadConversations: vi.fn().mockResolvedValue(undefined),
  loadSpaces: vi.fn().mockResolvedValue(undefined),
  selectConversation: vi.fn().mockResolvedValue(undefined),
  createConversation: vi.fn().mockResolvedValue(undefined),
  activeConversationId: null,
  conversations: [],
  setComposerDraft: vi.fn(),
  loadConversationLinkedDocuments: vi.fn().mockResolvedValue(undefined),
  ...overrides,
});

describe('ChatView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient();
    h.store = makeStore();
    h.fetchDownloadedModels.mockResolvedValue(undefined);
    h.scrollToMessage.mockReturnValue(undefined);
    localStorage.removeItem('chat.sidebar.collapsed');
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('loads spaces and models before conversations', async () => {
    const spaces = deferred<void>();
    (h.store.loadSpaces as ReturnType<typeof vi.fn>).mockReturnValueOnce(spaces.promise);
    renderView();
    expect(h.store.loadConversations).not.toHaveBeenCalled();
    expect(h.fetchDownloadedModels).toHaveBeenCalled();
    spaces.resolve();
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledTimes(1));
  });

  it('collapses and restores the sidebar, remembering the choice', async () => {
    renderView();
    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }));
    expect(localStorage.getItem('chat.sidebar.collapsed')).toBe('1');
    fireEvent.click(screen.getByRole('button', { name: /Show sidebar/ }));
    expect(localStorage.getItem('chat.sidebar.collapsed')).toBe('0');
    expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toBeInTheDocument();
  });

  it('starts collapsed when that was the last choice, and toggles with Mod+\\', () => {
    localStorage.setItem('chat.sidebar.collapsed', '1');
    renderView();
    expect(screen.getByRole('button', { name: /Show sidebar/ })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: '\\', metaKey: true });
    expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: '\\', ctrlKey: true });
    expect(screen.getByRole('button', { name: /Show sidebar/ })).toBeInTheDocument();
    // Shift+Mod+\ is not the shortcut.
    fireEvent.keyDown(window, { key: '\\', metaKey: true, shiftKey: true });
    expect(screen.getByRole('button', { name: /Show sidebar/ })).toBeInTheDocument();
  });

  it('still renders when storage is blocked', () => {
    const getItem = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    renderView();
    expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toBeInTheDocument();
    getItem.mockRestore();
    setItem.mockRestore();
  });

  it('toggles the spotlight with Shift+Mod+K only', () => {
    renderView();
    fireEvent.keyDown(window, { key: 'k', metaKey: true });
    expect(screen.queryByRole('dialog', { name: 'Spotlight' })).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'K', metaKey: true, shiftKey: true });
    expect(screen.getByRole('dialog', { name: 'Spotlight' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close spotlight' }));
    expect(screen.queryByRole('dialog', { name: 'Spotlight' })).not.toBeInTheDocument();
  });

  it('starts one conversation per new-item event, ignoring repeats while it runs', async () => {
    const creating = deferred<void>();
    (h.store.createConversation as ReturnType<typeof vi.fn>).mockReturnValueOnce(creating.promise);
    renderView();
    act(() => {
      window.dispatchEvent(new Event(NEW_ITEM_EVENT));
      window.dispatchEvent(new Event(NEW_ITEM_EVENT));
    });
    expect(h.store.createConversation).toHaveBeenCalledTimes(1);
    expect(h.store.createConversation).toHaveBeenCalledWith('New chat');
    await act(async () => { creating.resolve(); await creating.promise; });
    act(() => { window.dispatchEvent(new Event(NEW_ITEM_EVENT)); });
    expect(h.store.createConversation).toHaveBeenCalledTimes(2);
  });

  it('survives a failed create and lets the next one through', async () => {
    (h.store.createConversation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('db busy'));
    localStorage.setItem('chat.sidebar.collapsed', '1');
    renderView();
    fireEvent.click(screen.getByRole('button', { name: /New conversation/ }));
    await waitFor(() => expect(h.store.createConversation).toHaveBeenCalledTimes(1));
    await act(async () => {});
    fireEvent.click(screen.getByRole('button', { name: /New conversation/ }));
    await waitFor(() => expect(h.store.createConversation).toHaveBeenCalledTimes(2));
  });

  it('starts a conversation from ?new=1 and drops the flag', async () => {
    renderView('/chat?new=1&tab=x');
    await waitFor(() => expect(h.store.createConversation).toHaveBeenCalledTimes(1));
    expect(search()).toBe('?tab=x');
  });

  it('quotes ?quote= into the composer, line by line', () => {
    renderView(`/chat?quote=${encodeURIComponent('The Nile floods\nevery summer')}`);
    expect(h.store.setComposerDraft).toHaveBeenCalledWith('> The Nile floods\n> every summer\n\n');
    expect(search()).toBe('');
  });

  it('ignores a blank quote and keeps a quote that cannot be decoded again', () => {
    const { unmount } = renderView('/chat?quote=%20%20');
    expect(h.store.setComposerDraft).not.toHaveBeenCalled();
    unmount();
    renderView(`/chat?quote=${encodeURIComponent('Yields rose 100%')}`);
    expect(h.store.setComposerDraft).toHaveBeenCalledWith('> Yields rose 100%\n\n');
  });

  it('quotes a passage containing a percent escape exactly as captured', () => {
    const passage = 'See https://example.org/field%20notes for the survey';
    renderView(`/chat?quote=${encodeURIComponent(passage)}`);
    expect(h.store.setComposerDraft).toHaveBeenCalledWith(`> ${passage}\n\n`);
  });

  it('caps a long quote', () => {
    renderView(`/chat?quote=${'a'.repeat(2500)}`);
    const draft = (h.store.setComposerDraft as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(draft).toBe(`> ${'a'.repeat(2000)}\n\n`);
  });

  describe('?documentId=', () => {
    it('links the document to the open conversation’s space and says it is scoped', async () => {
      h.store = makeStore({ activeConversationId: 'c1', conversations: [{ id: 'c1', spaceId: 's-bio' }] });
      h.linkDocumentToSpace.mockResolvedValueOnce({ ok: true, data: null });
      h.fetchDocumentFileName.mockResolvedValueOnce('cell-biology.pdf');
      renderView('/chat?documentId=d1');
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Scoped to cell-biology.pdf.'));
      expect(h.linkDocumentToSpace).toHaveBeenCalledWith({ documentId: 'd1', spaceId: 's-bio' });
      expect(h.store.loadConversationLinkedDocuments).toHaveBeenCalledWith('c1');
      expect(search()).toBe('');
    });

    it('asks about the document in General without claiming scope', async () => {
      h.store = makeStore({ activeConversationId: 'c1', conversations: [] });
      h.linkDocumentToSpace.mockResolvedValueOnce({ ok: true, data: null });
      h.fetchDocumentFileName.mockResolvedValueOnce('roman-roads.md');
      renderView('/chat?documentId=d1');
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Asking about roman-roads.md.'));
      expect(h.linkDocumentToSpace).toHaveBeenCalledWith({ documentId: 'd1', spaceId: GENERAL_SPACE_ID });
    });

    it('stays quiet when the file name cannot be read', async () => {
      h.store = makeStore({ activeConversationId: 'c1' });
      h.linkDocumentToSpace.mockResolvedValueOnce({ ok: true, data: null });
      h.fetchDocumentFileName.mockResolvedValueOnce(null);
      renderView('/chat?documentId=d1');
      await waitFor(() => expect(h.fetchDocumentFileName).toHaveBeenCalledWith('d1', VaultAPI));
      expect(h.toast.success).not.toHaveBeenCalled();
    });

    it('reports a refused link', async () => {
      h.store = makeStore({ activeConversationId: 'c1' });
      h.linkDocumentToSpace.mockResolvedValueOnce({ ok: false, error: 'document deleted' });
      renderView('/chat?documentId=d1');
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't attach that document", { message: 'document deleted' }));
      expect(h.store.loadConversationLinkedDocuments).not.toHaveBeenCalled();
    });

    it('waits for the new conversation instead of linking to the previous one', async () => {
      h.store = makeStore({ activeConversationId: 'old' });
      h.linkDocumentToSpace.mockResolvedValue({ ok: true, data: null });
      h.fetchDocumentFileName.mockResolvedValue(null);
      const view = renderView('/chat?new=1&documentId=d1');
      await waitFor(() => expect(h.store.createConversation).toHaveBeenCalledTimes(1));
      expect(h.linkDocumentToSpace).not.toHaveBeenCalled();
      expect(search()).toBe('');

      h.store = { ...h.store, activeConversationId: 'fresh', conversations: [{ id: 'fresh', spaceId: null }] };
      view.rerender(<ChatView />);
      await waitFor(() => expect(h.linkDocumentToSpace).toHaveBeenCalledWith({ documentId: 'd1', spaceId: GENERAL_SPACE_ID }));
      expect(h.store.createConversation).toHaveBeenCalledTimes(1);
    });

    it('waits for any conversation to open', async () => {
      h.linkDocumentToSpace.mockResolvedValue({ ok: true, data: null });
      const view = renderView('/chat?documentId=d1');
      expect(h.linkDocumentToSpace).not.toHaveBeenCalled();
      h.store = { ...h.store, activeConversationId: 'c7' };
      view.rerender(<ChatView />);
      await waitFor(() => expect(h.linkDocumentToSpace).toHaveBeenCalledTimes(1));
    });
  });

  describe('?conversationId=', () => {
    it('opens the requested conversation and then drops the parameter', async () => {
      const view = renderView('/chat?conversationId=c2');
      await waitFor(() => expect(h.store.selectConversation).toHaveBeenCalledWith('c2'));
      expect(search()).toBe('?conversationId=c2');
      h.store = { ...h.store, activeConversationId: 'c2' };
      view.rerender(<ChatView />);
      await waitFor(() => expect(search()).toBe(''));
    });

    it('redirects a tangent to its parent conversation', async () => {
      queryClient.setQueryData(conversationKeys.detail('t1'), { id: 't1', tangentParentId: 'p1' });
      renderView('/chat?conversationId=t1');
      await waitFor(() => expect(search()).toBe('?conversationId=p1'));
      await waitFor(() => expect(h.store.selectConversation).toHaveBeenCalledWith('p1'));
    });

    it('does nothing when the conversation is already open', () => {
      h.store = makeStore({ activeConversationId: 'c2' });
      renderView('/chat?conversationId=c2&messageId=m1');
      expect(h.store.selectConversation).not.toHaveBeenCalled();
    });

    it('scrolls to the requested message and consumes the deep link once settled', async () => {
      h.store = makeStore({ activeConversationId: 'c2' });
      renderView('/chat?conversationId=c2&messageId=m9&keep=1');
      expect(h.scrollToMessage).toHaveBeenCalledWith('m9', expect.objectContaining({ onSettled: expect.any(Function) }));
      expect(search()).toBe('?conversationId=c2&messageId=m9&keep=1');
      act(() => h.scrollToMessage.mock.calls[0][1].onSettled());
      await waitFor(() => expect(search()).toBe('?keep=1'));
    });
  });

  describe('Explorer threads', () => {
    it('moves off an Explorer thread to the newest Chat conversation', async () => {
      h.store = makeStore({
        activeConversationId: 'x1',
        conversations: [{ id: 'x1', explorerRoot: '/notes' }, { id: 'c1' }],
      });
      renderView();
      await waitFor(() => expect(h.store.selectConversation).toHaveBeenCalledWith('c1'));
    });

    it('clears the open conversation when only Explorer threads exist', () => {
      h.store = makeStore({ activeConversationId: 'x1', conversations: [{ id: 'x1', explorerRoot: '/notes' }] });
      renderView();
      expect(h.uiSetState).toHaveBeenCalledWith({ activeConversationId: null });
    });
  });

  it('measures the room the chat and reader share', () => {
    let callback: (() => void) | undefined;
    const disconnect = vi.fn();
    vi.stubGlobal('ResizeObserver', class {
      constructor(cb: () => void) { callback = cb; }
      observe() {}
      disconnect() { disconnect(); }
    });
    const { container, unmount } = renderView();
    const row = container.firstElementChild as HTMLElement;
    Object.defineProperty(row, 'clientWidth', { configurable: true, value: 1200 });
    (row.firstElementChild as HTMLElement).getBoundingClientRect = () => ({ width: 260 } as DOMRect);
    act(() => callback?.());
    expect(screen.getByTestId('reader')).toHaveAttribute('data-row', '1200');
    expect(screen.getByTestId('reader')).toHaveAttribute('data-available', '940');
    // Same size again keeps the same state.
    act(() => callback?.());
    expect(screen.getByTestId('reader')).toHaveAttribute('data-available', '940');
    unmount();
    expect(disconnect).toHaveBeenCalled();
  });
});
