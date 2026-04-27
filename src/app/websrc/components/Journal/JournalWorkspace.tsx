import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { NotebookPen, PanelLeft, Plus } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { EntryEditor } from './EntryEditor';
import { EntryList } from './EntryList';
import { useJournalEntries } from './useJournalEntries';
import { useJournalNote } from './useJournalNote';
import { useJournalSources } from './useJournalSources';

import type { SynthesisScope } from './SynthesizePopover';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const SIDEBAR_COLLAPSED_KEY = 'journal.sidebar.collapsed';
const DEFAULT_JOURNAL_ICON = '📓';
const DEFAULT_JOURNAL_ACCENT = '#14b8a6';
const SYNTHESIS_ENTRY_LIMIT = 12;

type ActionTone = 'info' | 'success' | 'error';

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

function createSynthesisBlock(
  scope: SynthesisScope,
  entryCount: number,
  synthesis: string,
): string {
  const scopeLabel =
    scope === 'current'
      ? 'Current Entry'
      : scope === 'pinned'
        ? 'Pinned Entries'
        : 'Entry Deck';
  const generatedAt = new Date().toLocaleString();
  return [
    `## Journal Synthesis · ${scopeLabel}`,
    `_Generated ${generatedAt} from ${entryCount} entr${entryCount === 1 ? 'y' : 'ies'}._`,
    '',
    synthesis.trim(),
    '',
  ].join('\n');
}

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
  const [searchParams] = useSearchParams();
  const requestedJournalSpaceId = searchParams.get('journalSpaceId');
  const requestedEntryId = searchParams.get('entryId');

  const [allJournals, setAllJournals] = useState<ConversationJournalDto[]>([]);
  const [journalSpace, setJournalSpace] = useState<ConversationJournalDto | null>(null);
  const [topLevelError, setTopLevelError] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);
  const [notice, setNotice] = useState<{ tone: ActionTone; message: string } | null>(null);
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
  });
  const {
    activeNote,
    isLoadingNote,
    loadError: noteLoadError,
    saveError,
    hasPendingChanges,
    isSavingNow,
    updateNote,
    saveNow,
  } = noteState;

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
  }, [requestedJournalSpaceId, navigate, searchParams]);

  // Action handlers
  const notify = useCallback((tone: ActionTone, message: string) => {
    setNotice({ tone, message });
  }, []);

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

      // Update the notebook title if it was the default
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
      if (targets.length === 0) {
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
        const block = createSynthesisBlock(scope, result.data.entryCount, result.data.synthesis);
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
    [activeNote, notify, selectSynthesisTargets, updateNote],
  );

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
        <p className="max-w-md text-center text-sm text-[hsl(var(--danger-fg))]">
          {topLevelError}
        </p>
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
        onNotify={notify}
      />

      {notice && (
        <div
          role="status"
          className={`pointer-events-none absolute bottom-6 left-1/2 -translate-x-1/2 rounded-md border px-3 py-1.5 text-xs shadow-md ${
            notice.tone === 'error'
              ? 'border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]'
              : notice.tone === 'success'
                ? 'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]'
                : 'border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))]'
          }`}
        >
          {notice.message}
        </div>
      )}
    </div>
  );
}
