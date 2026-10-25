import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { ConversationSidebar } from '@/features/chat/components/ConversationSidebar';
import { deferred } from '@/tests/deferred';

const h = vi.hoisted(() => ({
  store: {} as Record<string, any>,
  navigate: vi.fn(),
  journals: { journals: [] as Array<Record<string, unknown>>, isLoading: false, error: null as null | Error, refetch: vi.fn() },
  selection: {} as Record<string, any>,
  listProps: null as null | Record<string, any>,
  referencesProps: null as null | Record<string, any>,
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/features/chat/hooks/workspaceQueries', () => ({ useJournalsQuery: () => h.journals }));
vi.mock('@/features/chat/components/sidebar/useJournalSelection', () => ({ useJournalSelection: () => h.selection }));
vi.mock('@/features/chat/components/sidebar/useConversationSynthesis', () => ({ useConversationSynthesis: () => ({ synthesis: true }) }));
vi.mock('@/features/chat/hooks/useConversationExport', () => ({ useConversationExport: () => ({ export: true }) }));
vi.mock('@/features/spaces/hooks/useSpaceEditor', () => ({ useSpaceEditor: () => ({ editor: true }) }));
vi.mock('@/features/chat/components/sidebar/useNewSpaceForm', () => ({ useNewSpaceForm: () => ({ form: true }) }));
vi.mock('@/hooks/useDebounce', () => ({ useDebounce: (value: unknown) => value }));
vi.mock('@/utils/conversationTitles', () => ({ createDefaultConversationTitle: () => 'New chat' }));
vi.mock('@/features/chat/components/sidebar/ConversationList', () => ({
  ConversationList: (props: Record<string, any>) => { h.listProps = props; return <div data-testid="list" />; },
}));
vi.mock('@/features/chat/components/sidebar/SidebarReferences', () => ({
  SidebarReferences: (props: Record<string, any>) => { h.referencesProps = props; return null; },
}));
vi.mock('@/features/chat/components/sidebar/SpacesPanel', () => ({
  SpacesPanel: ({ onClose }: { onClose: () => void }) => <div role="dialog" aria-label="Spaces"><button type="button" onClick={onClose}>Close spaces</button></div>,
}));

const makeStore = (overrides: Record<string, unknown> = {}) => ({
  spaces: [{ id: 's-hist', name: 'History', icon: '🏛️' }, { id: 's-plain', name: 'Plain', icon: null }],
  selectedSpaceId: null,
  filterMode: 'all',
  searchQuery: '',
  conversations: [{ id: 'c1' }, { id: 'c2' }],
  error: null,
  loadConversations: vi.fn().mockResolvedValue(undefined),
  setSelectedSpace: vi.fn(),
  setFilterMode: vi.fn(),
  setSearchQuery: vi.fn(),
  createConversation: vi.fn().mockResolvedValue(undefined),
  clearError: vi.fn(),
  ...overrides,
});

const makeSelection = (overrides: Record<string, unknown> = {}) => ({
  isSelectionMode: false,
  setIsSelectionMode: vi.fn(),
  selectedConversationIds: new Set<string>(),
  selectedConversationCount: 0,
  bulkSpaceIdDraft: '',
  setBulkSpaceIdDraft: vi.fn(),
  isBulkMoving: false,
  isCreatingQuickJournal: false,
  areAllVisibleConversationsSelected: false,
  toggleConversationSelection: vi.fn(),
  toggleSelectAllVisibleConversations: vi.fn(),
  addSelectedConversationsToJournal: vi.fn().mockResolvedValue(undefined),
  createQuickJournal: vi.fn().mockResolvedValue(undefined),
  openSelectedJournalNotebook: vi.fn(),
  ...overrides,
});

const renderSidebar = (onCollapse = vi.fn()) => {
  render(<TooltipProvider><ConversationSidebar onCollapse={onCollapse} /></TooltipProvider>);
  return { onCollapse };
};

describe('ConversationSidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.store = makeStore();
    h.selection = makeSelection();
    h.journals = { journals: [], isLoading: false, error: null, refetch: vi.fn().mockResolvedValue(undefined) };
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows Chat for all spaces with a conversation count', () => {
    renderSidebar();
    expect(screen.getByText('Chat')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Change scope' })).toHaveTextContent('All spaces');
    expect(screen.getByText('2 conversations')).toBeInTheDocument();
    expect(h.listProps).toMatchObject({ isJournalScope: false, isSelectionMode: false, synthesis: { synthesis: true }, exportActions: { export: true } });
    expect(h.referencesProps).toEqual({ query: '', active: false });
  });

  it('names the selected space with its icon', () => {
    h.store = makeStore({ selectedSpaceId: 's-hist', conversations: [{ id: 'c1' }] });
    const first = render(<TooltipProvider><ConversationSidebar /></TooltipProvider>);
    expect(screen.getByRole('button', { name: 'Change scope' })).toHaveTextContent('🏛️ History');
    expect(screen.getByText('1 conversation')).toBeInTheDocument();
    first.unmount();
    h.store = makeStore({ selectedSpaceId: 's-plain' });
    renderSidebar();
    expect(screen.getByRole('button', { name: 'Change scope' })).toHaveTextContent(/^Plain$/);
  });

  it('switches out of a journal scope, which belongs to the Journal', async () => {
    h.journals.journals = [{ id: 'j1', name: 'Garden diary', icon: '🌱', isArchived: false }];
    h.store = makeStore({ selectedSpaceId: 'j1' });
    renderSidebar();
    expect(screen.getByText('Garden diary')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Change scope' })).toHaveTextContent('🌱 Garden diary · Journal');
    expect(screen.getByLabelText('Search entries')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Open notebook' }));
    expect(h.selection.openSelectedJournalNotebook).toHaveBeenCalled();
    expect(h.store.setSelectedSpace).toHaveBeenCalledWith(null);
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledWith({ spaceId: null }));
  });

  it('counts journal entries in the plural', () => {
    h.journals.journals = [{ id: 'j1', name: 'Garden diary', icon: null, isArchived: false }];
    h.store = makeStore({ selectedSpaceId: 'j1' });
    renderSidebar();
    expect(screen.getByText('2 entries')).toBeInTheDocument();
  });

  it('names a journal without an icon', () => {
    h.journals.journals = [{ id: 'j1', name: 'Kitchen log', icon: null, isArchived: false }];
    h.store = makeStore({ selectedSpaceId: 'j1' });
    renderSidebar();
    expect(screen.getByRole('button', { name: 'Change scope' })).toHaveTextContent(/^Kitchen log · Journal$/);
    expect(screen.getByRole('button', { name: /New entry/ })).toBeInTheDocument();
  });

  it('starts a conversation, disabled while it starts', async () => {
    const creating = deferred<void>();
    h.store.createConversation.mockReturnValueOnce(creating.promise);
    renderSidebar();
    fireEvent.click(screen.getByRole('button', { name: /New conversation/ }));
    expect(h.store.createConversation).toHaveBeenCalledWith('New chat');
    await waitFor(() => expect(screen.getByRole('button', { name: /New conversation/ })).toBeDisabled());
    await act(async () => { creating.resolve(); await creating.promise; });
    expect(screen.getByRole('button', { name: /New conversation/ })).toBeEnabled();
  });

  it('collapses', () => {
    const { onCollapse } = renderSidebar();
    fireEvent.click(screen.getByRole('button', { name: /Hide sidebar/ }));
    expect(onCollapse).toHaveBeenCalled();
  });

  it('searches as you type and follows an outside query change', async () => {
    renderSidebar();
    fireEvent.change(screen.getByLabelText('Search conversations'), { target: { value: 'byzantium' } });
    expect(h.store.setSearchQuery).toHaveBeenCalledWith('byzantium');
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledWith({ searchQuery: 'byzantium' }));
    expect(h.referencesProps?.query).toBe('byzantium');
  });

  it('filters by tab', async () => {
    renderSidebar();
    fireEvent.click(screen.getByRole('tab', { name: 'Starred' }));
    expect(h.store.setFilterMode).toHaveBeenCalledWith('saved');
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledWith({ filterMode: 'saved' }));
  });

  it('shows references instead of selection in the Referenced tab', () => {
    h.store = makeStore({ filterMode: 'snippets' });
    h.selection = makeSelection({ isSelectionMode: true });
    renderSidebar();
    expect(screen.queryByRole('button', { name: /^Select$/ })).not.toBeInTheDocument();
    expect(screen.queryByText('0 selected')).not.toBeInTheDocument();
    expect(h.referencesProps?.active).toBe(true);
  });

  it('shows and dismisses an error', () => {
    h.store = makeStore({ error: 'Could not load conversations' });
    renderSidebar();
    expect(screen.getByText('Could not load conversations')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss error' }));
    expect(h.store.clearError).toHaveBeenCalled();
  });

  it('opens and closes the spaces panel', () => {
    renderSidebar();
    fireEvent.click(screen.getByRole('button', { name: 'Change scope' }));
    expect(screen.getByRole('dialog', { name: 'Spaces' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close spaces' }));
    expect(screen.queryByRole('dialog', { name: 'Spaces' })).not.toBeInTheDocument();
  });

  describe('selection mode', () => {
    it('turns on and loads journals, and turns off', () => {
      renderSidebar();
      fireEvent.click(screen.getByRole('button', { name: /^Select$/ }));
      expect(h.selection.setIsSelectionMode).toHaveBeenCalledWith(true);
      expect(h.journals.refetch).toHaveBeenCalled();
    });

    it('turning off does not reload journals', () => {
      h.selection = makeSelection({ isSelectionMode: true });
      renderSidebar();
      fireEvent.click(screen.getByRole('button', { name: /^Select$/ }));
      expect(h.selection.setIsSelectionMode).toHaveBeenCalledWith(false);
      expect(h.journals.refetch).not.toHaveBeenCalled();
    });

    it('moves the selection into a journal', async () => {
      h.journals.journals = [
        { id: 'j1', name: 'Garden diary', isArchived: false },
        { id: 'j2', name: 'Old notes', isArchived: true },
      ];
      h.selection = makeSelection({ isSelectionMode: true, selectedConversationCount: 2, bulkSpaceIdDraft: 'j1', areAllVisibleConversationsSelected: true });
      renderSidebar();
      expect(screen.getByText('2 selected')).toBeInTheDocument();
      const select = screen.getByRole('combobox', { name: 'Move to journal' });
      expect(Array.from((select as HTMLSelectElement).options).map((option) => option.text)).toEqual(['Choose a journal', 'Garden diary']);
      fireEvent.change(select, { target: { value: 'j1' } });
      expect(h.selection.setBulkSpaceIdDraft).toHaveBeenCalledWith('j1');
      fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
      expect(h.selection.toggleSelectAllVisibleConversations).toHaveBeenCalled();
      fireEvent.click(screen.getByRole('button', { name: 'Add' }));
      await waitFor(() => expect(h.selection.addSelectedConversationsToJournal).toHaveBeenCalled());
      fireEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(h.selection.setIsSelectionMode).toHaveBeenCalledWith(false);
    });

    it('cannot add with nothing selected or while moving', () => {
      h.journals.journals = [{ id: 'j1', name: 'Garden diary', isArchived: false }];
      h.selection = makeSelection({ isSelectionMode: true, bulkSpaceIdDraft: 'j1', isBulkMoving: true, selectedConversationCount: 1 });
      renderSidebar();
      expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Select all' })).toBeInTheDocument();
    });

    it('shows journals loading', () => {
      h.journals.isLoading = true;
      h.selection = makeSelection({ isSelectionMode: true });
      renderSidebar();
      expect(screen.getByRole('option', { name: 'Loading…' })).toBeInTheDocument();
      expect(screen.getByRole('combobox', { name: 'Move to journal' })).toBeDisabled();
    });

    it('explains journals that failed to load', () => {
      h.journals.error = new Error('journal table locked');
      h.selection = makeSelection({ isSelectionMode: true });
      renderSidebar();
      expect(screen.getByRole('option', { name: "Couldn't load journals" })).toBeInTheDocument();
      expect(screen.getByText("Couldn't load journals. journal table locked")).toBeInTheDocument();
    });

    it('offers to make or open a journal when there are none', async () => {
      h.selection = makeSelection({ isSelectionMode: true });
      renderSidebar();
      expect(screen.getByRole('option', { name: 'No journals yet' })).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'New journal' }));
      await waitFor(() => expect(h.selection.createQuickJournal).toHaveBeenCalled());
      fireEvent.click(screen.getByRole('button', { name: 'Open Journal' }));
      expect(h.navigate).toHaveBeenCalledWith('/journals');
    });
  });
});
