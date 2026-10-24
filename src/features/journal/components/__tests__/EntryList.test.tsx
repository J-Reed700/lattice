import type { ComponentProps } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { EntryList } from '@/features/journal/components/EntryList';
import type { JournalEntrySummary, UseJournalEntriesResult } from '@/features/journal/hooks/useJournalEntries';
import { spaceKeys } from '@/features/spaces/api/queries';
import type { ConversationSpaceDto } from '@/types';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

const entry = (id: string, title: string, updatedAt: string): JournalEntrySummary => ({ id, title, updatedAt, messageCount: 2 });

const page = (id: string, title: string): WorkspaceNote => ({
  id, title, content: '', linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [],
  conversationSnapshots: [], createdAt: '2026-10-01T09:00:00.000Z', updatedAt: '2026-10-20T09:00:00.000Z',
} as unknown as WorkspaceNote);

const space = (id: string, name: string): ConversationSpaceDto => ({ id, name, isArchived: false } as ConversationSpaceDto);

function entriesState(overrides: Partial<UseJournalEntriesResult> = {}): UseJournalEntriesResult {
  return {
    entries: [], isLoading: false, loadError: null, search: '', setSearch: vi.fn(), filter: 'all', setFilter: vi.fn(),
    pinnedIds: new Set(), togglePinned: vi.fn(), selectedId: null, setSelectedId: vi.fn(), selectedMessages: [],
    selectedMessagesLoading: false, reload: vi.fn(), removeEntry: vi.fn(), ...overrides,
  };
}

function renderList(overrides: Partial<ComponentProps<typeof EntryList>> = {}, spaces: ConversationSpaceDto[] = [space('general', 'General')]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(spaceKeys.list, spaces);
  const props: ComponentProps<typeof EntryList> = {
    entriesState: entriesState(), journals: [{ id: 'j1', name: 'Kitchen notes' } as never], currentJournal: { id: 'j1', name: 'Kitchen notes' } as never,
    onSwitchJournal: vi.fn(), onCreateJournal: vi.fn(), onRenameJournal: vi.fn(), onDeleteJournal: vi.fn(), onNewEntry: vi.fn(),
    onRenameEntry: vi.fn(), onDeleteEntry: vi.fn(), onToggleCollapse: vi.fn(), pages: [page('p1', 'Sourdough')], activePageId: 'p1',
    onSelectPage: vi.fn(), onNewPage: vi.fn(), onRenamePage: vi.fn(), onDeletePage: vi.fn(), displayPageTitle: (p) => p.title,
    ...overrides,
  };
  render(<QueryClientProvider client={client}><TooltipProvider><EntryList {...props} /></TooltipProvider></QueryClientProvider>);
  return props;
}

const row = (title: string) => screen.getByText(title).closest<HTMLElement>('[role="button"]')!;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(2026, 9, 24, 12, 0));
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => vi.useRealTimers());

describe('EntryList', () => {
  it('groups conversations by recency with pinned first and undated last', () => {
    const local = (m: number, d: number) => new Date(2026, m, d, 9).toISOString();
    renderList({
      entriesState: entriesState({
        pinnedIds: new Set(['pinned']),
        entries: [
          entry('aug', 'Harvest calendar', local(7, 5)),
          entry('undated', 'Mystery stew', 'not a date'),
          entry('today', 'Roman roads', local(9, 24)),
          entry('sep', 'Bread starters', local(8, 10)),
          entry('pinned', 'Cell membranes', local(9, 24)),
          entry('yesterday', 'Photosynthesis', local(9, 23)),
          entry('week', 'Fermentation', local(9, 20)),
          entry('month', 'Spice routes', local(9, 2)),
        ],
      }),
    });
    const labels = screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent);
    expect(labels).toEqual(['Pages1', 'Conversations8', 'Pinned', 'Today', 'Yesterday', 'This week', 'This month', 'Sep 2026', 'Aug 2026', 'Earlier']);
    expect(within(row('Cell membranes')).getByLabelText('Pinned')).toBeInTheDocument();
    expect(screen.getByText('1 page')).toBeInTheDocument();
  });

  it('selects, pins and deletes a conversation from its row', async () => {
    const user = userEvent.setup();
    const state = entriesState({ entries: [entry('c1', 'Roman roads', new Date().toISOString())], selectedId: 'c1' });
    const props = renderList({ entriesState: state });
    expect(row('Roman roads')).toHaveAttribute('aria-current', 'page');

    await user.click(row('Roman roads'));
    expect(state.setSelectedId).toHaveBeenCalledWith('c1');
    row('Roman roads').focus();
    await user.keyboard(' ');
    expect(state.setSelectedId).toHaveBeenCalledTimes(2);

    await user.hover(row('Roman roads'));
    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Pin entry' }));
    expect(state.togglePinned).toHaveBeenCalledWith('c1');
    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Delete entry' }));
    expect(props.onDeleteEntry).toHaveBeenCalledWith('c1');
    // Row actions never also select the row.
    expect(state.setSelectedId).toHaveBeenCalledTimes(2);
  });

  it('renames a conversation only when the trimmed title changed', async () => {
    const user = userEvent.setup();
    const props = renderList({ entriesState: entriesState({ entries: [entry('c1', 'Roman roads', new Date().toISOString())] }) });

    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Rename entry' }));
    const input = screen.getByDisplayValue('Roman roads');
    await user.click(input);
    await user.keyboard('{Enter}');
    expect(props.onRenameEntry).not.toHaveBeenCalled();

    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Rename entry' }));
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('textbox', { name: '' })).not.toBeInTheDocument();

    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Rename entry' }));
    await user.clear(screen.getByDisplayValue('Roman roads'));
    await user.keyboard('  Roads of Rome  {Enter}');
    expect(props.onRenameEntry).toHaveBeenCalledWith('c1', 'Roads of Rome');
    await waitFor(() => expect(screen.queryByDisplayValue(/Roads of Rome/)).not.toBeInTheDocument());
  });

  it('commits a rename when the field loses focus', async () => {
    const user = userEvent.setup();
    const props = renderList({ entriesState: entriesState({ entries: [entry('c1', 'Roman roads', new Date().toISOString())] }) });
    await user.click(within(row('Roman roads')).getByRole('button', { name: 'Rename entry' }));
    await user.type(screen.getByDisplayValue('Roman roads'), ' east');
    await user.click(screen.getByRole('button', { name: 'New page' }));
    expect(props.onRenameEntry).toHaveBeenCalledWith('c1', 'Roman roads east');
    expect(props.onNewPage).toHaveBeenCalled();
  });

  it('searches and filters once there are conversations', async () => {
    const user = userEvent.setup();
    const state = entriesState({ entries: [entry('c1', 'Roman roads', new Date().toISOString())] });
    renderList({ entriesState: state });
    await user.type(screen.getByLabelText('Search conversations'), 'r');
    expect(state.setSearch).toHaveBeenCalledWith('r');
    await user.click(screen.getByRole('tab', { name: 'Pinned' }));
    expect(state.setFilter).toHaveBeenCalledWith('pinned');
  });

  it('shows loading and load errors instead of the list', () => {
    renderList({ entriesState: entriesState({ isLoading: true }) });
    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  it('shows the load error', () => {
    renderList({ entriesState: entriesState({ loadError: 'Journal could not be read' }) });
    expect(screen.getByText('Journal could not be read')).toBeInTheDocument();
  });

  it('offers to clear a search that matched nothing', async () => {
    const user = userEvent.setup();
    const state = entriesState({ search: 'saffron', filter: 'today' });
    renderList({ entriesState: state, pages: [page('p1', 'A'), page('p2', 'B')] });
    expect(screen.getByText('No matching conversations')).toBeInTheDocument();
    expect(screen.getByText('2 pages')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(state.setSearch).toHaveBeenCalledWith('');
    expect(state.setFilter).toHaveBeenCalledWith('all');
  });

  it('starts a chat straight away when there is only one space', async () => {
    const user = userEvent.setup();
    const props = renderList();
    expect(screen.getByText('No conversations yet')).toBeInTheDocument();
    expect(screen.queryByLabelText('Search conversations')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Ask in Chat' }));
    expect(props.onNewEntry).toHaveBeenCalledWith();
  });

  it('asks which space to search when there are several', async () => {
    const user = userEvent.setup();
    const props = renderList({}, [space('general', 'General'), space('garden', 'Garden'), { ...space('old', 'Old'), isArchived: true }]);
    await user.click(screen.getByRole('button', { name: 'Ask something in Chat, filed under this journal' }));
    expect(await screen.findByText('Which documents should it search?')).toBeInTheDocument();
    expect(screen.queryByText('Old')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Garden/ }));
    expect(props.onNewEntry).toHaveBeenCalledWith('garden');
  });

  it('hides the sidebar', async () => {
    const user = userEvent.setup();
    const props = renderList();
    await user.click(screen.getByRole('button', { name: /Hide sidebar/ }));
    expect(props.onToggleCollapse).toHaveBeenCalled();
  });

  it('jumps from the calendar to the first conversation of a day, or the nearest day', async () => {
    const user = userEvent.setup();
    const state = entriesState({
      entries: [
        entry('later', 'Afternoon tea', new Date(2026, 9, 20, 15).toISOString()),
        entry('first', 'Morning bread', new Date(2026, 9, 20, 8).toISOString()),
        entry('far', 'Harvest', new Date(2026, 9, 2, 8).toISOString()),
        entry('bad', 'Undated', 'never'),
      ],
    });
    renderList({ entriesState: state });

    await user.click(screen.getByRole('button', { name: 'Open date picker' }));
    await user.click(await screen.findByRole('button', { name: /October 20th, 2026/ }));
    expect(state.setSelectedId).toHaveBeenLastCalledWith('first');
    await waitFor(() => expect(screen.queryByRole('grid')).not.toBeInTheDocument());

    await user.click(screen.getByRole('button', { name: 'Open date picker' }));
    await user.click(await screen.findByRole('button', { name: /October 18th, 2026/ }));
    expect(state.setSelectedId).toHaveBeenLastCalledWith('first');
  });

  it('keeps the calendar open when there is nothing to jump to', async () => {
    const user = userEvent.setup();
    const state = entriesState();
    renderList({ entriesState: state });
    await user.click(screen.getByRole('button', { name: 'Open date picker' }));
    await user.click(await screen.findByRole('button', { name: /October 18th, 2026/ }));
    expect(state.setSelectedId).not.toHaveBeenCalled();
    expect(screen.getByRole('grid')).toBeInTheDocument();
  });
});
