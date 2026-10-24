import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { deferred } from '@/tests/deferred';

import { ConversationList } from './ConversationList';

const h = vi.hoisted(() => ({
  store: {} as Record<string, unknown>,
  journals: [] as Array<{ id: string; name: string }>,
  forkLineage: new Map<string, { id: string; title: string; messageId: string | null }>(),
  deck: { mutate: vi.fn(), isPending: false, variables: undefined as { conversationId: string } | undefined },
  toast: { success: vi.fn(), warning: vi.fn(), error: vi.fn() },
  scrollToMessage: vi.fn(),
  reducedMotion: false,
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/features/chat/hooks/workspaceQueries', () => ({ useJournalsQuery: () => ({ journals: h.journals }) }));
vi.mock('@/features/chat/hooks/useForkLineage', () => ({ useForkLineage: () => h.forkLineage }));
vi.mock('@/features/learning/recall/flashcards/useFlashcards', () => ({ useGenerateConversationFlashcardDeck: () => h.deck }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/utils/chatMessageNavigation', () => ({ scrollToMessage: h.scrollToMessage }));
vi.mock('framer-motion', async (importOriginal) => ({
  ...(await importOriginal<typeof import('framer-motion')>()),
  useReducedMotion: () => h.reducedMotion,
}));

const NOW = new Date(2026, 4, 15, 12, 0, 0);
const daysAgo = (days: number) => new Date(NOW.getTime() - days * 86_400_000).toISOString();

const makeConversation = (id: string, title: string, overrides: Record<string, unknown> = {}) => ({
  id, title, updatedAt: daysAgo(0), messageCount: 2, isSaved: false, isPinned: false, isArchived: false, ...overrides,
});

const synthesis = () => ({
  synthesizeConversationToJournal: vi.fn(),
  synthesizingConversationId: null as string | null,
  isSynthesizing: false,
  continueConversationInNewChat: vi.fn(),
  continuingConversationId: null as string | null,
});

const exportActions = () => ({
  copyConversationAsMarkdown: vi.fn(),
  saveConversationToJournal: vi.fn(),
  copyingConversationId: null as string | null,
  savingConversationId: null as string | null,
});

const store = (overrides: Record<string, unknown> = {}) => ({
  spaces: [
    { id: 's-bio', name: 'Biology', accentColor: '#2f855a' },
    { id: 's-plain', name: 'Plain', accentColor: null },
  ],
  conversations: [],
  activeConversationId: null,
  isLoading: false,
  selectConversation: vi.fn().mockResolvedValue(undefined),
  renameConversation: vi.fn().mockResolvedValue(true),
  deleteConversation: vi.fn().mockResolvedValue(undefined),
  setConversationSaved: vi.fn(),
  setConversationPinned: vi.fn(),
  setConversationArchived: vi.fn(),
  ...overrides,
});

type Props = Partial<Parameters<typeof ConversationList>[0]>;
const renderList = (props: Props = {}) => {
  const all = {
    isJournalScope: false,
    isSelectionMode: false,
    selectedConversationIds: new Set<string>(),
    toggleConversationSelection: vi.fn(),
    synthesis: synthesis(),
    exportActions: exportActions(),
    ...props,
  };
  return { ...render(<ConversationList {...all} />, { wrapper: TooltipProvider }), props: all };
};

const row = (title: string) => screen.getByRole('button', { name: `Select conversation: ${title}` });

const openActions = async (title: string) => {
  await userEvent.click(screen.getByRole('button', { name: `Actions for ${title}` }));
  return screen.findByRole('dialog', { name: `Actions for ${title}` });
};

describe('ConversationList', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
    vi.clearAllMocks();
    h.journals = [];
    h.forkLineage = new Map();
    h.deck = { mutate: vi.fn(), isPending: false, variables: undefined };
    h.reducedMotion = false;
    h.store = store();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows a spinner while the first page loads', () => {
    h.store = store({ isLoading: true });
    renderList();
    expect(screen.queryByRole('navigation')).not.toBeInTheDocument();
    expect(screen.queryByText('No conversations yet.')).not.toBeInTheDocument();
  });

  it('says when there are no conversations, or no journal entries', () => {
    const { unmount } = renderList();
    expect(screen.getByText('No conversations yet.')).toBeInTheDocument();
    unmount();
    renderList({ isJournalScope: true });
    expect(screen.getByText('No entries yet.')).toBeInTheDocument();
  });

  it('leaves Explorer threads to the Explorer', () => {
    h.store = store({ conversations: [makeConversation('x', 'Folder thread', { explorerRoot: '/notes' })] });
    renderList();
    expect(screen.getByText('No conversations yet.')).toBeInTheDocument();
  });

  it('groups conversations into time buckets, newest first', () => {
    h.store = store({
      conversations: [
        makeConversation('a', 'Mitosis basics', { updatedAt: daysAgo(40) }),
        makeConversation('b', 'Sourdough starter', { updatedAt: new Date(NOW.getTime() - 3_600_000).toISOString() }),
        makeConversation('c', 'Feudal Japan', { updatedAt: daysAgo(3) }),
        makeConversation('d', 'Tide pools', { updatedAt: new Date(NOW.getTime() - 60_000).toISOString() }),
        makeConversation('e', 'Bad date', { updatedAt: 'not-a-date' }),
      ],
    });
    renderList();
    const nav = screen.getByRole('navigation', { name: 'Conversations' });
    const sections = within(nav).getAllByRole('heading', { level: 3 }).map((heading) => heading.textContent);
    expect(sections).toEqual(['Tide pools', 'Sourdough starter', 'Bad date', 'Feudal Japan', 'Mitosis basics']);
    expect(screen.getByText('Today')).toBeInTheDocument();
    expect(screen.getByText('Last 7 days')).toBeInTheDocument();
    expect(screen.getByText('Older')).toBeInTheDocument();
    expect(screen.queryByText('Yesterday')).not.toBeInTheDocument();
  });

  it('groups journal entries by day', () => {
    h.store = store({
      conversations: [
        makeConversation('a', 'Morning walk', { updatedAt: daysAgo(1) }),
        makeConversation('b', 'Bread notes', { updatedAt: daysAgo(0) }),
        makeConversation('c', 'Late entry', { updatedAt: daysAgo(1) }),
      ],
    });
    renderList({ isJournalScope: true });
    const labels = screen.getAllByText(/^(Today|Yesterday)$/).map((node) => node.textContent);
    expect(labels).toEqual(['Today', 'Yesterday']);
  });

  it('shows space, journal, accent, preview and the active row', () => {
    h.journals = [{ id: 'j1', name: 'Diary' }];
    h.store = store({
      activeConversationId: 'a',
      spaces: [{ id: 's-bio', name: 'Biology', accentColor: '#2f855a' }, { id: 'j1', name: 'Diary', accentColor: null }],
      conversations: [
        makeConversation('a', 'Photosynthesis', { spaceId: 's-bio', lastMessagePreview: 'Light reactions first' }),
        makeConversation('b', 'Dream log', { spaceId: 'j1', messages: [{ content: 'first' }, { content: 'Flying over hills' }] }),
        makeConversation('c', 'Unknown space', { spaceId: 's-gone' }),
      ],
    });
    renderList();
    expect(row('Photosynthesis')).toHaveAttribute('aria-current', 'page');
    expect(row('Dream log')).not.toHaveAttribute('aria-current');
    expect(screen.getByLabelText('Space: Biology')).toHaveStyle({ backgroundColor: '#2f855a' });
    expect(screen.getByText(/^Biology · /)).toBeInTheDocument();
    expect(screen.getByText(/^Diary · Journal · /)).toBeInTheDocument();
    expect(screen.getByText(/^s-gone · /)).toBeInTheDocument();
    expect(screen.getByText('Light reactions first')).toBeInTheDocument();
    expect(screen.getByText('Flying over hills')).toBeInTheDocument();
  });

  it('draws a still active bar when motion is reduced', () => {
    h.reducedMotion = true;
    h.store = store({ activeConversationId: 'a', conversations: [makeConversation('a', 'Volcanoes')] });
    renderList();
    expect(row('Volcanoes')).toHaveAttribute('aria-current', 'page');
  });

  it('opens a conversation by click, Enter or Space', async () => {
    h.store = store({ conversations: [makeConversation('a', 'Volcanoes')] });
    const select = h.store.selectConversation as ReturnType<typeof vi.fn>;
    renderList();
    fireEvent.click(row('Volcanoes'));
    fireEvent.keyDown(row('Volcanoes'), { key: 'Enter' });
    fireEvent.keyDown(row('Volcanoes'), { key: ' ' });
    fireEvent.keyDown(row('Volcanoes'), { key: 'a' });
    await waitFor(() => expect(select).toHaveBeenCalledTimes(3));
    expect(select).toHaveBeenCalledWith('a');
  });

  it('toggles rows instead of opening them in selection mode', () => {
    h.store = store({ conversations: [makeConversation('a', 'Volcanoes'), makeConversation('b', 'Glaciers')] });
    const toggle = vi.fn();
    renderList({ isSelectionMode: true, selectedConversationIds: new Set(['b']), toggleConversationSelection: toggle });
    const [volcanoBox, glacierBox] = screen.getAllByRole('checkbox');
    expect(volcanoBox).not.toBeChecked();
    expect(glacierBox).toBeChecked();
    fireEvent.click(row('Volcanoes'));
    fireEvent.keyDown(row('Glaciers'), { key: 'Enter' });
    fireEvent.click(volcanoBox);
    expect(toggle.mock.calls).toEqual([['a'], ['b'], ['a']]);
    expect(h.store.selectConversation).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /Actions for/ })).not.toBeInTheDocument();
  });

  it('shows progress for synthesis and continuation', () => {
    h.store = store({ conversations: [makeConversation('a', 'Volcanoes'), makeConversation('b', 'Glaciers')] });
    renderList({ synthesis: { ...synthesis(), synthesizingConversationId: 'a', continuingConversationId: 'b' } });
    expect(screen.getByText('Synthesizing to Journal…')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Summarizing into a new chat…');
  });

  it('links a branch to its parent at the turn it forked from', async () => {
    h.forkLineage = new Map([
      ['b', { id: 'a', title: 'Volcanoes', messageId: 'm7' }],
      ['c', { id: 'a', title: 'Volcanoes', messageId: null }],
    ]);
    h.store = store({ conversations: [makeConversation('a', 'Volcanoes'), makeConversation('b', 'Lava'), makeConversation('c', 'Ash')] });
    renderList();
    const links = screen.getAllByRole('button', { name: /Branched from Volcanoes/ });
    expect(links[0]).toHaveAttribute('title', 'Open "Volcanoes"');
    fireEvent.click(links[0]);
    await waitFor(() => expect(h.scrollToMessage).toHaveBeenCalledWith('m7'));
    expect(h.store.selectConversation).toHaveBeenCalledWith('a');
    expect(h.store.selectConversation).toHaveBeenCalledTimes(1);
    fireEvent.click(links[1]);
    await waitFor(() => expect(h.store.selectConversation).toHaveBeenCalledTimes(2));
    expect(h.scrollToMessage).toHaveBeenCalledTimes(1);
  });

  describe('renaming', () => {
    beforeEach(() => {
      h.store = store({ conversations: [makeConversation('a', 'Volcanoes')] });
    });

    it('renames on double-click and Enter', async () => {
      renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      const input = screen.getByRole('textbox', { name: 'Rename conversation: Volcanoes' });
      expect(input).toHaveValue('Volcanoes');
      // A click on the row while renaming must not open the chat.
      fireEvent.click(row('Volcanoes'));
      fireEvent.keyDown(row('Volcanoes'), { key: 'Enter' });
      fireEvent.change(input, { target: { value: '  Shield volcanoes  ' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() => expect(h.store.renameConversation).toHaveBeenCalledWith('a', 'Shield volcanoes'));
      await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument());
      expect(h.store.selectConversation).not.toHaveBeenCalled();
    });

    it('keeps the editor open when the rename is refused', async () => {
      (h.store.renameConversation as ReturnType<typeof vi.fn>).mockResolvedValueOnce(false);
      renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Calderas' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save conversation title' }));
      await waitFor(() => expect(h.store.renameConversation).toHaveBeenCalledWith('a', 'Calderas'));
      expect(screen.getByRole('textbox')).toBeInTheDocument();
    });

    it('refuses an empty title', () => {
      renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });
      fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
      expect(h.toast.warning).toHaveBeenCalledWith('Title required', expect.objectContaining({ message: 'Conversation title cannot be empty.' }));
      expect(h.store.renameConversation).not.toHaveBeenCalled();
    });

    it('closes without a call when the title is unchanged', async () => {
      renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
      await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument());
      expect(h.store.renameConversation).not.toHaveBeenCalled();
    });

    it('cancels with Escape or the cancel button', () => {
      renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      fireEvent.keyDown(screen.getByRole('textbox'), { key: 'a' });
      fireEvent.click(screen.getByRole('button', { name: 'Cancel rename' }));
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    });

    it('drops an open rename when selection mode starts', () => {
      const { rerender, props } = renderList();
      fireEvent.doubleClick(screen.getByRole('heading', { name: 'Volcanoes' }));
      expect(screen.getByRole('textbox')).toBeInTheDocument();
      rerender(<ConversationList {...props} isSelectionMode />);
      expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    });

    it('starts from the actions menu', async () => {
      renderList();
      const menu = await openActions('Volcanoes');
      fireEvent.click(within(menu).getByRole('button', { name: 'Rename' }));
      expect(screen.getByRole('textbox', { name: 'Rename conversation: Volcanoes' })).toBeInTheDocument();
    });
  });

  describe('actions menu', () => {
    it('acts on the row it was opened from', async () => {
      h.store = store({ conversations: [makeConversation('a', 'Volcanoes'), makeConversation('b', 'Glaciers')] });
      const actions = exportActions();
      const synth = synthesis();
      renderList({ exportActions: actions, synthesis: synth });

      let menu = await openActions('Glaciers');
      fireEvent.click(within(menu).getByRole('button', { name: 'Copy as Markdown' }));
      expect(actions.copyConversationAsMarkdown).toHaveBeenCalledWith('b', 'Glaciers');
      await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

      menu = await openActions('Glaciers');
      fireEvent.click(within(menu).getByRole('button', { name: 'Save to journal' }));
      expect(actions.saveConversationToJournal).toHaveBeenCalledWith('b', 'Glaciers');

      menu = await openActions('Glaciers');
      fireEvent.click(within(menu).getByRole('button', { name: 'Synthesize to journal' }));
      expect(synth.synthesizeConversationToJournal).toHaveBeenCalledWith('b', 'Glaciers');

      menu = await openActions('Glaciers');
      fireEvent.click(within(menu).getByRole('button', { name: 'Continue in new chat' }));
      expect(synth.continueConversationInNewChat).toHaveBeenCalledWith('b', 'Glaciers');
    });

    it.each([
      ['Save conversation', 'setConversationSaved', false],
      ['Pin', 'setConversationPinned', false],
      ['Archive', 'setConversationArchived', false],
      ['Remove from saved', 'setConversationSaved', true],
      ['Unpin', 'setConversationPinned', true],
      ['Restore', 'setConversationArchived', true],
    ] as const)('"%s" calls %s with the opposite of %s', async (label, action, current) => {
      h.store = store({
        conversations: [
          makeConversation('a', 'Volcanoes', { isSaved: current, isPinned: current, isArchived: current }),
        ],
      });
      renderList();
      const menu = await openActions('Volcanoes');
      fireEvent.click(within(menu).getByRole('button', { name: label }));
      expect(h.store[action]).toHaveBeenCalledWith('a', !current);
    });

    it('creates flashcards titled after the conversation and reports a failure', async () => {
      h.store = store({ conversations: [makeConversation('a', 'Volcanoes')] });
      renderList();
      const menu = await openActions('Volcanoes');
      fireEvent.click(within(menu).getByRole('button', { name: 'Create flashcards' }));
      expect(h.deck.mutate).toHaveBeenCalledWith(
        { conversationId: 'a', title: 'Volcanoes · Verified claims' },
        expect.objectContaining({ onError: expect.any(Function) }),
      );
      const { onError } = h.deck.mutate.mock.calls[0][1];
      onError(new Error('no verified claims'));
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't create flashcards", { message: 'no verified claims' });
    });

    it('disables busy actions', async () => {
      h.deck = { mutate: vi.fn(), isPending: true, variables: { conversationId: 'a' } };
      h.store = store({ conversations: [makeConversation('a', 'Volcanoes')] });
      renderList({
        exportActions: { ...exportActions(), copyingConversationId: 'a', savingConversationId: 'a' },
        synthesis: { ...synthesis(), isSynthesizing: true, synthesizingConversationId: 'a', continuingConversationId: 'a' },
      });
      const menu = await openActions('Volcanoes');
      for (const name of ['Copy as Markdown', 'Save to journal', 'Create flashcards', 'Synthesize to journal', 'Continue in new chat']) {
        expect(within(menu).getByRole('button', { name })).toBeDisabled();
      }
    });

    it('deletes and disables the delete while it runs', async () => {
      const pending = deferred<void>();
      h.store = store({
        conversations: [makeConversation('a', 'Volcanoes')],
        deleteConversation: vi.fn(() => pending.promise),
      });
      renderList();
      let menu = await openActions('Volcanoes');
      fireEvent.click(within(menu).getByRole('button', { name: 'Delete conversation' }));
      expect(h.store.deleteConversation).toHaveBeenCalledWith('a');
      menu = await openActions('Volcanoes');
      expect(within(menu).getByRole('button', { name: 'Delete conversation' })).toBeDisabled();
      pending.resolve();
      await waitFor(() => expect(within(menu).getByRole('button', { name: 'Delete conversation' })).toBeEnabled());
    });
  });
});
