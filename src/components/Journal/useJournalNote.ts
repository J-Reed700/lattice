import { useCallback, useEffect, useRef, useState } from 'react';

import VaultAPI from '@/lib/api';
import { registerPendingSave } from '@/lib/pendingSaves';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

function nowIso(): string {
  return new Date().toISOString();
}

/** Title given to a journal's first page by earlier builds; shown as untitled. */
export function defaultJournalTitle(spaceName: string): string {
  return `Journal · ${spaceName}`;
}

function storageKeyFor(spaceId: string): string {
  return `journal.noteBySpace.${spaceId}`;
}

/**
 * Which page this journal was last on. A remembered selection is a UI
 * preference, not state the backend reads, so localStorage is the right home.
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
  /** Renames any page, open or not. */
  renamePage: (noteId: string, title: string) => Promise<boolean>;
  /** Deletes any page. Deleting the open one opens the next most recent. */
  deletePage: (noteId: string) => Promise<boolean>;
  /** Re-reads the page list (after a synthesis writes one, say). */
  refreshPages: () => Promise<WorkspaceNote[]>;
}

const DEBOUNCE_MS = 450;

/** What a page is called until its writer names it. */
export const UNTITLED_PAGE = 'Untitled page';

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

  const saveQueueRef = useRef<Promise<boolean>>(Promise.resolve(true));

  const clearPersistTimer = useCallback(() => {
    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
      persistTimerRef.current = null;
    }
  }, []);

  const persistNote = useCallback(async (note: WorkspaceNote): Promise<boolean> => {
    const write = async (): Promise<boolean> => {
      const result = await VaultAPI.updateWorkspaceNote(note);
      if (!result.ok) {
        setSaveError(result.error);
        return false;
      }
      // A response acknowledges only the snapshot sent. Typing while IPC is
      // pending must remain dirty and must never be replaced by that response.
      if (noteRef.current === note) {
        setSaveError(null);
        dirtyRef.current = false;
        setHasPendingChanges(false);
        setActiveNote(result.data);
        noteRef.current = result.data;
      }
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
    };
    // Serialize writes so an older request cannot land after a newer one.
    const queued = saveQueueRef.current.then(write, write).catch((error: unknown) => {
      setSaveError(error instanceof Error ? error.message : 'Could not save this page.');
      return false;
    });
    saveQueueRef.current = queued;
    return queued;
  }, []);

  const saveNow = useCallback(async (): Promise<boolean> => {
    if (!dirtyRef.current || !noteRef.current) return true;
    setIsSavingNow(true);
    clearPersistTimer();
    try {
      while (dirtyRef.current && noteRef.current) {
        if (!(await persistNote(noteRef.current))) return false;
      }
      return true;
    } finally {
      clearPersistTimer();
      setIsSavingNow(false);
    }
  }, [clearPersistTimer, persistNote]);

  useEffect(() => registerPendingSave(saveNow), [saveNow]);

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
      // Renaming or changing journal scope can retrigger this effect while
      // the current editor still owns a draft.
      if (dirtyRef.current && !(await saveNow())) return;
      if (cancelled) return;
      if (!journalSpaceId) {
        setActiveNote(null);
        setIsLoadingNote(false);
        return;
      }

      setIsLoadingNote(true);
      setLoadError(null);

      const listResult = await VaultAPI.listWorkspaceNotes(journalSpaceId);
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

      // A journal with pages never needs another made for it: renaming the
      // journal, or its first page, used to mint a fresh "Journal · …" page here.
      if (!target && allPages.length > 0) {
        target = allPages[0];
      }

      if (!target) {
        const created = await VaultAPI.createWorkspaceNote(UNTITLED_PAGE, journalSpaceId);
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
  }, [journalSpaceId, journalName, requestedNoteId, saveNow]);

  useEffect(() => {
    const flush = () => {
      if (dirtyRef.current && noteRef.current) {
        void persistNote(noteRef.current);
      }
    };
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') flush();
    };
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (!dirtyRef.current) return;
      event.preventDefault();
      event.returnValue = '';
      flush();
    };
    window.addEventListener('beforeunload', beforeUnload);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.removeEventListener('beforeunload', beforeUnload);
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
    if (!journalSpaceId) return pagesRef.current;
    const result = await VaultAPI.listWorkspaceNotes(journalSpaceId);
    if (!result.ok) return pagesRef.current;
    const sorted = sortPages(result.data.notes);
    setPages(sorted);
    pagesRef.current = sorted;
    return sorted;
  }, [journalSpaceId]);

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
      if (!(await saveNow())) return;
      // The page may have been written since it was listed (a synthesis, a
      // capture), so read it fresh rather than trusting the cached copy.
      const fresh = await refreshPages();
      const target = fresh.find((page) => page.id === noteId) ?? null;
      if (!target) {
        setSaveError('That page is no longer here.');
        return;
      }
      if (!(await saveNow())) return;
      openPage(target);
    },
    [openPage, refreshPages, saveNow],
  );

  const createPage = useCallback(
    async (title: string): Promise<WorkspaceNote | null> => {
      if (!(await saveNow())) return null;
      const created = await VaultAPI.createWorkspaceNote(title, journalSpaceId ?? undefined);
      if (!created.ok) {
        setSaveError(created.error);
        return null;
      }
      const next = sortPages([...pagesRef.current, created.data]);
      setPages(next);
      pagesRef.current = next;
      if (!(await saveNow())) return null;
      openPage(created.data);
      return created.data;
    },
    [journalSpaceId, openPage, saveNow],
  );

  const renamePage = useCallback(
    async (noteId: string, title: string): Promise<boolean> => {
      const next = title.trim() || UNTITLED_PAGE;
      if (noteRef.current?.id === noteId) {
        if (noteRef.current.title === next) return true;
        updateNote((note) => ({ ...note, title: next }));
        return saveNow();
      }
      const page = pagesRef.current.find((candidate) => candidate.id === noteId);
      if (!page) return false;
      if (page.title === next) return true;
      const result = await VaultAPI.updateWorkspaceNote({ ...page, title: next });
      if (!result.ok) {
        setSaveError(result.error);
        return false;
      }
      const updated = sortPages(
        pagesRef.current.map((candidate) => (candidate.id === noteId ? result.data : candidate)),
      );
      setPages(updated);
      pagesRef.current = updated;
      return true;
    },
    [saveNow, updateNote],
  );

  const deletePage = useCallback(
    async (noteId: string): Promise<boolean> => {
      const wasOpen = noteRef.current?.id === noteId;
      if (wasOpen) clearPersistTimer();
      const result = await VaultAPI.deleteWorkspaceNote(noteId);
      if (!result.ok) {
        setSaveError(result.error);
        return false;
      }
      const remaining = pagesRef.current.filter((page) => page.id !== noteId);
      setPages(remaining);
      pagesRef.current = remaining;
      if (wasOpen) {
        dirtyRef.current = false;
        setHasPendingChanges(false);
        const next = remaining[0] ?? null;
        if (next) {
          openPage(next);
        } else {
          setActiveNote(null);
          noteRef.current = null;
        }
      }
      return true;
    },
    [clearPersistTimer, openPage],
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
    renamePage,
    deletePage,
    refreshPages,
  };
}
