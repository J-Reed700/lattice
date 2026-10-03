import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { fetchConversationList } from '@/hooks/queries/conversationQueryData';
import { VaultAPI } from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore } from '@/stores/explorerStore';
import type { Conversation } from '@/types/conversation';

import { GENERAL_SPACE_ID } from './useExplorerFolders';

/** Under the list prefix, so every list invalidation refreshes it too. */
const threadsKey = (root: string) => [...conversationKeys.lists, 'explorer', root] as const;

export interface ExplorerThreads {
  threads: Conversation[];
  activeId: string | null;
  /** True while the thread for this folder is being found or made. */
  preparing: boolean;
  error: string | null;
  select: (_id: string) => void;
  create: () => Promise<void>;
  retry: () => void;
}

function threadTitle(rootName: string): string {
  const stamp = new Date().toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  return `${rootName} · ${stamp}`;
}

/**
 * The conversations bound to a folder, and which of them the chat shows.
 *
 * Entering a folder opens the thread last used there, else its most recent
 * one, else a new one. The chat column only ever shows a thread of the
 * current folder: the send path reads the folder's focus, and a Chat
 * conversation must not receive it.
 */
export function useExplorerThread(root: string | null, rootName: string): ExplorerThreads {
  const queryClient = useQueryClient();
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const threadByRoot = useExplorerStore((state) => state.threadByRoot);
  const rememberThread = useExplorerStore((state) => state.rememberThread);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const creatingForRef = useRef<string | null>(null);

  const threadsQuery = useQuery({
    queryKey: threadsKey(root ?? ''),
    queryFn: async () => {
      const all = await fetchConversationList({ spaceId: null, filterMode: 'all', searchQuery: '' });
      return all
        .filter((conversation) => conversation.explorerRoot === root)
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    },
    enabled: Boolean(root),
    staleTime: 15_000,
  });
  const threads = useMemo(() => threadsQuery.data ?? [], [threadsQuery.data]);
  const activeIsThread = threads.some((thread) => thread.id === activeConversationId);

  const create = useCallback(async () => {
    if (!root || creatingForRef.current === root) return;
    creatingForRef.current = root;
    setCreating(true);
    setError(null);
    try {
      // Made in General, never the Chat sidebar's space: binding it to the
      // folder files it in the folder's own space.
      const id = await createConversation(threadTitle(rootName), GENERAL_SPACE_ID);
      const bound = await VaultAPI.setConversationExplorerRoot(id, root);
      if (!bound.ok) throw new Error(bound.error);
      // The create response predates the binding; the send path reads it.
      queryClient.setQueryData<Conversation>(conversationKeys.detail(id), (current) =>
        current ? { ...current, explorerRoot: root } : current);
      // Seeded so the chat shows the thread before the list refetch lands.
      queryClient.setQueryData<Conversation[]>(threadsKey(root), (current) => {
        const created = queryClient.getQueryData<Conversation>(conversationKeys.detail(id));
        return created ? [created, ...(current ?? []).filter((item) => item.id !== id)] : current;
      });
      rememberThread(root, id);
      // The binding moved it into the folder's space after the create.
      void queryClient.invalidateQueries({ queryKey: conversationKeys.detail(id) });
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      creatingForRef.current = null;
      setCreating(false);
    }
  }, [createConversation, queryClient, rememberThread, root, rootName]);

  // Land on this folder's thread whenever the folder or its threads change.
  useEffect(() => {
    if (!root || !threadsQuery.isSuccess || creating || error) return;
    if (activeIsThread) {
      rememberThread(root, activeConversationId!);
      return;
    }
    const remembered = threads.find((thread) => thread.id === threadByRoot[root]);
    const next = remembered ?? threads[0];
    if (next) void selectConversation(next.id);
    else void create();
  }, [activeConversationId, activeIsThread, create, creating, error, rememberThread, root, selectConversation, threadByRoot, threads, threadsQuery.isSuccess]);

  return {
    threads,
    activeId: activeIsThread ? activeConversationId : null,
    preparing: Boolean(root) && (threadsQuery.isPending || creating || (!activeIsThread && !error && !threadsQuery.isError)),
    error: error ?? (threadsQuery.isError ? threadsQuery.error.message : null),
    select: (id) => {
      void selectConversation(id);
      if (root) rememberThread(root, id);
    },
    create,
    retry: () => {
      setError(null);
      void threadsQuery.refetch();
    },
  };
}
