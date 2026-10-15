import { useCallback, useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import type { SynthesisScope } from '@/features/journal/components/SynthesizePopover';
import type { JournalEntrySummary } from '@/features/journal/hooks/useJournalEntries';
import type { JournalNotify } from '@/features/journal/hooks/useJournalNotice';
import { weekPageTitle } from '@/features/journal/model/synthesisTargets';
import { runSynthesis } from '@/features/journal/synthesis/runSynthesis';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import type { SetURLSearchParams } from 'react-router';

export const SYNTHESIS_ENTRY_LIMIT = 12;

const SYNTHESIS_HEADINGS: Record<SynthesisScope, string> = {
  current: 'Current Entry',
  pinned: 'Pinned Entries',
  deck: 'Recent Entries',
  week: 'Past Week',
  conversation: 'This Conversation',
};

/**
 * Synthesizes journal entries into the active page (or the week's page), and
 * answers Home's "Synthesize last week" deep link.
 */
export function useJournalSynthesis(options: {
  activeNote: WorkspaceNote | null;
  entries: JournalEntrySummary[];
  pinnedIds: Set<string>;
  selectedId: string | null;
  notify: JournalNotify;
  searchParams: URLSearchParams;
  setSearchParams: SetURLSearchParams;
}) {
  const { activeNote, entries, pinnedIds, selectedId, notify, searchParams, setSearchParams } = options;
  const queryClient = useQueryClient();

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
      return runSynthesis({
        title: scope === 'current' ? targets[0].title : SYNTHESIS_HEADINGS[scope],
        heading: SYNTHESIS_HEADINGS[scope],
        request: {
          conversationIds: targets.map((t) => t.id),
          scope,
          maxEntries: SYNTHESIS_ENTRY_LIMIT,
        },
        // Capture the destination at start, even if the user changes pages.
        destination: scope === 'week' ? { kind: 'week', title: weekPageTitle() } : { kind: 'note', noteId: activeNote.id },
      }, queryClient);
    },
    [
      activeNote,
      notify,
      queryClient,
      selectSynthesisTargets,
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

  return handleSynthesize;
}
