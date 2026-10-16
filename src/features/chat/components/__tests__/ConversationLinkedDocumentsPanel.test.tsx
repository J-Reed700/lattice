import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
import { useChatReaderStore } from '@/features/reading/stores/chatReaderStore';

const storeState = vi.hoisted(() => ({
  current: {} as Record<string, unknown>,
}));

const api = vi.hoisted(() => ({
  addDocumentsToLibrary: vi.fn(),
  ingestWebSource: vi.fn(),
  openDocumentFile: vi.fn(),
}));

vi.mock('@/features/chat/hooks/useConversationSourceFiling', () => ({
  useConversationSourceFiling: () => ({
    addDocumentsToLibrary: api.addDocumentsToLibrary,
    ingestWebSource: api.ingestWebSource,
  }),
}));

vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({
  openDocumentFile: api.openDocumentFile,
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: () => storeState.current,
}));

const emptyStore = () => ({
  spaces: [],
  conversations: [],
  lastMessageSources: new Map(),
  linkedDocumentsByConversationId: new Map(),
  webSourcesByConversationId: new Map(),
  documentSpaceMembershipsByDocumentId: new Map(),
  loadConversationLinkedDocuments: vi.fn(),
  loadConversationWebSources: vi.fn(),
  addConversationWebSource: vi.fn(),
  removeConversationWebSource: vi.fn(),
  removeConversationLinkedDocument: vi.fn(),
  loadDocumentSpaceMemberships: vi.fn(),
  setDocumentSpaceMembership: vi.fn(),
});

describe('ConversationLinkedDocumentsPanel — scope release', () => {
  beforeEach(() => {
    storeState.current = emptyStore();
  });

  it('names the space and offers the way out even with nothing staged or linked', async () => {
    // The narrowing outlives the staging row that used to announce it, so the
    // release has to stand on its own in the footer.
    const onMoveToGeneral = vi.fn();
    render(
      <ConversationLinkedDocumentsPanel
        conversationId="conv-1"
        scopedSpaceName="Movies"
        onMoveToGeneral={onMoveToGeneral}
      />
    );

    expect(
      screen.getByText(/Answers use only documents filed in Movies\./)
    ).toBeInTheDocument();
    // General is a space, not the whole vault: the way out must not promise
    // documents it cannot reach.
    expect(screen.queryByText(/whole vault/i)).not.toBeInTheDocument();
    // …and without an "always zero" count above it.
    expect(screen.queryByText(/Sources in this conversation/)).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Move this chat to General' }));
    expect(onMoveToGeneral).toHaveBeenCalledTimes(1);
  });

  it('says nothing about scope for a chat in General', () => {
    const { container } = render(
      <ConversationLinkedDocumentsPanel
        conversationId="conv-1"
        scopedSpaceName={null}
        onMoveToGeneral={vi.fn()}
      />
    );

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText(/Move this chat to General/)).not.toBeInTheDocument();
  });
});

describe('ConversationLinkedDocumentsPanel — attachments', () => {
  const linkedDocument = (overrides: Record<string, unknown> = {}) => ({
    documentId: 'doc-1',
    fileName: 'Greens growing list.txt',
    filePath: '/library/greens.txt',
    fileType: 'txt',
    category: 'Text',
    indexedAt: new Date().toISOString(),
    attachedToConversation: true,
    lastReferencedAt: new Date().toISOString(),
    referenceCount: 1,
    ...overrides,
  });

  const storeWithDocument = (document: Record<string, unknown>) => ({
    ...emptyStore(),
    linkedDocumentsByConversationId: new Map([['conv-1', [document]]]),
  });

  beforeEach(() => {
    api.addDocumentsToLibrary.mockReset();
    api.addDocumentsToLibrary.mockResolvedValue({ ok: true, data: { status: 'added' } });
    storeState.current = storeWithDocument(linkedDocument());
  });

  it('says an attached file is not in the library, and offers to file it', async () => {
    render(<ConversationLinkedDocumentsPanel conversationId="conv-1" />);

    await userEvent.click(screen.getByRole('button', { name: /Sources in this conversation/ }));

    expect(
      screen.getByText(/Attached to this chat\. Not in your library/)
    ).toBeInTheDocument();
    // The space checkboxes belong to library documents; an attachment is in no
    // space, so offering to assign one would be a control over nothing.
    expect(screen.queryByText(/^Scope:/)).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Add to library' }));
    expect(api.addDocumentsToLibrary).toHaveBeenCalledWith(['doc-1']);
  });

  it('leaves a library document its space controls and no filing offer', async () => {
    storeState.current = storeWithDocument(
      linkedDocument({ attachedToConversation: false })
    );

    render(<ConversationLinkedDocumentsPanel conversationId="conv-1" />);

    await userEvent.click(screen.getByRole('button', { name: /Sources in this conversation/ }));

    expect(screen.getByText(/Scope:/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Add to library' })).not.toBeInTheDocument();
    expect(screen.queryByText(/Attached to this chat/)).not.toBeInTheDocument();
  });
});

describe('ConversationLinkedDocumentsPanel — opening sources', () => {
  beforeEach(() => {
    useChatReaderStore.getState().close();
  });

  it('opens a linked document in the chat reader pane', async () => {
    storeState.current = {
      ...emptyStore(),
      linkedDocumentsByConversationId: new Map([
        [
          'conv-1',
          [
            {
              documentId: 'doc-1',
              fileName: 'Greens growing list.txt',
              filePath: '/library/greens.txt',
              fileType: 'txt',
              category: 'Text',
              indexedAt: new Date().toISOString(),
              attachedToConversation: false,
              lastReferencedAt: new Date().toISOString(),
              referenceCount: 1,
            },
          ],
        ],
      ]),
    };

    render(<ConversationLinkedDocumentsPanel conversationId="conv-1" />);
    await userEvent.click(screen.getByRole('button', { name: /Sources in this conversation/ }));

    await userEvent.click(screen.getByRole('button', { name: 'Open' }));

    const session = useChatReaderStore.getState().session;
    expect(session).not.toBeNull();
    expect(session?.ownerKey).toBe('linked-doc:conv-1');
    expect(session?.citations[0].documentId).toBe('doc-1');
    expect(session?.citations[0].fileName).toBe('Greens growing list.txt');
  });

  it('opens a cited web source in the chat reader pane', async () => {
    storeState.current = {
      ...emptyStore(),
      webSourcesByConversationId: new Map([
        [
          'conv-1',
          [
            {
              id: 'ws-1',
              conversationId: 'conv-1',
              url: 'https://example.com/guide',
              title: 'Guide to Greens',
              excerpt: 'An excerpt about greens.',
              createdAt: new Date().toISOString(),
            },
          ],
        ],
      ]),
    };

    render(<ConversationLinkedDocumentsPanel conversationId="conv-1" />);
    await userEvent.click(screen.getByRole('button', { name: /Sources in this conversation/ }));

    await userEvent.click(screen.getByRole('button', { name: 'Open URL' }));

    const session = useChatReaderStore.getState().session;
    expect(session).not.toBeNull();
    expect(session?.ownerKey).toBe('web-source:conv-1');
    expect(session?.citations[0].filePath).toBe('https://example.com/guide');
    expect(session?.citations[0].fileName).toBe('Guide to Greens');
  });
});
