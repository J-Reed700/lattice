import { useState } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { DEFAULT_TANGENT_VIEW, type TangentListView } from './tangentGroups';
import { TangentList } from './TangentList';

const renameConversation = vi.fn(async () => true);
vi.mock('@/lib/api', () => ({ VaultAPI: {} }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: object) => unknown) => select({ renameConversation, deleteConversation: vi.fn(), selectConversation: vi.fn() }),
}));

const tangentFixture = (id: string, overrides: Partial<ConversationTangentDto> = {}): ConversationTangentDto => ({
  conversationId: id, parentConversationId: 'parent', sourceConversationId: 'parent', sourceMessageId: 'a1',
  selectedText: `Passage of ${id}`, title: `Title ${id}`, createdAt: '2026-10-01T10:00:00Z', updatedAt: '2026-10-01T10:00:00Z',
  contextMessageCount: 2, ...overrides,
});

const messages = [
  { id: 'u1', role: 'user', content: 'First question' },
  { id: 'a1', role: 'assistant', content: 'The **first** reply about photosynthesis.' },
  { id: 'u2', role: 'user', content: 'Second question' },
  { id: 'a2', role: 'assistant', content: 'The second reply about orbits.' },
];

const tangents = [
  tangentFixture('late-reply', { sourceMessageId: 'a2', title: 'Kepler', selectedText: 'elliptical orbits', createdAt: '2026-10-01T09:00:00Z', updatedAt: '2026-10-03T00:00:00Z' }),
  tangentFixture('early-reply', { sourceMessageId: 'a1', title: 'Chlorophyll', selectedText: 'light absorption', createdAt: '2026-10-02T09:00:00Z', updatedAt: '2026-10-02T00:00:00Z' }),
  tangentFixture('nested', { sourceConversationId: 'early-reply', sourceMessageId: 'tangent-reply', title: 'Quantum yield', selectedText: 'photons per molecule', updatedAt: '2026-10-04T00:00:00Z' }),
];

function renderList(initial: Partial<TangentListView> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(conversationKeys.messages('parent'), messages);
  const onOpen = vi.fn();
  const onShowInChat = vi.fn();
  function Harness() {
    const [view, setView] = useState<TangentListView>({ ...DEFAULT_TANGENT_VIEW, ...initial });
    return <TangentList conversationId="parent" tangents={tangents} drafts={new Map()} view={view} onViewChange={setView} onOpen={onOpen} onShowInChat={onShowInChat} />;
  }
  render(<QueryClientProvider client={client}><TooltipProvider><Harness /></TooltipProvider></QueryClientProvider>);
  return { onOpen, onShowInChat };
}

/** Each row's open button carries its last-activity time. */
const rowTitles = () => screen.queryAllByRole('button').filter(button => button.querySelector('time'))
  .map(button => button.querySelector('span')?.textContent);

describe('TangentList', () => {
  beforeEach(() => renameConversation.mockClear());

  it('groups by source reply in conversation order and nests tangents opened inside a tangent', () => {
    const { onShowInChat } = renderList();
    const groups = screen.getAllByRole('region', { name: 'Tangents from one reply' });
    expect(groups).toHaveLength(2);
    expect(within(groups[0]).getByText('The first reply about photosynthesis.')).toBeInTheDocument();
    expect(within(groups[1]).getByText('Kepler')).toBeInTheDocument();
    const nested = within(groups[0]).getByRole('list', { name: 'Tangents opened in Chlorophyll' });
    expect(within(nested).getByText('Quantum yield')).toBeInTheDocument();
    fireEvent.click(within(groups[1]).getByRole('button', { name: 'Show in chat' }));
    expect(onShowInChat).toHaveBeenCalledWith('a2');
  });

  it('filters on title and passage, keeps a matching tangent’s parent, and shows an empty state', () => {
    renderList();
    const filter = screen.getByRole('searchbox', { name: 'Filter tangents' });
    fireEvent.change(filter, { target: { value: 'ELLIPTICAL' } });
    expect(rowTitles()).toEqual(['Kepler']);
    fireEvent.change(filter, { target: { value: 'photons' } });
    expect(rowTitles()).toEqual(['Chlorophyll', 'Quantum yield']);
    fireEvent.change(filter, { target: { value: 'nothing like this' } });
    expect(screen.getByRole('status')).toHaveTextContent('No tangents match “nothing like this”.');
    fireEvent.keyDown(filter, { key: 'Escape' });
    expect(filter).toHaveValue('');
    expect(rowTitles()).toHaveLength(3);
  });

  it('switches to a flat list ordered by recent activity', () => {
    renderList();
    fireEvent.click(screen.getByRole('button', { name: 'Recent activity' }));
    expect(screen.getByRole('button', { name: 'Recent activity' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('region')).not.toBeInTheDocument();
    expect(rowTitles()).toEqual(['Quantum yield', 'Kepler', 'Chlorophyll']);
  });

  it('shows only one reply’s tangents when focused on it', () => {
    renderList({ focusMessageId: 'a2' });
    expect(rowTitles()).toEqual(['Kepler']);
    fireEvent.click(screen.getByRole('button', { name: 'Show all' }));
    expect(rowTitles()).toHaveLength(3);
  });

  it('renames a tangent inline', async () => {
    renderList();
    fireEvent.click(screen.getByRole('button', { name: 'Actions for Kepler' }));
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Rename' }));
    const input = screen.getByRole('textbox', { name: 'Tangent title' });
    fireEvent.change(input, { target: { value: 'Kepler’s laws' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(renameConversation).toHaveBeenCalledWith('late-reply', 'Kepler’s laws');
  });
});
