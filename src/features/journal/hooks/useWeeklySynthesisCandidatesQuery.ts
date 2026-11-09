import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



export const weeklySynthesisCandidatesKey = ['synthesis', 'weekly-candidates'] as const;

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const WEEK_PAGE_TITLE_PREFIX = 'Week of ';

export interface WeeklySynthesisCandidates {
  conversations: number;
  references: number;
  notes: number;
  total: number;
}

function isWithinWeek(value: string | null | undefined, cutoff: number): boolean {
  if (!value) return false;
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) return false;
  return parsed >= cutoff;
}

/**
 * Real counts behind the weekly-synthesis affordances. The Home row and the
 * popover option only appear when there is genuinely something to synthesize —
 * no stat that is always zero.
 *
 * A failing sub-call contributes 0 rather than throwing: the affordance simply
 * does not appear, which is the correct degraded state. Each count is everything
 * in the week, which is what the backend run reads whole.
 */
export function useWeeklySynthesisCandidatesQuery() {
  const api = useApi();
  return useQuery<WeeklySynthesisCandidates, Error>({
    queryKey: weeklySynthesisCandidatesKey,
    queryFn: async () => {
      const cutoff = Date.now() - WEEK_MS;
      const [conversationsResult, referencesResult, notesResult] = await Promise.all([
        api.listConversations(),
        api.listPassageReferences(200),
        api.listWorkspaceNotes(),
      ]);

      const conversations = conversationsResult.ok
        ? conversationsResult.data.conversations.filter(
            (conversation) =>
              !conversation.isArchived && isWithinWeek(conversation.updatedAt, cutoff),
          ).length
        : 0;

      const references = referencesResult.ok
        ? referencesResult.data.filter((reference) =>
            isWithinWeek(reference.createdAt, cutoff),
          ).length
        : 0;

      const notes = notesResult.ok
        ? notesResult.data.notes.filter(
            (note) =>
              isWithinWeek(note.updatedAt, cutoff) &&
              note.content.trim() !== '' &&
              !note.title.startsWith(WEEK_PAGE_TITLE_PREFIX),
          ).length
        : 0;

      return {
        conversations,
        references,
        notes,
        total: conversations + references + notes,
      };
    },
    staleTime: 5 * 60_000,
  });
}
