import type { ReactNode } from 'react';

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { ExplorerThreads } from '@/features/explorer/hooks/useExplorerThread';
import type { Conversation } from '@/types/conversation';

import { ExplorerChat } from '../ExplorerChat';
import { groupThreads } from '../ExplorerThreadList';

const mocks = vi.hoisted(() => ({
  store: {
    renameConversation: vi.fn(),
    setConversationSaved: vi.fn(),
    setConversationPinned: vi.fn(),
    setConversationArchived: vi.fn(),
  },
}));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: typeof mocks.store) => unknown) => select(mocks.store),
}));
vi.mock('@/features/chat/components/sidebar/useConversationSynthesis', () => ({
  useConversationSynthesis: () => ({
    synthesizeConversationToJournal: vi.fn(),
    synthesizingConversationId: null,
    isSynthesizing: false,
    continueConversationInNewChat: vi.fn(),
    continuingConversationId: null,
  }),
}));
vi.mock('@/features/chat/hooks/useConversationExport', () => ({
  useConversationExport: () => ({
    copyConversationAsMarkdown: vi.fn(),
    saveConversationToJournal: vi.fn(),
    copyingConversationId: null,
    savingConversationId: null,
  }),
}));
vi.mock('@/features/learning/recall/flashcards/useFlashcards', () => ({
  useGenerateConversationFlashcardDeck: () => ({ isPending: false, variables: undefined, mutate: vi.fn() }),
}));


vi.mock('@/features/chat/components/ChatPanel', () => ({
  ChatPanel: ({ composerNotice, composerChips }: { composerNotice?: ReactNode; composerChips?: ReactNode }) => (
    <div>
      Conversation messages
      <div data-testid="composer-notice">{composerNotice}</div>
      <div data-testid="composer-chips">{composerChips}</div>
    </div>
  ),
}));
vi.mock('../ExplorerIndexNotice', () => ({ ExplorerIndexNotice: () => <p>Folder index notice</p> }));
vi.mock('../ExplorerSelectionChip', () => ({ ExplorerSelectionChip: () => <span>Selected lines chip</span> }));

function threads(remove = vi.fn().mockResolvedValue(true)): ExplorerThreads {
  return {
    threads: [{ id: 'thread-1', title: 'Project questions', updatedAt: '2026-10-08T12:00:00Z' }],
    archived: [],
    activeId: 'thread-1',
    preparing: false,
    error: null,
    select: vi.fn(),
    create: vi.fn(),
    remove,
    retry: vi.fn(),
  };
}

function show(value: ExplorerThreads) {
  return render(<TooltipProvider><ExplorerChat threads={value} /></TooltipProvider>);
}

const DAY = 86_400_000;
const ago = (days: number) => new Date(Date.now() - days * DAY).toISOString();
const thread = (id: string, title: string, updatedAt: string, extra: Partial<Conversation> = {}): Conversation =>
  ({ id, title, updatedAt, explorerRoot: '/project', ...extra });

beforeEach(() => {
  vi.clearAllMocks();
  mocks.store.renameConversation.mockResolvedValue(true);
});

describe('ExplorerChat header menu', () => {
  it('offers the same actions as the Chat sidebar', async () => {
    const user = userEvent.setup();
    show(threads());

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    const menu = screen.getByRole('dialog', { name: 'Actions for Project questions' });
    for (const name of ['Copy as Markdown', 'Save to journal', 'Create flashcards', 'Synthesize to journal', 'Continue in new chat', 'Rename', 'Save conversation', 'Pin', 'Archive', 'Delete conversation…']) {
      expect(within(menu).getByRole('button', { name })).toBeInTheDocument();
    }

    await user.click(within(menu).getByRole('button', { name: 'Pin' }));
    expect(mocks.store.setConversationPinned).toHaveBeenCalledWith('thread-1', true);
  });

  it('renames the open thread in place', async () => {
    const user = userEvent.setup();
    show(threads());

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    await user.click(screen.getByRole('button', { name: 'Rename' }));
    const field = screen.getByRole('textbox', { name: 'Rename conversation: Project questions' });
    await user.clear(field);
    await user.type(field, 'Build questions{Enter}');
    expect(mocks.store.renameConversation).toHaveBeenCalledWith('thread-1', 'Build questions');
  });
});

describe('ExplorerChat thread list', () => {
  function folder(): ExplorerThreads {
    return {
      ...threads(),
      threads: [
        thread('thread-1', 'Project questions', ago(0)),
        thread('pinned', 'Pinned plan', ago(40), { isPinned: true }),
        thread('yesterday', 'Build failure', ago(1)),
        thread('old', 'Old notes', ago(90)),
      ],
      archived: [thread('gone', 'Archived spike', ago(3), { isArchived: true })],
    };
  }

  async function openList() {
    const user = userEvent.setup();
    const value = folder();
    show(value);
    await user.click(screen.getByRole('button', { name: /Project questions/ }));
    return { user, value, list: screen.getByRole('navigation', { name: 'Threads' }) };
  }

  it('puts pinned threads first, then date groups, and marks the open thread', async () => {
    const { list } = await openList();
    const labels = within(list).getAllByRole('region').map((region) => region.getAttribute('aria-label'));
    const month = new Date(Date.now() - 90 * DAY).toLocaleDateString(undefined, { month: 'long', year: 'numeric' });
    expect(labels).toEqual(['Pinned', 'Today', 'Yesterday', month]);
    expect(within(list).getByRole('button', { name: /^Project questions/, current: true })).toBeInTheDocument();
    expect(within(list).queryByText('Archived spike')).not.toBeInTheDocument();
  });

  it('filters by the search box', async () => {
    const { user, list } = await openList();
    await user.type(screen.getByRole('searchbox', { name: 'Search threads' }), 'build');
    expect(within(list).getByText('Build failure')).toBeInTheDocument();
    expect(within(list).queryByText('Old notes')).not.toBeInTheDocument();
    expect(within(list).queryByText('Pinned plan')).not.toBeInTheDocument();
  });

  it('keeps archived threads behind a toggle', async () => {
    const { user, value, list } = await openList();
    const toggle = within(list).getByRole('button', { name: 'Archived (1)' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');

    await user.click(toggle);
    await user.click(within(list).getByRole('button', { name: /^Archived spike/ }));
    expect(value.select).toHaveBeenCalledWith('gone');
  });

  it('gives each row the shared menu, with rename in place', async () => {
    const { user, list } = await openList();
    await user.click(within(list).getByRole('button', { name: 'Actions for Old notes' }));
    const menu = screen.getByRole('dialog', { name: 'Actions for Old notes' });
    expect(within(menu).getByRole('button', { name: 'Copy as Markdown' })).toBeInTheDocument();
    expect(within(menu).getByRole('button', { name: 'Delete conversation…' })).toBeInTheDocument();

    await user.click(within(menu).getByRole('button', { name: 'Rename' }));
    const field = within(list).getByRole('textbox', { name: 'Rename conversation: Old notes' });
    await user.clear(field);
    await user.type(field, 'Older notes{Enter}');
    expect(mocks.store.renameConversation).toHaveBeenCalledWith('old', 'Older notes');
  });
});

describe('groupThreads', () => {
  afterEach(() => vi.useRealTimers());

  it('files threads by the sidebar buckets, then by month', () => {
    vi.useFakeTimers({ now: new Date(2026, 9, 17, 12) });
    const at = (month: number, day: number) => new Date(2026, month, day, 9).toISOString();
    const groups = groupThreads([
      thread('a', 'a', at(9, 17)),
      thread('b', 'b', at(9, 12)),
      thread('c', 'c', at(8, 25)),
      thread('d', 'd', at(7, 2)),
      thread('e', 'e', at(6, 20)),
      thread('f', 'f', at(6, 5), { isPinned: true }),
    ]);
    expect(groups.map((group) => [group.label, group.items.map((item) => item.id)])).toEqual([
      ['Pinned', ['f']],
      ['Today', ['a']],
      ['Last 7 days', ['b']],
      ['Last 30 days', ['c']],
      [new Date(2026, 7, 1).toLocaleDateString(undefined, { month: 'long', year: 'numeric' }), ['d']],
      [new Date(2026, 6, 1).toLocaleDateString(undefined, { month: 'long', year: 'numeric' }), ['e']],
    ]);
  });
});

describe('ExplorerChat conversation actions', () => {
  it('deletes the active conversation after confirmation', async () => {
    const user = userEvent.setup();
    const remove = vi.fn().mockResolvedValue(true);
    show(threads(remove));

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation…' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('Project questions');
    expect(remove).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Delete conversation' }));
    expect(remove).toHaveBeenCalledWith('thread-1');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('keeps the confirmation open when deletion fails', async () => {
    const user = userEvent.setup();
    show(threads(vi.fn().mockResolvedValue(false)));

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation…' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be deleted');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
});

describe('ExplorerChat composer', () => {
  it('puts the folder index notice and the selected lines into the chat composer', () => {
    show(threads());
    expect(screen.getByTestId('composer-notice')).toHaveTextContent('Folder index notice');
    expect(screen.getByTestId('composer-chips')).toHaveTextContent('Selected lines chip');
  });
});
