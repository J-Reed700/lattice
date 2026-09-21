import { useMemo } from 'react';

import { useQueries } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { Conversation } from '@/types/conversation';

/**
 * Where a branched conversation came from.
 *
 * A fork copies a thread up to a turn and leaves the two unlinked on screen, so
 * a sidebar full of branches reads as a sidebar full of near-duplicates. The
 * parent's title, shown on the child's row, is what tells them apart.
 *
 * No foreign key stands behind `forkedFromConversationId`: a deleted parent
 * leaves a dangling id. That is an ordinary state, so a parent that cannot be
 * found is simply not drawn — never an error, never a dead link.
 */

export interface ForkParent {
  id: string;
  title: string;
  /** The turn the branch was taken at, when one was named. */
  messageId: string | null;
}

/** Kept apart from the controller's detail query: a miss here means "no parent". */
const forkParentKey = (id: string) => ['conversation-fork-parent', id] as const;

export function useForkLineage(conversations: readonly Conversation[]): Map<string, ForkParent> {
  const loadedTitles = useMemo(() => {
    const map = new Map<string, string>();
    for (const conversation of conversations) map.set(conversation.id, conversation.title);
    return map;
  }, [conversations]);

  // Only the parents the sidebar is not already holding are fetched. Filtering
  // by space or by "archived" routinely hides a parent that still exists, and
  // dropping the lineage in that case would be a lie of omission.
  const missingIds = useMemo(() => {
    const ids = new Set<string>();
    for (const conversation of conversations) {
      const parentId = conversation.forkedFromConversationId;
      if (parentId && parentId !== conversation.id && !loadedTitles.has(parentId)) ids.add(parentId);
    }
    return [...ids];
  }, [conversations, loadedTitles]);

  const fetched = useQueries({
    queries: missingIds.map((id) => ({
      queryKey: forkParentKey(id),
      staleTime: 5 * 60_000,
      retry: false,
      queryFn: async (): Promise<string | null> => {
        const result = await VaultAPI.getConversation(id);
        // A deleted parent is not a failure; it is simply no longer there.
        return result.ok ? result.data.conversation?.title ?? null : null;
      },
    })),
    combine: (results) => {
      const map = new Map<string, string>();
      results.forEach((result, index) => {
        const title = result.data;
        const id = missingIds[index];
        if (id && typeof title === 'string' && title.trim()) map.set(id, title);
      });
      return map;
    },
  });

  return useMemo(() => {
    const lineage = new Map<string, ForkParent>();
    for (const conversation of conversations) {
      const parentId = conversation.forkedFromConversationId;
      if (!parentId || parentId === conversation.id) continue;
      const title = loadedTitles.get(parentId) ?? fetched.get(parentId);
      if (!title) continue;
      lineage.set(conversation.id, {
        id: parentId,
        title,
        messageId: conversation.forkedFromMessageId ?? null,
      });
    }
    return lineage;
  }, [conversations, fetched, loadedTitles]);
}
