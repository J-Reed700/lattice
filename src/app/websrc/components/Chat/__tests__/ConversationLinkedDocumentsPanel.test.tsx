import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationLinkedDocumentsPanel } from '../ConversationLinkedDocumentsPanel';

const storeState = vi.hoisted(() => ({
  current: {} as Record<string, unknown>,
}));

vi.mock('../../../stores/conversationsStore', () => ({
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

  it('offers the way back to the whole vault even with nothing staged or linked', async () => {
    // The narrowing outlives the staging row that used to announce it, so the
    // release has to stand on its own in the footer.
    const onSearchWholeVault = vi.fn();
    render(
      <ConversationLinkedDocumentsPanel
        conversationId="conv-1"
        isScopedToLinkedFiles
        onSearchWholeVault={onSearchWholeVault}
      />
    );

    expect(
      screen.getByText(/Answers use only this conversation's files\./)
    ).toBeInTheDocument();
    // …and without an "always zero" count above it.
    expect(screen.queryByText(/Sources in this conversation/)).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Search the whole vault' }));
    expect(onSearchWholeVault).toHaveBeenCalledTimes(1);
  });

  it('says nothing about scope when retrieval is already vault-wide', () => {
    const { container } = render(
      <ConversationLinkedDocumentsPanel
        conversationId="conv-1"
        isScopedToLinkedFiles={false}
        onSearchWholeVault={vi.fn()}
      />
    );

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText(/Search the whole vault/)).not.toBeInTheDocument();
  });
});
