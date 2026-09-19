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
