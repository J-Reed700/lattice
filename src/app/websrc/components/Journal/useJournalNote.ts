import { useCallback, useEffect, useRef, useState } from 'react';

import VaultAPI from '@/lib/api';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

function nowIso(): string {
  return new Date().toISOString();
}

function defaultJournalTitle(spaceName: string): string {
  return `Journal · ${spaceName}`;
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
  isLoadingNote: boolean;
  loadError: string | null;
  saveError: string | null;
  hasPendingChanges: boolean;
  isSavingNow: boolean;
  updateNote: (updater: (note: WorkspaceNote) => WorkspaceNote) => void;
  saveNow: () => Promise<boolean>;
  deleteActiveNote: () => Promise<boolean>;
}

const DEBOUNCE_MS = 450;

/**
 * Owns the per-journal notebook WorkspaceNote lifecycle: load, debounced
 * autosave, flush on unload.
 */
export function useJournalNote(options: {
  journalSpaceId: string | null;
  journalName: string | null;
}): UseJournalNoteResult {
  const { journalSpaceId, journalName } = options;

  const [activeNote, setActiveNote] = useState<WorkspaceNote | null>(null);
  const [isLoadingNote, setIsLoadingNote] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [hasPendingChanges, setHasPendingChanges] = useState(false);
  const [isSavingNow, setIsSavingNow] = useState(false);

  const noteRef = useRef<WorkspaceNote | null>(null);
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dirtyRef = useRef<boolean>(false);

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

      const storageKey = `journal.noteBySpace.${journalSpaceId}`;
      let storedNoteId: string | null = null;
      try {
        storedNoteId = localStorage.getItem(storageKey);
      } catch {
        storedNoteId = null;
      }

      let target: WorkspaceNote | null = storedNoteId
        ? listResult.data.notes.find((n) => n.id === storedNoteId) ?? null
        : null;

      if (!target && journalName) {
        const expected = defaultJournalTitle(journalName);
        target = listResult.data.notes.find((n) => n.title.trim() === expected) ?? null;
      }

      if (!target) {
        const titleBase = journalName ?? inferInitialJournalNameFromNotes(listResult.data.notes) ?? 'Journal';
        const created = await VaultAPI.createWorkspaceNote(defaultJournalTitle(titleBase));
        if (cancelled) return;
        if (!created.ok) {
          setLoadError(created.error);
          setIsLoadingNote(false);
          return;
        }
        target = created.data;
      }

      try {
        localStorage.setItem(storageKey, target.id);
      } catch {
        // Ignore localStorage failures in constrained environments.
      }

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
  }, [journalSpaceId, journalName]);

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
    return true;
  }, [clearPersistTimer]);

  return {
    activeNote,
    isLoadingNote,
    loadError,
    saveError,
    hasPendingChanges,
    isSavingNow,
    updateNote,
    saveNow,
    deleteActiveNote,
  };
}
