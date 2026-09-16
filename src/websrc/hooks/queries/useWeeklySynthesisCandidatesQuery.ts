import { useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

export const weeklySynthesisCandidatesKey = ['synthesis', 'weekly-candidates'] as const;

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const WEEK_PAGE_TITLE_PREFIX = 'Week of ';

/**
 * What a week run actually reads, from
 * `features/conversation/plugin_impl.rs::select_week_entries`
 * (WEEK_SYNTHESIS_MAX_CONVERSATIONS / _REFERENCES / _NOTES). Counting past the
 * caps would let Home promise twenty conversations before a twelve-conversation
 * synthesis.
 */
const MAX_CONVERSATIONS = 12;
const MAX_REFERENCES = 20;
const MAX_NOTES = 8;

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
 * does not appear, which is the correct degraded state. Each count is capped at
 * what the backend run will actually read, so the promise matches the delivery.
 */
export function useWeeklySynthesisCandidatesQuery() {
  return useQuery<WeeklySynthesisCandidates, Error>({
    queryKey: weeklySynthesisCandidatesKey,
    queryFn: async () => {
      const cutoff = Date.now() - WEEK_MS;
      const [conversationsResult, referencesResult, notesResult] = await Promise.all([
        VaultAPI.listConversations(),
        VaultAPI.listPassageReferences(200),
        VaultAPI.listWorkspaceNotes(),
      ]);

      const conversations = Math.min(
        conversationsResult.ok
          ? conversationsResult.data.conversations.filter(
              (conversation) =>
                !conversation.isArchived && isWithinWeek(conversation.updatedAt, cutoff),
            ).length
          : 0,
        MAX_CONVERSATIONS,
      );

      const references = Math.min(
        referencesResult.ok
          ? referencesResult.data.filter((reference) =>
              isWithinWeek(reference.createdAt, cutoff),
            ).length
          : 0,
        MAX_REFERENCES,
      );

      const notes = Math.min(
        notesResult.ok
          ? notesResult.data.notes.filter(
              (note) =>
                isWithinWeek(note.updatedAt, cutoff) &&
                note.content.trim() !== '' &&
                !note.title.startsWith(WEEK_PAGE_TITLE_PREFIX),
            ).length
          : 0,
        MAX_NOTES,
      );

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
