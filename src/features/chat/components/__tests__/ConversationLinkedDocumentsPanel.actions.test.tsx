import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
import { deferred } from '@/tests/deferred';

const h = vi.hoisted(() => ({
  store: {} as Record<string, any>,
  filing: { addDocumentsToLibrary: vi.fn(), ingestWebSource: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('@/features/chat/hooks/useConversationSourceFiling', () => ({ useConversationSourceFiling: () => h.filing }));
vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({ openDocumentFile: vi.fn() }));
vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const doc = (id: string, fileName: string, overrides: Record<string, unknown> = {}) => ({
  documentId: id, fileName, referenceCount: 1, lastReferencedAt: new Date().toISOString(), attachedToConversation: false, ...overrides,
});

const makeStore = (overrides: Record<string, unknown> = {}) => ({
  spaces: [{ id: 's-hist', name: 'History', isArchived: false }, { id: 's-old', name: 'Old', isArchived: true }],
  conversations: [],
  lastMessageSources: new Map(),
  linkedDocumentsByConversationId: new Map(),
  webSourcesByConversationId: new Map(),
  documentSpaceMembershipsByDocumentId: new Map(),
  loadConversationLinkedDocuments: vi.fn().mockResolvedValue(undefined),
  loadConversationWebSources: vi.fn().mockResolvedValue(undefined),
  addConversationWebSource: vi.fn().mockResolvedValue(true),
  removeConversationWebSource: vi.fn().mockResolvedValue(true),
  removeConversationLinkedDocument: vi.fn().mockResolvedValue(undefined),
  loadDocumentSpaceMemberships: vi.fn().mockResolvedValue(undefined),
  setDocumentSpaceMembership: vi.fn().mockResolvedValue(undefined),
  ...overrides,
});

const expand = () => fireEvent.click(screen.getByRole('button', { name: /Sources in this conversation/ }));
const webSource = (id: string, url: string, title: string | null = null) => ({ id, url, normalizedUrl: null, title, excerpt: null });
const message = (id: string, role: string, sources: unknown[]) => ({ id, role, content: '', sources });

describe('ConversationLinkedDocumentsPanel actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.store = makeStore();
    h.filing.addDocumentsToLibrary.mockResolvedValue({ ok: true, data: null });
    h.filing.ingestWebSource.mockResolvedValue({ ok: true, data: null });
  });

  it('renders nothing with nothing linked and no scope to release, and loads what is linked', () => {
    const { container } = render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
    expect(container).toBeEmptyDOMElement();
    expect(h.store.loadConversationLinkedDocuments).toHaveBeenCalledWith('c1');
    expect(h.store.loadConversationWebSources).toHaveBeenCalledWith('c1');
  });

  describe('documents', () => {
    beforeEach(() => {
      h.store = makeStore({
        linkedDocumentsByConversationId: new Map([['c1', [
          doc('d1', 'Gibbon vol 1.pdf', { referenceCount: 3 }),
          doc('d2', 'scan.png', { attachedToConversation: true }),
        ]]]),
      });
    });

    it('counts references and toggles open', () => {
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expect(screen.getByRole('button', { name: /Sources in this conversation · 2/ })).toHaveAttribute('aria-expanded', 'false');
      expect(screen.queryByText('Gibbon vol 1.pdf')).not.toBeInTheDocument();
      expand();
      expect(screen.getByText(/^3 references · /)).toBeInTheDocument();
      expect(screen.getByText(/^1 reference · /)).toBeInTheDocument();
      expand();
      expect(screen.queryByText('Gibbon vol 1.pdf')).not.toBeInTheDocument();
    });

    it('removes a library document, or deletes an attachment', async () => {
      const removing = deferred<void>();
      h.store.removeConversationLinkedDocument.mockReturnValueOnce(removing.promise);
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
      expect(screen.getByRole('button', { name: 'Remove' })).toBeDisabled();
      await act(async () => { removing.resolve(); await removing.promise; });
      expect(h.store.removeConversationLinkedDocument).toHaveBeenCalledWith('c1', 'd1');
      expect(h.toast.success).toHaveBeenCalledWith('Removed linked document');
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Delete' })); });
      expect(h.toast.success).toHaveBeenLastCalledWith('Deleted attachment');
    });

    it('files an attachment into the library, or says why it could not', async () => {
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add to library' })); });
      expect(h.filing.addDocumentsToLibrary).toHaveBeenCalledWith(['d2']);
      expect(h.toast.success).toHaveBeenCalledWith('Added scan.png to the library', expect.anything());
      h.filing.addDocumentsToLibrary.mockResolvedValueOnce({ ok: false, error: 'disk full' });
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add to library' })); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't add this to the library", { message: 'disk full' });
    });

    it('loads a document’s spaces once, when its scope opens, and saves a change', async () => {
      const loading = deferred<void>();
      h.store.loadDocumentSpaceMemberships.mockReturnValueOnce(loading.promise);
      const { rerender } = render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      const details = screen.getByText('Scope: Assign spaces').closest('details')!;
      details.open = true;
      fireEvent(details, new Event('toggle'));
      expect(h.store.loadDocumentSpaceMemberships).toHaveBeenCalledWith('d1');
      expect(screen.getByText('Loading spaces...')).toBeInTheDocument();
      await act(async () => { loading.resolve(); await loading.promise; });
      expect(screen.queryByText('Loading spaces...')).not.toBeInTheDocument();
      expect(screen.getByText('Old (archived)')).toBeInTheDocument();

      h.store = { ...h.store, documentSpaceMembershipsByDocumentId: new Map([['d1', [{ spaceId: 's-hist' }]]]) };
      rerender(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expect(screen.getByText('Scope: 1 space')).toBeInTheDocument();
      // Once loaded, opening the scope again reads nothing more.
      const calls = h.store.loadDocumentSpaceMemberships.mock.calls.length;
      fireEvent(details, new Event('toggle'));
      expect(h.store.loadDocumentSpaceMemberships).toHaveBeenCalledTimes(calls);
      details.open = false;
      fireEvent(details, new Event('toggle'));

      const [history, old] = screen.getAllByRole('checkbox');
      expect(history).toBeChecked();
      await act(async () => { fireEvent.click(old); });
      expect(h.store.setDocumentSpaceMembership).toHaveBeenCalledWith('d1', 's-old', true);
    });

    it('counts several spaces in the plural', () => {
      h.store = { ...h.store, documentSpaceMembershipsByDocumentId: new Map([['d1', [{ spaceId: 's-hist' }, { spaceId: 's-old' }]]]) };
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      expect(screen.getByText('Scope: 2 spaces')).toBeInTheDocument();
    });

    it('says nothing is linked once everything is gone', () => {
      const { rerender } = render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      h.store = { ...h.store, linkedDocumentsByConversationId: new Map() };
      rerender(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expect(screen.getByText('Nothing linked. Sources cited in answers will appear here.')).toBeInTheDocument();
    });
  });

  describe('web sources', () => {
    beforeEach(() => {
      h.store = makeStore({
        webSourcesByConversationId: new Map([['c1', [webSource('w1', 'https://example.org/rome', 'Rome article'), webSource('w0', '   ')]]]),
        conversations: [{
          id: 'c1',
          spaceId: 's-hist',
          messages: [
            message('m1', 'user', [{ filePath: 'https://ignored.example.org' }]),
            message('m2', 'assistant', [
              { filePath: 'https://EXAMPLE.org/rome', fileName: 'duplicate' },
              { filePath: 'https://example.org/carthage', fileName: '  ' },
              { filePath: '/local/notes.md', fileName: 'local' },
            ]),
          ],
        }],
        lastMessageSources: new Map([['m2', [{ filePath: 'https://example.org/troy', fileName: 'Troy dig' }]]]),
      });
    });

    it('lists linked and cited web pages once each', () => {
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expect(screen.getByRole('button', { name: /Sources in this conversation · 3/ })).toBeInTheDocument();
      expand();
      expect(screen.getByText('Rome article')).toBeInTheDocument();
      expect(screen.getByText('Web source')).toBeInTheDocument();
      expect(screen.getByText('Troy dig')).toBeInTheDocument();
      expect(screen.queryByText('duplicate')).not.toBeInTheDocument();
      expect(screen.getAllByRole('button', { name: 'Remove link' })).toHaveLength(1);
      expect(screen.getAllByRole('button', { name: 'Link' })).toHaveLength(2);
    });

    it('links a cited page and unlinks a linked one', async () => {
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getAllByRole('button', { name: 'Link' })[1]); });
      expect(h.store.addConversationWebSource).toHaveBeenCalledWith('c1', 'https://example.org/troy', { title: 'Troy dig' });
      expect(h.toast.success).toHaveBeenCalledWith('Linked source');
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Remove link' })); });
      expect(h.store.removeConversationWebSource).toHaveBeenCalledWith('c1', 'w1');
      expect(h.toast.success).toHaveBeenCalledWith('Removed source link');
    });

    it('stays quiet when linking or unlinking is refused', async () => {
      h.store.addConversationWebSource.mockResolvedValueOnce(false);
      h.store.removeConversationWebSource.mockResolvedValueOnce(false);
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getAllByRole('button', { name: 'Link' })[0]); });
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Remove link' })); });
      expect(h.toast.success).not.toHaveBeenCalled();
    });

    it('indexes one page into the conversation’s space, then hides it', async () => {
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getAllByRole('button', { name: 'Index' })[0]); });
      expect(h.filing.ingestWebSource).toHaveBeenCalledWith({ url: 'https://example.org/rome', spaceId: 's-hist', skipRefresh: undefined });
      expect(screen.queryByText('Rome article')).not.toBeInTheDocument();
    });

    it('reports a page that could not be indexed and keeps it', async () => {
      h.filing.ingestWebSource.mockResolvedValueOnce({ ok: false, error: 'HTTP 403' });
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getAllByRole('button', { name: 'Index' })[0]); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't index Rome article", { message: 'HTTP 403' });
      expect(screen.getByText('Rome article')).toBeInTheDocument();
    });

    it('indexes everything pending, counts the successes, and refreshes once', async () => {
      h.filing.ingestWebSource
        .mockResolvedValueOnce({ ok: true, data: null })
        .mockResolvedValueOnce({ ok: false, error: 'timeout' })
        .mockResolvedValueOnce({ ok: true, data: null });
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Index all' })); });
      expect(h.filing.ingestWebSource).toHaveBeenCalledTimes(3);
      expect(h.filing.ingestWebSource).toHaveBeenCalledWith(expect.objectContaining({ skipRefresh: true }));
      expect(h.store.loadConversationLinkedDocuments).toHaveBeenCalledTimes(2);
      expect(h.toast.success).toHaveBeenCalledWith('Indexed 2 sources');
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Index all' })); });
      expect(h.toast.success).toHaveBeenLastCalledWith('Indexed 1 source');
      expect(screen.getByText('Nothing pending.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Index all' })).not.toBeInTheDocument();
    });

    it('says nothing when every page fails to index', async () => {
      h.filing.ingestWebSource.mockResolvedValue({ ok: false, error: 'offline' });
      render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Index all' })); });
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledTimes(3));
      expect(h.toast.success).not.toHaveBeenCalled();
    });

    it('forgets what was indexed when the conversation changes', async () => {
      const { rerender } = render(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expand();
      await act(async () => { fireEvent.click(screen.getAllByRole('button', { name: 'Index' })[0]); });
      rerender(<ConversationLinkedDocumentsPanel conversationId="c2" />);
      h.store = { ...h.store, webSourcesByConversationId: new Map([['c1', [webSource('w1', 'https://example.org/rome', 'Rome article')]]]) };
      rerender(<ConversationLinkedDocumentsPanel conversationId="c1" />);
      expect(await screen.findByText('Rome article')).toBeInTheDocument();
    });
  });
});
