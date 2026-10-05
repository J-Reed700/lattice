import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useSearchParams } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { JournalWorkspace } from '@/features/journal/components/JournalWorkspace';

const { listJournals, createJournal } = vi.hoisted(() => ({
  listJournals: vi.fn(),
  createJournal: vi.fn(),
}));

vi.mock('@/lib/api', () => ({ default: { listJournals, createJournal }, VaultAPI: { listJournals, createJournal } }));
vi.mock('@/features/chat/components/SpacePickerPopover', () => ({ GENERAL_SPACE_ID: 'space_general' }));
vi.mock('@/components/RootLayout', () => ({ NEW_ITEM_EVENT: 'new-item' }));
vi.mock('@/hooks/queries/useWeeklySynthesisCandidatesQuery', () => ({
  useWeeklySynthesisCandidatesQuery: () => ({ data: { total: 0 }, refetch: vi.fn() }),
}));
vi.mock('@/hooks/useRegisterPaletteCommands', () => ({ useRegisterPaletteCommands: vi.fn() }));
vi.mock('@/stores/conversationsStore', () => ({ useConversationsStore: () => vi.fn() }));
vi.mock('@/features/journal/hooks/useJournalEntries', () => ({
  useJournalEntries: () => ({
    entries: [], pinnedIds: new Set(), selectedId: null, setSelectedId: vi.fn(),
    messagesByConversation: {}, loadingByConversation: {}, loadMessages: vi.fn(),
    removeEntry: vi.fn(), reload: vi.fn(), search: '', setSearch: vi.fn(),
    filter: 'all', setFilter: vi.fn(), isLoading: false, loadError: null,
    togglePinned: vi.fn(),
  }),
}));
vi.mock('@/features/journal/hooks/useJournalNote', () => ({
  UNTITLED_PAGE: 'Untitled page',
  useJournalNote: () => ({
    activeNote: null, pages: [], isLoadingNote: false, loadError: null, saveError: null,
    hasPendingChanges: false, isSavingNow: false, updateNote: vi.fn(), saveNow: vi.fn(),
    selectPage: vi.fn(), createPage: vi.fn(), renamePage: vi.fn(), deletePage: vi.fn(),
    refreshPages: vi.fn(), deleteActiveNote: vi.fn(),
  }),
}));
vi.mock('@/features/journal/hooks/useJournalSources', () => ({
  useJournalSources: () => ({ summaries: [], isLoading: false, scannedConversationCount: 0 }),
}));
vi.mock('@/features/journal/hooks/useJournalNavigationGuard', () => ({ useJournalNavigationGuard: vi.fn() }));
vi.mock('@/features/journal/components/EntryEditor', () => ({ EntryEditor: () => null }));
vi.mock('@/features/journal/components/EntryList', () => ({ EntryList: () => null }));
vi.mock('@/features/journal/components/PageList', () => ({ pageTitle: (page: { title: string }) => page.title }));

function LocationProbe() {
  const [params] = useSearchParams();
  return <output data-testid="journal-id">{params.get('journalSpaceId') ?? ''}</output>;
}

function renderWorkspace(children?: ReactNode) {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <MemoryRouter initialEntries={['/journals']}>
      <LocationProbe />
      {children}
      <JournalWorkspace />
    </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('JournalWorkspace first-run creation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('shares an in-progress initial journal creation with the empty-state button', async () => {
    const journals: Array<{ id: string; name: string; isArchived: boolean }> = [];
    listJournals.mockImplementation(async () => ({ ok: true, data: journals }));
    let resolveCreate: ((value: unknown) => void) | undefined;
    createJournal.mockImplementation(() => new Promise(resolve => { resolveCreate = resolve; }));
    renderWorkspace();

    await waitFor(() => expect(createJournal).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: 'New journal' }));
    expect(createJournal).toHaveBeenCalledTimes(1);

    const created = {
      id: 'journal-created-once', name: 'Journal 1', isArchived: false,
    };
    journals.push(created);
    resolveCreate?.({ ok: true, data: created });
    await waitFor(() => expect(screen.getByTestId('journal-id').textContent).toBe('journal-created-once'));
  });

  it('chooses an unused name when archived journals are the only existing journals', async () => {
    const journals: Array<{ id: string; name: string; isArchived: boolean }> = [{
      id: 'archived-journal', name: 'Old journal', isArchived: true,
    }];
    listJournals.mockImplementation(async () => ({ ok: true, data: journals }));
    createJournal.mockImplementation(async (request: { name: string }) => {
      const created = { id: 'new-journal', name: request.name, isArchived: false };
      journals.push(created);
      return { ok: true, data: created };
    });
    renderWorkspace();
    await screen.findByRole('button', { name: 'New journal' });

    fireEvent.click(screen.getByRole('button', { name: 'New journal' }));

    await waitFor(() => expect(createJournal).toHaveBeenCalledWith(expect.objectContaining({ name: 'Journal 2' })));
    await waitFor(() => expect(screen.getByTestId('journal-id').textContent).toBe('new-journal'));
  });

  it('shows manual creation errors in the empty state', async () => {
    listJournals.mockResolvedValue({ ok: true, data: [{
      id: 'archived-journal', name: 'Old journal', isArchived: true,
    }] });
    createJournal.mockResolvedValue({ ok: false, error: 'storage unavailable' });
    renderWorkspace();
    await screen.findByRole('button', { name: 'New journal' });

    fireEvent.click(screen.getByRole('button', { name: 'New journal' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('storage unavailable');
  });
});
