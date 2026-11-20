import { useCallback, useEffect, useMemo, useState } from 'react';

import type { WorkspaceNote } from '@/types/api/dailyNotes';

const SIDEBAR_COLLAPSED_KEY = 'journal.sidebar.collapsed';
const EMPTY_HIGHLIGHT_IDS = new Set<string>();

type PinnedHighlightsByNote = Record<string, string[]>;

interface PinnedHighlightsState {
  spaceId: string | null;
  byNote: PinnedHighlightsByNote;
  dirty: boolean;
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

function readPinnedNoteHighlights(spaceId: string | null): PinnedHighlightsByNote {
  if (!spaceId) return {};
  try {
    const raw = localStorage.getItem(`journal.pinnedNoteHighlights.${spaceId}`);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const result: PinnedHighlightsByNote = {};
    for (const [noteId, ids] of Object.entries(parsed)) {
      if (!Array.isArray(ids)) continue;
      const valid = Array.from(new Set(ids.filter((v): v is string => typeof v === 'string')));
      if (valid.length > 0) {
        result[noteId] = valid;
      }
    }
    return result;
  } catch {
    return {};
  }
}

function writePinnedNoteHighlights(spaceId: string, byNote: PinnedHighlightsByNote): void {
  try {
    localStorage.setItem(
      `journal.pinnedNoteHighlights.${spaceId}`,
      JSON.stringify(byNote),
    );
  } catch {
    // Ignore
  }
}

function noteBelongsToJournal(note: WorkspaceNote | null, spaceId: string | null): note is WorkspaceNote {
  if (!note || !spaceId) return false;
  return !note.journalId || note.journalId === spaceId;
}

/** Whether the journal sidebar is collapsed, remembered across sessions. */
export function useSidebarCollapsed() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);

  // Persist sidebar collapsed state
  useEffect(() => {
    writeSidebarCollapsed(sidebarCollapsed);
  }, [sidebarCollapsed]);

  return [sidebarCollapsed, setSidebarCollapsed] as const;
}

/** The active page's pinned highlights, remembered per page inside each journal. */
export function usePinnedNoteHighlights(
  requestedJournalSpaceId: string | null,
  activeNote: WorkspaceNote | null,
) {
  const [state, setState] = useState<PinnedHighlightsState>(() => ({
    spaceId: requestedJournalSpaceId,
    byNote: readPinnedNoteHighlights(requestedJournalSpaceId),
    dirty: false,
  }));

  // Load pinned highlights when the active journal changes; tracking `spaceId`
  // prevents the write effect from flushing the prior journal into the new key.
  useEffect(() => {
    setState((current) =>
      current.spaceId === requestedJournalSpaceId
        ? current
        : {
            spaceId: requestedJournalSpaceId,
            byNote: readPinnedNoteHighlights(requestedJournalSpaceId),
            dirty: false,
          },
    );
  }, [requestedJournalSpaceId]);

  useEffect(() => {
    if (!requestedJournalSpaceId || !state.dirty || state.spaceId !== requestedJournalSpaceId) {
      return;
    }
    writePinnedNoteHighlights(requestedJournalSpaceId, state.byNote);
  }, [requestedJournalSpaceId, state]);

  // Prune removed highlights only on the active note itself, leaving other pages' pins intact.
  useEffect(() => {
    if (!noteBelongsToJournal(activeNote, requestedJournalSpaceId)) return;
    const valid = new Set(activeNote.highlights.map((h) => h.id));
    setState((current) => {
      if (current.spaceId !== requestedJournalSpaceId) return current;
      const existing = current.byNote[activeNote.id];
      if (!existing || existing.length === 0) return current;
      const filtered = existing.filter((id) => valid.has(id));
      if (filtered.length === existing.length) return current;
      const nextByNote = { ...current.byNote };
      if (filtered.length === 0) {
        delete nextByNote[activeNote.id];
      } else {
        nextByNote[activeNote.id] = filtered;
      }
      return { spaceId: current.spaceId, byNote: nextByNote, dirty: true };
    });
  }, [activeNote, requestedJournalSpaceId]);

  const activeNoteId = noteBelongsToJournal(activeNote, requestedJournalSpaceId)
    ? activeNote.id
    : null;

  const handleTogglePinnedHighlight = useCallback(
    (highlightId: string) => {
      if (!activeNoteId || !requestedJournalSpaceId) return;
      setState((current) => {
        if (current.spaceId !== requestedJournalSpaceId) return current;
        const nextSet = new Set(current.byNote[activeNoteId] ?? []);
        if (nextSet.has(highlightId)) nextSet.delete(highlightId);
        else nextSet.add(highlightId);
        const nextByNote = { ...current.byNote };
        if (nextSet.size === 0) {
          delete nextByNote[activeNoteId];
        } else {
          nextByNote[activeNoteId] = [...nextSet];
        }
        return { spaceId: current.spaceId, byNote: nextByNote, dirty: true };
      });
    },
    [activeNoteId, requestedJournalSpaceId],
  );

  const pinnedNoteHighlightIds = useMemo(() => {
    if (!activeNoteId || !requestedJournalSpaceId || state.spaceId !== requestedJournalSpaceId) {
      return EMPTY_HIGHLIGHT_IDS;
    }
    return new Set(state.byNote[activeNoteId] ?? []);
  }, [activeNoteId, requestedJournalSpaceId, state.byNote, state.spaceId]);

  return { pinnedNoteHighlightIds, handleTogglePinnedHighlight };
}
