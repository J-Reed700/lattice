import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ToolPreferences } from '@/types';

import { ConversationTangents } from './ConversationTangents';
import { TangentReplyAction } from './TangentReplyAction';

vi.mock('@/lib/api', () => ({ VaultAPI: {} }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: object) => unknown) => select({ renameConversation: vi.fn(), deleteConversation: vi.fn(), selectConversation: vi.fn() }),
}));

const tangent = (id: string, sourceMessageId: string, selectedText: string): ConversationTangentDto => ({
  conversationId: id, parentConversationId: 'parent', sourceConversationId: 'parent', sourceMessageId, selectedText,
  title: `Tangent ${id}`, createdAt: '2026-10-01T10:00:00Z', updatedAt: '2026-10-01T10:00:00Z', contextMessageCount: 2,
});

function renderChat() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(conversationKeys.messages('parent'), [
    { id: 'a1', role: 'assistant', content: 'First reply' },
    { id: 'a2', role: 'assistant', content: 'Second reply, explored as a whole' },
  ]);
  client.setQueryData(conversationKeys.tangents('parent'), [
    tangent('whole', 'a2', 'Second reply, explored as a whole'),
    tangent('part', 'a2', 'explored'),
    tangent('other', 'a1', 'First'),
  ]);
  render(<QueryClientProvider client={client}><TooltipProvider>
    <ConversationTangents conversationId="parent" toolPreferences={{} as ToolPreferences} unavailable={false}>
      <TangentReplyAction source={{ conversationId: 'parent', messageId: 'a2', getText: () => 'Second reply' }} className="" disabled={false} />
    </ConversationTangents>
  </TooltipProvider></QueryClientProvider>);
}

describe('ConversationTangents', () => {
  afterEach(() => conversationUiStore.setState({ requestedTangent: null, requestedTangentReply: null }));

  it('opens the sidebar on one reply’s tangents from its marker, and the toggle shows them all again', () => {
    renderChat();
    const marker = screen.getByRole('button', { name: 'Show the 2 tangents from this reply' });
    expect(marker).toHaveTextContent('2 tangents');
    fireEvent.click(marker);
    const sidebar = screen.getByRole('complementary', { name: 'Tangents' });
    expect(within(sidebar).getByText('Showing one reply’s tangents')).toBeInTheDocument();
    expect(within(sidebar).getByText('Tangent whole')).toBeInTheDocument();
    expect(within(sidebar).getByText('Tangent part')).toBeInTheDocument();
    expect(within(sidebar).queryByText('Tangent other')).not.toBeInTheDocument();

    const toggle = screen.getByRole('button', { name: /^Tangents/ });
    fireEvent.click(toggle);
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    fireEvent.click(toggle);
    expect(within(screen.getByRole('complementary', { name: 'Tangents' })).getByText('Tangent other')).toBeInTheDocument();
  });

  it('ignores a reply request for another conversation', () => {
    renderChat();
    act(() => conversationUiStore.setState({ requestedTangentReply: { parentId: 'elsewhere', messageId: 'a2' } }));
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
  });
});
