import { useCallback, useEffect, useRef, useState } from 'react';

import VaultAPI from '@/lib/api';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

function nowIso(): string {
  return new Date().toISOString();
}

function defaultJournalTitle(spaceName: string): string {
  return `Journal · ${spaceName}`;
}

function storageKeyFor(spaceId: string): string {
  return `journal.noteBySpace.${spaceId}`;
}

/**
 * Which page this journal was last on. A remembered selection is a UI
 * preference, not state the backend reads, so localStorage is the right home
 * for it (CLAUDE.md rule 3).
 */
function readRememberedNoteId(spaceId: string): string | null {
  try {
    return localStorage.getItem(storageKeyFor(spaceId));
  } catch {
    return null;
  }
}

function rememberNoteId(spaceId: string, noteId: string): void {
  try {
    localStorage.setItem(storageKeyFor(spaceId), noteId);
  } catch {
    // Ignore localStorage failures in constrained environments.
  }
}

function timestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

/** Most recently updated first, with a total tie-break so it never reshuffles. */
function sortPages(notes: WorkspaceNote[]): WorkspaceNote[] {
  return [...notes].sort((a, b) => {
    const delta = timestamp(b.updatedAt) - timestamp(a.updatedAt);
    if (delta !== 0) return delta;
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

function inferInitialJournalNameFromNotes(notes: WorkspaceNote[]): string | null {
  const firstNamed = notes
    .map((note) => note.title.trim())
    .find((title) => title.length > 0);
  if (!firstNamed) return null;
  const lower = firstNamed.toLowerCase();
  if (lower.startsWith('journal · ')) return null;
  if (lower.startsWith('note ')) return null;
  return firstNamed;
}

export interface UseJournalNoteResult {
  activeNote: WorkspaceNote | null;
  /** Every page in the workspace, most recently updated first. */
  pages: WorkspaceNote[];
  isLoadingNote: boolean;
  loadError: string | null;
  saveError: string | null;
  hasPendingChanges: boolean;
  isSavingNow: boolean;
  updateNote: (updater: (note: WorkspaceNote) => WorkspaceNote) => void;
  saveNow: () => Promise<boolean>;
  deleteActiveNote: () => Promise<boolean>;
  /** Opens a page, flushing any unsaved edits on the one being left. */
  selectPage: (noteId: string) => Promise<void>;
  /** Creates a page and opens it. */
  createPage: (title: string) => Promise<WorkspaceNote | null>;
  /** Re-reads the page list (after a synthesis writes one, say). */
  refreshPages: () => Promise<WorkspaceNote[]>;
}

const DEBOUNCE_MS = 450;

/**
 * Owns the per-journal notebook WorkspaceNote lifecycle: load, debounced
 * autosave, flush on unload.
 */
export function useJournalNote(options: {
  journalSpaceId: string | null;
  journalName: string | null;
  /** A page to open on arrival (`?noteId=…`). Applied once, then the user owns
   *  the selection — the same rule the reference inbox follows. */
  requestedNoteId?: string | null;
}): UseJournalNoteResult {
  const { journalSpaceId, journalName, requestedNoteId = null } = options;

  const [activeNote, setActiveNote] = useState<WorkspaceNote | null>(null);
  const [pages, setPages] = useState<WorkspaceNote[]>([]);
  const [isLoadingNote, setIsLoadingNote] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [hasPendingChanges, setHasPendingChanges] = useState(false);
  const [isSavingNow, setIsSavingNow] = useState(false);

  const noteRef = useRef<WorkspaceNote | null>(null);
  const pagesRef = useRef<WorkspaceNote[]>([]);
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dirtyRef = useRef<boolean>(false);
  const appliedRequestRef = useRef<string | null>(null);

  useEffect(() => {
    noteRef.current = activeNote;
  }, [activeNote]);

  const clearPersistTimer = useCallback(() => {
    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
      persistTimerRef.current = null;
    }
  }, []);

  const persistNote = useCallback(async (note: WorkspaceNote): Promise<boolean> => {
    const result = await VaultAPI.updateWorkspaceNote(note);
    if (!result.ok) {
      setSaveError(result.error);
      return false;
    }
    setSaveError(null);
    dirtyRef.current = false;
    setHasPendingChanges(false);
    setActiveNote((current) => (current?.id === result.data.id ? result.data : current));
    noteRef.current = result.data;
    // Keep the sidebar's page list honest: a save moves the page to the top of
    // "most recently updated" and may have renamed it.
    setPages((current) => {
      const next = sortPages(
        current.some((page) => page.id === result.data.id)
          ? current.map((page) => (page.id === result.data.id ? result.data : page))
          : [...current, result.data],
      );
      pagesRef.current = next;
      return next;
    });
    return true;
  }, []);

  const saveNow = useCallback(async (): Promise<boolean> => {
    if (!dirtyRef.current || !noteRef.current) return true;
    setIsSavingNow(true);
    clearPersistTimer();
    const ok = await persistNote(noteRef.current);
    setIsSavingNow(false);
    return ok;
  }, [clearPersistTimer, persistNote]);

  const updateNote = useCallback(
    (updater: (note: WorkspaceNote) => WorkspaceNote) => {
      const current = noteRef.current;
      if (!current) return;
      const updated = { ...updater(current), updatedAt: nowIso() };
      noteRef.current = updated;
      setActiveNote(updated);

      dirtyRef.current = true;
      setHasPendingChanges(true);

      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
      }
      persistTimerRef.current = setTimeout(async () => {
        persistTimerRef.current = null;
        const latest = noteRef.current;
        if (!latest) return;
        await persistNote(latest);
      }, DEBOUNCE_MS);
    },
    [persistNote],
  );

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      if (!journalSpaceId) {
        setActiveNote(null);
        setIsLoadingNote(false);
        return;
      }

      setIsLoadingNote(true);
      setLoadError(null);

      const listResult = await VaultAPI.listWorkspaceNotes();
      if (cancelled) return;
      if (!listResult.ok) {
        setLoadError(listResult.error);
        setIsLoadingNote(false);
        return;
      }

      const allPages = sortPages(listResult.data.notes);
      setPages(allPages);
      pagesRef.current = allPages;

      let target: WorkspaceNote | null = null;

      // A deep link ("open the page the synthesis just wrote") wins, once.
      if (requestedNoteId && requestedNoteId !== appliedRequestRef.current) {
        const requested = allPages.find((n) => n.id === requestedNoteId) ?? null;
        if (requested) {
          appliedRequestRef.current = requestedNoteId;
          target = requested;
        }
      }

      const storedNoteId = readRememberedNoteId(journalSpaceId);
      if (!target && storedNoteId) {
        target = allPages.find((n) => n.id === storedNoteId) ?? null;
      }

      if (!target && journalName) {
        const expected = defaultJournalTitle(journalName);
        target = allPages.find((n) => n.title.trim() === expected) ?? null;
      }

      if (!target) {
        const titleBase = journalName ?? inferInitialJournalNameFromNotes(allPages) ?? 'Journal';
        const created = await VaultAPI.createWorkspaceNote(defaultJournalTitle(titleBase));
        if (cancelled) return;
        if (!created.ok) {
          setLoadError(created.error);
          setIsLoadingNote(false);
          return;
        }
        target = created.data;
        const withCreated = sortPages([...allPages, created.data]);
        setPages(withCreated);
        pagesRef.current = withCreated;
      }

      rememberNoteId(journalSpaceId, target.id);

      setActiveNote(target);
      noteRef.current = target;
      dirtyRef.current = false;
      setHasPendingChanges(false);
      setIsLoadingNote(false);
    };

    void load();

    return () => {
      cancelled = true;
    };
  }, [journalSpaceId, journalName, requestedNoteId]);

  useEffect(() => {
    const flush = () => {
      if (dirtyRef.current && noteRef.current) {
        void persistNote(noteRef.current);
      }
    };
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') flush();
    };
    window.addEventListener('beforeunload', flush);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.removeEventListener('beforeunload', flush);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [persistNote]);

  useEffect(
    () => () => {
      if (dirtyRef.current && noteRef.current) {
        void persistNote(noteRef.current);
      }
      clearPersistTimer();
    },
    [clearPersistTimer, persistNote],
  );

  const refreshPages = useCallback(async (): Promise<WorkspaceNote[]> => {
    const result = await VaultAPI.listWorkspaceNotes();
    if (!result.ok) return pagesRef.current;
    const sorted = sortPages(result.data.notes);
    setPages(sorted);
    pagesRef.current = sorted;
    return sorted;
  }, []);

  /** Makes `note` the page being edited, remembering it for this journal. */
  const openPage = useCallback(
    (note: WorkspaceNote) => {
      if (journalSpaceId) rememberNoteId(journalSpaceId, note.id);
      clearPersistTimer();
      setActiveNote(note);
      noteRef.current = note;
      dirtyRef.current = false;
      setHasPendingChanges(false);
      setSaveError(null);
    },
    [clearPersistTimer, journalSpaceId],
  );

  const selectPage = useCallback(
    async (noteId: string): Promise<void> => {
      if (noteRef.current?.id === noteId) return;
      // Leaving a page must not drop its unsaved edits.
      await saveNow();
      // The page may have been written since it was listed (a synthesis, a
      // capture), so read it fresh rather than trusting the cached copy.
      const fresh = await refreshPages();
      const target = fresh.find((page) => page.id === noteId) ?? null;
      if (!target) {
        setSaveError('That page is no longer here.');
        return;
      }
      openPage(target);
    },
    [openPage, refreshPages, saveNow],
  );

  const createPage = useCallback(
    async (title: string): Promise<WorkspaceNote | null> => {
      await saveNow();
      const created = await VaultAPI.createWorkspaceNote(title);
      if (!created.ok) {
        setSaveError(created.error);
        return null;
      }
      const next = sortPages([...pagesRef.current, created.data]);
      setPages(next);
      pagesRef.current = next;
      openPage(created.data);
      return created.data;
    },
    [openPage, saveNow],
  );

  const deleteActiveNote = useCallback(async (): Promise<boolean> => {
    const current = noteRef.current;
    if (!current) return false;
    clearPersistTimer();
    const result = await VaultAPI.deleteWorkspaceNote(current.id);
    if (!result.ok) {
      setSaveError(result.error);
      return false;
    }
    dirtyRef.current = false;
    setHasPendingChanges(false);
    setActiveNote(null);
    noteRef.current = null;
    const remaining = pagesRef.current.filter((page) => page.id !== current.id);
    setPages(remaining);
    pagesRef.current = remaining;
    return true;
  }, [clearPersistTimer]);

  return {
    activeNote,
    pages,
    isLoadingNote,
    loadError,
    saveError,
    hasPendingChanges,
    isSavingNow,
    updateNote,
    saveNow,
    deleteActiveNote,
    selectPage,
    createPage,
    refreshPages,
  };
}
