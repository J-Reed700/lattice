import { useCallback, useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { createJournal, updateJournal, deleteJournal, journalsQueryOptions } from '@/features/journal/api/queries';
import { defaultJournalTitle } from '@/features/journal/hooks/useJournalNote';
import type { JournalNotify } from '@/features/journal/hooks/useJournalNotice';
import VaultAPI from '@/lib/api';
import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import type { NavigateFunction, SetURLSearchParams } from 'react-router';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const DEFAULT_JOURNAL_ICON = '📓';
const DEFAULT_JOURNAL_ACCENT = '#aa503d'; // Default notebook cover accent; saved custom colors are preserved.

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

export function useFollowNoteIdParam(options: {
  requestedNoteIdParam: string | null;
  searchParams: URLSearchParams;
  setSearchParams: SetURLSearchParams;
  setRequestedNoteId: (noteId: string | null) => void;
}) {
  const { requestedNoteIdParam, searchParams, setSearchParams, setRequestedNoteId } = options;

  // Take `?noteId=` off the URL once it has been handed to the note hook, and
  // move to the journal that owns the page. Pages are listed per journal, so a
  // link that arrives while another journal is open would find nothing there —
  // the "saved" toast pointing at a page the screen then fails to show.
  useEffect(() => {
    if (!requestedNoteIdParam) return;
    let cancelled = false;

    const follow = async () => {
      const notesResult = await VaultAPI.listWorkspaceNotes();
      if (cancelled) return;
      const owningJournalId = notesResult.ok
        ? (notesResult.data.notes.find((note) => note.id === requestedNoteIdParam)?.journalId ?? null)
        : null;

      setRequestedNoteId(requestedNoteIdParam);
      const next = new URLSearchParams(searchParams);
      next.delete('noteId');
      if (owningJournalId && owningJournalId !== next.get('journalSpaceId')) {
        next.set('journalSpaceId', owningJournalId);
        next.delete('entryId');
      }
      setSearchParams(next, { replace: true });
    };

    void follow();
    return () => {
      cancelled = true;
    };
  }, [requestedNoteIdParam, searchParams, setSearchParams, setRequestedNoteId]);
}

/**
 * The journal list lifecycle: keeps a journal selected in the URL (creating
 * the first one on first run), and creates, switches, renames and deletes
 * journals.
 */
export function useJournalSpaces(options: {
  requestedJournalSpaceId: string | null;
  journalSpace: ConversationJournalDto | null;
  activeNote: WorkspaceNote | null;
  updateNote: (updater: (note: WorkspaceNote) => WorkspaceNote) => void;
  searchParams: URLSearchParams;
  navigate: NavigateFunction;
  notify: JournalNotify;
}) {
  const { requestedJournalSpaceId, journalSpace, activeNote, updateNote, searchParams, navigate, notify } = options;
  const queryClient = useQueryClient();
  const [topLevelError, setTopLevelError] = useState<string | null>(null);
  const [journalLoadAttempt, setJournalLoadAttempt] = useState(0);
  const [isCreatingJournal, setIsCreatingJournal] = useState(false);
  const initialJournalHandledRef = useRef(false);
  const journalCreationRef = useRef<ReturnType<typeof VaultAPI.createJournal> | null>(null);

  const createJournalOnce = useCallback(() => {
    if (journalCreationRef.current) return journalCreationRef.current;
    const request = (async () => {
      try {
        const journals = await queryClient.fetchQuery(journalsQueryOptions());
        return createJournal(queryClient, {
          name: nextJournalName(journals),
          description: null,
          icon: DEFAULT_JOURNAL_ICON,
          accentColor: DEFAULT_JOURNAL_ACCENT,
          spacePrompt: null,
          defaultModelName: null,
          toolPreferencesJson: null,
        });
      } catch (error) {
        return { ok: false as const, error: error instanceof Error ? error.message : String(error) };
      }
    })();
    journalCreationRef.current = request;
    void request.then(
      () => { if (journalCreationRef.current === request) journalCreationRef.current = null; },
      () => { if (journalCreationRef.current === request) journalCreationRef.current = null; },
    );
    return request;
  }, [queryClient]);

  // Auto-redirect: ensure URL always has a journalSpaceId when one exists
  useEffect(() => {
    let cancelled = false;

    const ensureJournal = async () => {
      setTopLevelError(null);
      const fetched = await queryClient.fetchQuery(journalsQueryOptions());
      if (cancelled) return;
      const active = fetched.filter(journal => !journal.isArchived);
      if (fetched.length > 0) initialJournalHandledRef.current = true;

      if (requestedJournalSpaceId) {
        const requestedIsActive = active.some((j) => j.id === requestedJournalSpaceId);
        if (requestedIsActive) {
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

      // Opening Journal again removes the selection from the URL without
      // unmounting this workspace. Resolve it on every such navigation;
      // createJournalOnce already shares any pending first-journal creation.
      if (active.length === 0 && fetched.length === 0) {
        // Deleting the last journal should leave the workspace empty. A
        // navigation during first-run creation may still await that operation.
        if (initialJournalHandledRef.current && !journalCreationRef.current) return;
        initialJournalHandledRef.current = true;
        const created = await createJournalOnce();
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

    void ensureJournal().catch((error: unknown) => {
      if (!cancelled) setTopLevelError(error instanceof Error ? error.message : String(error));
    });
    return () => {
      cancelled = true;
    };
  }, [queryClient, createJournalOnce, requestedJournalSpaceId, navigate, searchParams, journalLoadAttempt]);

  const retryJournalLoad = useCallback(() => {
    initialJournalHandledRef.current = false;
    setJournalLoadAttempt((attempt) => attempt + 1);
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
    if (isCreatingJournal) return;
    setIsCreatingJournal(true);
    try {
      const created = await createJournalOnce();
      if (!created.ok) {
        notify('error', `Failed to create journal: ${created.error}`);
        return;
      }
      const params = new URLSearchParams(searchParams);
      params.set('journalSpaceId', created.data.id);
      params.delete('entryId');
      navigate(`/journals?${params.toString()}`);
      notify('success', `Created "${created.data.name}".`);
    } catch (error) {
      notify('error', `Failed to create journal: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setIsCreatingJournal(false);
    }
  }, [createJournalOnce, isCreatingJournal, navigate, notify, searchParams]);

  const handleRenameJournal = useCallback(
    async (nextName: string) => {
      if (!requestedJournalSpaceId || !journalSpace) return;
      const result = await updateJournal(queryClient, {
        journalId: requestedJournalSpaceId,
        name: nextName,
      });
      if (!result.ok) {
        notify('error', result.error);
        return;
      }
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
    [queryClient, activeNote, journalSpace, notify, requestedJournalSpaceId, updateNote],
  );

  const handleDeleteJournal = useCallback(async () => {
    if (!requestedJournalSpaceId || !journalSpace) return;
    const result = await deleteJournal(queryClient, { journalId: requestedJournalSpaceId });
    if (!result.ok) {
      notify('error', result.error);
      return;
    }
    try {
      localStorage.removeItem(`journal.noteBySpace.${requestedJournalSpaceId}`);
      localStorage.removeItem(`journal.pinnedBookmarks.${requestedJournalSpaceId}`);
      localStorage.removeItem(`journal.pinnedNoteHighlights.${requestedJournalSpaceId}`);
      const last = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
      if (last === requestedJournalSpaceId) {
        localStorage.removeItem(LAST_JOURNAL_SPACE_KEY);
      }
    } catch {
      // Ignore
    }
    const remaining = (await queryClient.fetchQuery(journalsQueryOptions())).filter(journal => !journal.isArchived);
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
  }, [queryClient, journalSpace, navigate, notify, requestedJournalSpaceId, searchParams]);

  return {
    topLevelError,
    retryJournalLoad,
    isCreatingJournal,
    handleSwitchJournal,
    handleCreateJournal,
    handleRenameJournal,
    handleDeleteJournal,
  };
}
