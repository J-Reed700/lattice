import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { Combine, FilePlus2, NotebookPen, PanelLeft, Plus } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router';

import { NEW_ITEM_EVENT } from '@/components/RootLayout';
import { useWeeklySynthesisCandidatesQuery } from '@/hooks/queries/useWeeklySynthesisCandidatesQuery';
import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { EntryEditor } from './EntryEditor';
import { EntryList } from './EntryList';
import {
  appendToNote,
  buildSynthesisBlock,
  resolveWeekPage,
  weekPageTitle,
} from './synthesisTargets';
import { useJournalEntries } from './useJournalEntries';
import { useJournalNavigationGuard } from './useJournalNavigationGuard';
import { useJournalNote } from './useJournalNote';
import { useJournalSources } from './useJournalSources';

import type { SynthesisScope } from './SynthesizePopover';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const SIDEBAR_COLLAPSED_KEY = 'journal.sidebar.collapsed';
const DEFAULT_JOURNAL_ICON = '📓';
const DEFAULT_JOURNAL_ACCENT = '#aa503d'; // Default notebook cover accent; saved custom colors are preserved.
const SYNTHESIS_ENTRY_LIMIT = 12;

type ActionTone = 'info' | 'success' | 'error';

interface Notice {
  tone: ActionTone;
  message: string;
  /** One verb, when the message names somewhere the user may want to go. */
  action?: { label: string; run: () => void };
}

function nowIso(): string {
  return new Date().toISOString();
}

function nextJournalName(existing: ConversationJournalDto[]): string {
  const existingNames = new Set(existing.map((j) => j.name.toLowerCase()));
  let idx = existing.length + 1;
  let candidate = `Journal ${idx}`;
  while (existingNames.has(candidate.toLowerCase())) {
    idx += 1;
    candidate = `Journal ${idx}`;
  }
  return candidate;
}

function readSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === '1';
  } catch {
    return false;
  }
}

function writeSidebarCollapsed(value: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, value ? '1' : '0');
  } catch {
    // Ignore
  }
}

function defaultJournalTitle(spaceName: string): string {
  return `Journal · ${spaceName}`;
}

function unique(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

const SYNTHESIS_HEADINGS: Record<SynthesisScope, string> = {
  current: 'Current Entry',
  pinned: 'Pinned Entries',
  deck: 'Recent Entries',
  week: 'Past Week',
  conversation: 'This Conversation',
};

function readPinnedNoteHighlights(spaceId: string | null): Set<string> {
  if (!spaceId) return new Set();
  try {
    const raw = localStorage.getItem(`journal.pinnedNoteHighlights.${spaceId}`);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((v): v is string => typeof v === 'string'));
  } catch {
    return new Set();
  }
}

function writePinnedNoteHighlights(spaceId: string, ids: Set<string>): void {
  try {
    localStorage.setItem(
      `journal.pinnedNoteHighlights.${spaceId}`,
      JSON.stringify([...ids]),
    );
  } catch {
    // Ignore
  }
}

function makeId(prefix: string): string {
  return `${prefix}_${crypto.randomUUID()}`;
}

/**
 * Shell orchestrator for the journal workspace. Reads URL params, handles
 * journal list / auto-redirect lifecycle, wires the data hooks to the
 * presentational components.
 */
export function JournalWorkspace() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedJournalSpaceId = searchParams.get('journalSpaceId');
  const requestedEntryId = searchParams.get('entryId');
  const requestedNoteIdParam = searchParams.get('noteId');
  // `?noteId=` is a one-shot instruction ("open the page that was just
  // written"), not a mirror of the selection: it is consumed on arrival so a
  // later journal switch does not drag the user back to the same page.
  const [requestedNoteId, setRequestedNoteId] = useState<string | null>(requestedNoteIdParam);

  const { data: weekCandidates, refetch: refetchWeekCandidates } =
    useWeeklySynthesisCandidatesQuery();

  const [allJournals, setAllJournals] = useState<ConversationJournalDto[]>([]);
  const [journalSpace, setJournalSpace] = useState<ConversationJournalDto | null>(null);
  const [topLevelError, setTopLevelError] = useState<string | null>(null);
  const [journalLoadAttempt, setJournalLoadAttempt] = useState(0);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [pinnedNoteHighlightIds, setPinnedNoteHighlightIds] = useState<Set<string>>(
    new Set(),
  );
  const appliedInitialJournalRef = useRef(false);

  const entriesState = useJournalEntries({
    journalSpaceId: requestedJournalSpaceId,
    requestedEntryId,
  });
  const {
    entries,
    pinnedIds,
    selectedId,
    setSelectedId,
    messagesByConversation,
    loadingByConversation,
    loadMessages,
    removeEntry,
    reload: reloadEntries,
  } = entriesState;

  const noteState = useJournalNote({
    journalSpaceId: requestedJournalSpaceId,
    journalName: journalSpace?.name ?? null,
    requestedNoteId,
  });
  const {
    activeNote,
    pages,
    isLoadingNote,
    loadError: noteLoadError,
    saveError,
    hasPendingChanges,
    isSavingNow,
    updateNote,
    saveNow,
    selectPage,
    createPage,
    refreshPages,
  } = noteState;

  useJournalNavigationGuard(hasPendingChanges, saveNow);

  const sourcesState = useJournalSources({
    entries,
    pinnedIds,
    messagesByConversation,
    loadingByConversation,
    loadMessages,
    enabled: true,
  });

  // Persist sidebar collapsed state
  useEffect(() => {
    writeSidebarCollapsed(sidebarCollapsed);
  }, [sidebarCollapsed]);

  // Take `?noteId=` off the URL once it has been handed to the note hook.
  useEffect(() => {
    if (!requestedNoteIdParam) return;
    setRequestedNoteId(requestedNoteIdParam);
    const next = new URLSearchParams(searchParams);
    next.delete('noteId');
    setSearchParams(next, { replace: true });
  }, [requestedNoteIdParam, searchParams, setSearchParams]);

  // Sync pinned note-highlight ids (per journal) from localStorage
  useEffect(() => {
    setPinnedNoteHighlightIds(readPinnedNoteHighlights(requestedJournalSpaceId));
  }, [requestedJournalSpaceId]);
  useEffect(() => {
    if (!requestedJournalSpaceId) return;
    writePinnedNoteHighlights(requestedJournalSpaceId, pinnedNoteHighlightIds);
  }, [requestedJournalSpaceId, pinnedNoteHighlightIds]);

  // Reconcile pinned note-highlight ids to only valid highlights on the active note
  useEffect(() => {
    if (!activeNote) return;
    const valid = new Set(activeNote.highlights.map((h) => h.id));
    setPinnedNoteHighlightIds((current) => {
      const next = new Set([...current].filter((id) => valid.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [activeNote]);

  // Auto-redirect: ensure URL always has a journalSpaceId when one exists
  useEffect(() => {
    let cancelled = false;

    const ensureJournal = async () => {
      setTopLevelError(null);
      const journalsResult = await VaultAPI.listJournals();
      if (cancelled) return;
      if (!journalsResult.ok) {
        setTopLevelError(`Failed to load journals: ${journalsResult.error}`);
        return;
      }

      const fetched = journalsResult.data;
      const active = fetched.filter((j) => !j.isArchived);
      setAllJournals(active);

      if (requestedJournalSpaceId) {
        const requestedIsActive = active.some((j) => j.id === requestedJournalSpaceId);
        if (requestedIsActive) {
          const match = active.find((j) => j.id === requestedJournalSpaceId) ?? null;
          setJournalSpace(match);
          try {
            localStorage.setItem(LAST_JOURNAL_SPACE_KEY, requestedJournalSpaceId);
          } catch {
            // Ignore
          }
          return;
        }

        const params = new URLSearchParams(searchParams);
        params.delete('entryId');
        if (active.length === 0) {
          params.delete('journalSpaceId');
          const next = params.toString();
          navigate(next ? `/journals?${next}` : '/journals', { replace: true });
          return;
        }
        params.set('journalSpaceId', active[0].id);
        navigate(`/journals?${params.toString()}`, { replace: true });
        return;
      }

      if (appliedInitialJournalRef.current) return;
      appliedInitialJournalRef.current = true;

      if (active.length === 0 && fetched.length === 0) {
        const initialName = nextJournalName(fetched);
        const created = await VaultAPI.createJournal({
          name: initialName,
          description: null,
          icon: DEFAULT_JOURNAL_ICON,
          accentColor: DEFAULT_JOURNAL_ACCENT,
          spacePrompt: null,
          defaultModelName: null,
          toolPreferencesJson: null,
        });
        if (cancelled) return;
        if (!created.ok) {
          setTopLevelError(`Failed to create initial journal: ${created.error}`);
          return;
        }
        const params = new URLSearchParams(searchParams);
        params.set('journalSpaceId', created.data.id);
        try {
          localStorage.setItem(LAST_JOURNAL_SPACE_KEY, created.data.id);
        } catch {
          // Ignore
        }
        navigate(`/journals?${params.toString()}`, { replace: true });
        return;
      }

      if (active.length === 0) return;

      let preferredId: string | null = null;
      try {
        preferredId = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
      } catch {
        preferredId = null;
      }
      const target = active.find((j) => j.id === preferredId) ?? active[0];
      const params = new URLSearchParams(searchParams);
      params.set('journalSpaceId', target.id);
      navigate(`/journals?${params.toString()}`, { replace: true });
    };

    void ensureJournal();
    return () => {
      cancelled = true;
    };
  }, [requestedJournalSpaceId, navigate, searchParams, journalLoadAttempt]);

  const notify = useCallback(
    (tone: ActionTone, message: string, action?: Notice['action']) => {
      setNotice({ tone, message, action });
    },
    [],
  );

  const handleSwitchJournal = useCallback(
    (journalId: string) => {
      const params = new URLSearchParams(searchParams);
      params.set('journalSpaceId', journalId);
      params.delete('entryId');
      navigate(`/journals?${params.toString()}`);
    },
    [navigate, searchParams],
  );

  const handleCreateJournal = useCallback(async () => {
    const name = nextJournalName(allJournals);
    const created = await VaultAPI.createJournal({
      name,
      description: null,
      icon: DEFAULT_JOURNAL_ICON,
      accentColor: DEFAULT_JOURNAL_ACCENT,
      spacePrompt: null,
      defaultModelName: null,
      toolPreferencesJson: null,
    });
    if (!created.ok) {
      notify('error', `Failed to create journal: ${created.error}`);
      return;
    }
    setAllJournals((prev) => [...prev, created.data]);
    const params = new URLSearchParams(searchParams);
    params.set('journalSpaceId', created.data.id);
    params.delete('entryId');
    navigate(`/journals?${params.toString()}`);
    notify('success', `Created "${name}".`);
  }, [allJournals, navigate, notify, searchParams]);

  const handleRenameJournal = useCallback(
    async (nextName: string) => {
      if (!requestedJournalSpaceId || !journalSpace) return;
      const result = await VaultAPI.updateJournal({
        journalId: requestedJournalSpaceId,
        name: nextName,
      });
      if (!result.ok) {
        notify('error', result.error);
        return;
      }
      setJournalSpace(result.data);
      setAllJournals((prev) =>
        prev.map((j) => (j.id === result.data.id ? result.data : j)),
      );
      notify('success', `Renamed journal to "${result.data.name}".`);

      if (activeNote) {
        const oldDefault = defaultJournalTitle(journalSpace.name);
        if (activeNote.title.trim() === oldDefault) {
          updateNote((note) => ({
            ...note,
            title: defaultJournalTitle(result.data.name),
          }));
        }
      }
    },
    [activeNote, journalSpace, notify, requestedJournalSpaceId, updateNote],
  );

  const handleDeleteJournal = useCallback(async () => {
    if (!requestedJournalSpaceId || !journalSpace) return;
    const result = await VaultAPI.deleteJournal({ journalId: requestedJournalSpaceId });
    if (!result.ok) {
      notify('error', result.error);
      return;
    }
    try {
      localStorage.removeItem(`journal.noteBySpace.${requestedJournalSpaceId}`);
      localStorage.removeItem(`journal.pinnedBookmarks.${requestedJournalSpaceId}`);
      localStorage.removeItem(`journal.pinnedNoteHighlights.${requestedJournalSpaceId}`);
      localStorage.removeItem(`journal.pinnedEntries.${requestedJournalSpaceId}`);
      const last = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
      if (last === requestedJournalSpaceId) {
        localStorage.removeItem(LAST_JOURNAL_SPACE_KEY);
      }
    } catch {
      // Ignore
    }
    const fresh = await VaultAPI.listJournals();
    const remaining = fresh.ok ? fresh.data.filter((j) => !j.isArchived) : [];
    const params = new URLSearchParams(searchParams);
    params.delete('entryId');
    if (remaining.length === 0) {
      params.delete('journalSpaceId');
    } else {
      params.set('journalSpaceId', remaining[0].id);
    }
    const next = params.toString();
    navigate(next ? `/journals?${next}` : '/journals', { replace: true });
    notify('success', `Deleted journal "${journalSpace.name}".`);
  }, [journalSpace, navigate, notify, requestedJournalSpaceId, searchParams]);

  const createConversationViaStore = useConversationsStore((s) => s.createConversation);

  const handleNewEntry = useCallback(async () => {
    if (!requestedJournalSpaceId) return;
    try {
      const title = `Entry · ${new Date().toLocaleDateString(undefined, {
        weekday: 'short',
        month: 'short',
        day: 'numeric',
      })}`;
      const newConversationId = await createConversationViaStore(title);
      const linkResult = await VaultAPI.addConversationToJournal({
        journalSpaceId: requestedJournalSpaceId,
        conversationId: newConversationId,
      });
      if (!linkResult.ok) {
        notify('error', `Failed to attach entry to journal: ${linkResult.error}`);
        return;
      }
      await reloadEntries();
      setSelectedId(newConversationId);
    } catch (error) {
      notify(
        'error',
        error instanceof Error ? error.message : 'Failed to create entry.',
      );
    }
  }, [createConversationViaStore, notify, reloadEntries, requestedJournalSpaceId, setSelectedId]);

  // ⌘N from anywhere in the app (RootLayout) and `?new=1` deep links both
  // create an entry in the current journal.
  useEffect(() => {
    const onNew = () => {
      void handleNewEntry();
    };
    window.addEventListener(NEW_ITEM_EVENT, onNew);
    return () => window.removeEventListener(NEW_ITEM_EVENT, onNew);
  }, [handleNewEntry]);

  useEffect(() => {
    if (searchParams.get('new') !== '1' || !requestedJournalSpaceId) return;
    const next = new URLSearchParams(searchParams);
    next.delete('new');
    setSearchParams(next, { replace: true });
    void handleNewEntry();
  }, [handleNewEntry, requestedJournalSpaceId, searchParams, setSearchParams]);

  const handleSelectPage = useCallback(
    (noteId: string) => {
      void selectPage(noteId);
    },
    [selectPage],
  );

  const handleNewPage = useCallback(async () => {
    const title = `Page · ${new Date().toLocaleDateString(undefined, {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    })}`;
    const created = await createPage(title);
    if (created) notify('success', `Created "${created.title}".`);
  }, [createPage, notify]);

  const handleRenameEntry = useCallback(
    async (entryId: string, title: string) => {
      const result = await VaultAPI.renameConversation(entryId, title);
      if (!result.ok) {
        notify('error', `Failed to rename entry: ${result.error}`);
        return;
      }
      await reloadEntries();
    },
    [notify, reloadEntries],
  );

  const handleDeleteEntry = useCallback(
    async (entryId: string) => {
      const result = await VaultAPI.deleteConversation(entryId);
      if (!result.ok) {
        notify('error', `Failed to delete entry: ${result.error}`);
        return;
      }
      removeEntry(entryId);
      notify('success', 'Entry deleted.');
    },
    [notify, removeEntry],
  );

  const handleUpdateNoteContent = useCallback(
    (content: string) => {
      updateNote((note) => ({ ...note, content }));
    },
    [updateNote],
  );

  const handleAddHighlight = useCallback(
    (text: string) => {
      if (!text.trim()) return;
      updateNote((note) => ({
        ...note,
        highlights: [
          {
            id: makeId('highlight'),
            text,
            createdAt: nowIso(),
          },
          ...note.highlights,
        ],
      }));
      notify('success', 'Highlight added.');
    },
    [notify, updateNote],
  );

  const handleRemoveHighlight = useCallback(
    (highlightId: string) => {
      updateNote((note) => ({
        ...note,
        highlights: note.highlights.filter((h) => h.id !== highlightId),
      }));
    },
    [updateNote],
  );

  const handleTogglePinnedHighlight = useCallback((highlightId: string) => {
    setPinnedNoteHighlightIds((current) => {
      const next = new Set(current);
      if (next.has(highlightId)) next.delete(highlightId);
      else next.add(highlightId);
      return next;
    });
  }, []);

  const selectedEntry = useMemo(
    () => entries.find((e) => e.id === selectedId) ?? null,
    [entries, selectedId],
  );

  const selectedEntryMessages = selectedId ? messagesByConversation[selectedId] ?? [] : [];
  const selectedEntryLoading = selectedId ? Boolean(loadingByConversation[selectedId]) : false;

  const selectSynthesisTargets = useCallback(
    (scope: SynthesisScope) => {
      // The week scope selects server-side; "this conversation" is not reachable
      // from the Journal surface.
      if (scope === 'week' || scope === 'conversation') return [];
      if (scope === 'current') {
        const currentId = selectedId ?? entries[0]?.id ?? null;
        if (!currentId) return [];
        return entries.filter((e) => e.id === currentId);
      }
      if (scope === 'pinned') {
        return entries
          .filter((e) => pinnedIds.has(e.id))
          .slice(0, SYNTHESIS_ENTRY_LIMIT);
      }
      return entries.slice(0, SYNTHESIS_ENTRY_LIMIT);
    },
    [entries, pinnedIds, selectedId],
  );

  const handleSynthesize = useCallback(
    async (scope: SynthesisScope): Promise<boolean> => {
      if (!activeNote) {
        notify('error', 'Select a notebook page before synthesizing.');
        return false;
      }
      const targets = selectSynthesisTargets(scope);
      if (scope !== 'week' && targets.length === 0) {
        notify('error', 'No journal entries available for synthesis.');
        return false;
      }
      try {
        const result = await VaultAPI.synthesizeJournalEntries({
          conversationIds: targets.map((t) => t.id),
          scope,
          maxEntries: SYNTHESIS_ENTRY_LIMIT,
        });
        if (!result.ok) {
          notify('error', result.error);
          return false;
        }
        const block = buildSynthesisBlock({
          heading: SYNTHESIS_HEADINGS[scope],
          entryCount: result.data.entryCount,
          synthesis: result.data.synthesis,
          citations: result.data.citations,
        });

        // The week synthesis belongs on the week's own page, not on whatever
        // page happens to be open.
        if (scope === 'week') {
          const page = await resolveWeekPage(weekPageTitle());
          const saved = await appendToNote(page, block);
          if (saved.id === activeNote.id) {
            updateNote((note: WorkspaceNote) => ({ ...note, content: saved.content }));
          }
          void refetchWeekCandidates();
          void refreshPages();
          notify(
            'success',
            `Written to "${saved.title}".`,
            saved.id === activeNote.id
              ? undefined
              : { label: 'Open', run: () => void selectPage(saved.id) },
          );
          return true;
        }

        updateNote((note: WorkspaceNote) => ({
          ...note,
          content: note.content.trim() ? `${note.content.trim()}\n\n${block}` : block,
          linkedConversationIds: unique([
            ...note.linkedConversationIds,
            ...result.data.conversationIds,
          ]),
        }));
        notify(
          'success',
          `Synthesis complete for ${result.data.entryCount} entr${
            result.data.entryCount === 1 ? 'y' : 'ies'
          }.`,
        );
        return true;
      } catch (error) {
        notify(
          'error',
          error instanceof Error ? error.message : 'Synthesis failed.',
        );
        return false;
      }
    },
    [
      activeNote,
      notify,
      refetchWeekCandidates,
      refreshPages,
      selectPage,
      selectSynthesisTargets,
      updateNote,
    ],
  );

  // Home's "Synthesize last week" row lands here with ?synthesize=week.
  const weekSynthesisRequested = searchParams.get('synthesize') === 'week';
  useEffect(() => {
    if (!weekSynthesisRequested || !activeNote) return;
    const next = new URLSearchParams(searchParams);
    next.delete('synthesize');
    setSearchParams(next, { replace: true });
    void handleSynthesize('week');
    // handleSynthesize is intentionally excluded: this must fire once per
    // arrival, not on every re-derivation of the callback.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [weekSynthesisRequested, activeNote?.id]);

  const journalPaletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'journal.synthesize.week',
        label: 'Synthesize the past week',
        group: 'Journal',
        icon: Combine,
        enabled: (weekCandidates?.total ?? 0) > 0,
        run: () => {
          void handleSynthesize('week');
        },
      },
      {
        id: 'journal.synthesize.pinned',
        label: 'Synthesize pinned entries',
        group: 'Journal',
        icon: Combine,
        enabled: pinnedIds.size > 0,
        run: () => {
          void handleSynthesize('pinned');
        },
      },
      {
        id: 'journal.newEntry',
        label: 'New journal entry',
        group: 'Journal',
        icon: Plus,
        enabled: Boolean(journalSpace),
        run: () => {
          void handleNewEntry();
        },
      },
      {
        id: 'journal.newPage',
        label: 'New journal page',
        group: 'Journal',
        icon: FilePlus2,
        enabled: Boolean(journalSpace),
        run: () => {
          void handleNewPage();
        },
      },
    ],
    [
      handleNewEntry,
      handleNewPage,
      handleSynthesize,
      journalSpace,
      pinnedIds,
      weekCandidates?.total,
    ],
  );
  useRegisterPaletteCommands(journalPaletteCommands);

  // Keyboard shortcuts: J/K navigate, T today, ⌘N new entry, ⌘S save now,
  // ⌘⇧H highlight selection (delegates to highlights strip via selection),
  // ⌘\ toggle sidebar.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isEditable =
        target instanceof HTMLElement &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable);

      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        void saveNow();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === '\\') {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 'n') {
        if (isEditable) return;
        e.preventDefault();
        void handleNewEntry();
        return;
      }
      if (!isEditable) {
        if (e.key === 'j' || e.key === 'ArrowDown') {
          if (entries.length === 0) return;
          const idx = entries.findIndex((en) => en.id === selectedId);
          const nextIdx = idx < 0 ? 0 : Math.min(entries.length - 1, idx + 1);
          setSelectedId(entries[nextIdx].id);
          e.preventDefault();
          return;
        }
        if (e.key === 'k' || e.key === 'ArrowUp') {
          if (entries.length === 0) return;
          const idx = entries.findIndex((en) => en.id === selectedId);
          const nextIdx = idx <= 0 ? 0 : idx - 1;
          setSelectedId(entries[nextIdx].id);
          e.preventDefault();
          return;
        }
        if (e.key === 't' || e.key === 'T') {
          const today = entries.find((en) => {
            const d = new Date(en.updatedAt);
            return !Number.isNaN(d.getTime()) &&
              d.toDateString() === new Date().toDateString();
          });
          if (today) {
            setSelectedId(today.id);
            e.preventDefault();
          }
          return;
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [entries, handleNewEntry, saveNow, selectedId, setSelectedId]);

  // Auto-dismiss notice after 3.5s
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 3500);
    return () => clearTimeout(timer);
  }, [notice]);

  const journalName = journalSpace?.name ?? 'Journal';
  const pinnedEntryCount = [...pinnedIds].length;
  const deckCount = Math.min(entries.length, SYNTHESIS_ENTRY_LIMIT);

  if (topLevelError) {
    return (
      <div className="flex h-full items-center justify-center bg-[hsl(var(--bg))] p-6">
        <div className="max-w-sm text-center">
          <NotebookPen className="mx-auto mb-5 h-8 w-8 text-text-tertiary" strokeWidth={1.5} />
          <h1 className="font-serif text-2xl text-text-primary">Your journal couldn’t load</h1>
          <p className="mt-3 text-sm leading-relaxed text-text-tertiary">Try connecting again to return to your pages.</p>
          <button type="button" onClick={() => { appliedInitialJournalRef.current = false; setJournalLoadAttempt((attempt) => attempt + 1); }} className="mt-6 rounded-lg bg-accent px-5 py-2.5 text-sm font-medium text-accent-fg transition-colors hover:bg-accent-hover">Try again</button>
          <details className="mt-5 text-xs text-text-tertiary">
            <summary className="cursor-pointer">Error details</summary>
            <p className="mt-2 break-words text-left">{topLevelError}</p>
          </details>
        </div>
      </div>
    );
  }

  if (!requestedJournalSpaceId) {
    // First-run empty state (no journal in URL, auto-redirect in flight)
    return (
      <div className="flex h-full items-center justify-center bg-[hsl(var(--bg))] p-6">
        <div className="flex max-w-md flex-col items-center gap-4 text-center">
          <NotebookPen
            className="h-10 w-10 text-[hsl(var(--text-muted))]"
            strokeWidth={1.5}
          />
          <h1 className="font-serif text-xl text-[hsl(var(--text-primary))]">
            Start a journal.
          </h1>
          <p className="text-sm text-[hsl(var(--text-tertiary))]">
            A journal is a notebook of your own thinking. Conversations attached to it
            become entries you can return to.
          </p>
          <button
            type="button"
            onClick={() => void handleCreateJournal()}
            className="inline-flex h-9 items-center gap-1.5 rounded-md bg-[hsl(var(--accent))] px-4 text-sm font-medium text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
          >
            <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
            New journal
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="relative flex h-full overflow-hidden bg-[hsl(var(--bg))] text-[hsl(var(--text-primary))]">
      {sidebarCollapsed ? (
        <aside className="flex h-full w-14 shrink-0 flex-col items-center gap-2 border-r border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] py-2">
          <button
            type="button"
            onClick={() => setSidebarCollapsed(false)}
            className="rounded-sm p-2 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            aria-label="Expand sidebar"
            title="Expand sidebar (⌘\\)"
          >
            <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
          </button>
          <button
            type="button"
            onClick={() => void handleNewEntry()}
            className="rounded-sm p-2 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            aria-label="New entry"
            title="New entry (⌘N)"
          >
            <Plus className="h-4 w-4" strokeWidth={1.75} />
          </button>
        </aside>
      ) : (
        <EntryList
          entriesState={entriesState}
          journals={allJournals}
          currentJournal={journalSpace}
          onSwitchJournal={handleSwitchJournal}
          onCreateJournal={() => void handleCreateJournal()}
          onRenameJournal={handleRenameJournal}
          onDeleteJournal={handleDeleteJournal}
          onNewEntry={() => void handleNewEntry()}
          onRenameEntry={handleRenameEntry}
          onDeleteEntry={handleDeleteEntry}
          onToggleCollapse={() => setSidebarCollapsed(true)}
          pages={pages}
          activePageId={activeNote?.id ?? null}
          onSelectPage={handleSelectPage}
          onNewPage={() => void handleNewPage()}
        />
      )}

      <EntryEditor
        activeNote={activeNote}
        isLoadingNote={isLoadingNote}
        loadError={noteLoadError}
        saveError={saveError}
        hasPendingChanges={hasPendingChanges}
        isSaving={isSavingNow}
        onSaveNow={() => void saveNow()}
        onUpdateNoteContent={handleUpdateNoteContent}
        onAddHighlight={handleAddHighlight}
        onRemoveHighlight={handleRemoveHighlight}
        pinnedHighlightIds={pinnedNoteHighlightIds}
        onTogglePinnedHighlight={handleTogglePinnedHighlight}
        journalName={journalName}
        pageTitle={activeNote?.title ?? null}
        isDefaultPage={
          (activeNote?.title.trim() ?? '') === defaultJournalTitle(journalName)
        }
        selectedEntry={selectedEntry}
        selectedEntryMessages={selectedEntryMessages}
        selectedEntryLoading={selectedEntryLoading}
        pinnedEntryCount={pinnedEntryCount}
        deckCount={deckCount}
        crossEntrySources={sourcesState.summaries}
        crossEntryLoading={sourcesState.isLoading}
        scannedConversationCount={sourcesState.scannedConversationCount}
        onJumpToEntry={(id) => setSelectedId(id)}
        onSynthesize={handleSynthesize}
        weekCandidates={weekCandidates}
        onNotify={notify}
      />

      {notice && (
        <div
          role="status"
          className={`absolute bottom-6 left-1/2 flex -translate-x-1/2 items-center gap-3 rounded-md border px-3 py-1.5 text-xs shadow-md ${
            notice.action ? '' : 'pointer-events-none '
          }${
            notice.tone === 'error'
              ? 'border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]'
              : notice.tone === 'success'
                ? 'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]'
                : 'border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))]'
          }`}
        >
          <span>{notice.message}</span>
          {notice.action && (
            <button
              type="button"
              onClick={() => {
                notice.action?.run();
                setNotice(null);
              }}
              className="shrink-0 font-medium underline-offset-2 hover:underline"
            >
              {notice.action.label}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
