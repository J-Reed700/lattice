import { useCallback, useEffect, useState } from 'react';

import type { WorkspaceNote } from '@/types/api/dailyNotes';

const SIDEBAR_COLLAPSED_KEY = 'journal.sidebar.collapsed';

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

/** Whether the journal sidebar is collapsed, remembered across sessions. */
export function useSidebarCollapsed() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);

  // Persist sidebar collapsed state
  useEffect(() => {
    writeSidebarCollapsed(sidebarCollapsed);
  }, [sidebarCollapsed]);

  return [sidebarCollapsed, setSidebarCollapsed] as const;
}

/** The active page's pinned highlights, remembered per journal. */
export function usePinnedNoteHighlights(
  requestedJournalSpaceId: string | null,
  activeNote: WorkspaceNote | null,
) {
  const [pinnedNoteHighlightIds, setPinnedNoteHighlightIds] = useState<Set<string>>(
    new Set(),
  );

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

  const handleTogglePinnedHighlight = useCallback((highlightId: string) => {
    setPinnedNoteHighlightIds((current) => {
      const next = new Set(current);
      if (next.has(highlightId)) next.delete(highlightId);
      else next.add(highlightId);
      return next;
    });
  }, []);

  return { pinnedNoteHighlightIds, handleTogglePinnedHighlight };
}
