import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { fetchConversationList } from '@/features/chat/api/conversationQueryData';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { bindThreadToFolder, useExplorerFolderMutations, useExplorerFoldersQuery } from '@/features/explorer/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { Conversation } from '@/types/conversation';


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
  remove: (_id: string) => Promise<boolean>;
  retry: () => void;
}

function threadTitle(rootName: string): string {
  const stamp = new Date().toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  return `${rootName} · ${stamp}`;
}

/**
 * The conversations bound to a folder, and which of them the chat shows.
 *
 * Keeps the active folder thread in local state (projected into `<ChatPanel />`
 * via `ScopedConversationProvider`) so visiting Explorer never overwrites
 * `/chat`'s `activeConversationId`.
 */
export function useExplorerThread(root: string | null, rootName: string): ExplorerThreads {
  const api = useApi();
  const queryClient = useQueryClient();
  const createConversation = useConversationsStore((state) => state.createConversation);
  const deleteConversation = useConversationsStore((state) => state.deleteConversation);
  const foldersQuery = useExplorerFoldersQuery();
  const lastThreadId = foldersQuery.data?.folders.find((folder) => folder.root === root)?.lastThreadId ?? null;
  const { mutate: saveLastThread } = useExplorerFolderMutations().setLastThread;
  const [selectedByRoot, setSelectedByRoot] = useState<Record<string, string | null>>({});
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const creatingForRef = useRef<string | null>(null);
  /** The last thread written per folder, so a write is not repeated while the list refreshes. */
  const writtenRef = useRef<Record<string, string>>({});

  const rememberThread = useCallback((folder: string, conversationId: string) => {
    if (lastThreadId === conversationId || writtenRef.current[folder] === conversationId) return;
    writtenRef.current[folder] = conversationId;
    // Best effort: losing it only means the folder reopens on its newest thread.
    saveLastThread({ root: folder, conversationId }, {
      onError: () => {
        if (writtenRef.current[folder] === conversationId) delete writtenRef.current[folder];
      },
    });
  }, [lastThreadId, saveLastThread]);

  const threadsQuery = useQuery({
    queryKey: threadsKey(root ?? ''),
    queryFn: async () => {
      const all = await fetchConversationList({ spaceId: null, filterMode: 'all', searchQuery: '' }, api);
      return all
        .filter((conversation) => conversation.explorerRoot === root)
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    },
    enabled: Boolean(root),
    staleTime: 15_000,
  });
  const threads = useMemo(() => threadsQuery.data ?? [], [threadsQuery.data]);
  const explicitId = root ? (selectedByRoot[root] ?? null) : null;
  const activeThread = useMemo(() => {
    if (!root || !threadsQuery.isSuccess || foldersQuery.isPending) return null;
    if (explicitId) {
      const picked = threads.find((thread) => thread.id === explicitId);
      if (picked) return picked;
    }
    if (lastThreadId) {
      const remembered = threads.find((thread) => thread.id === lastThreadId);
      if (remembered) return remembered;
    }
    return threads[0] ?? null;
  }, [explicitId, foldersQuery.isPending, lastThreadId, root, threads, threadsQuery.isSuccess]);
  const activeId = activeThread?.id ?? null;

  const create = useCallback(async () => {
    if (!root || creatingForRef.current === root) return;
    creatingForRef.current = root;
    setCreating(true);
    setError(null);
    const previousActiveId = conversationUiStore.getState().activeConversationId;
    try {
      // Made in General, never the Chat sidebar's space: binding it to the
      // folder files it in the folder's own space. Preserve `/chat`'s global
      // selection because `createConversation` activates the newly created id.
      const id = await createConversation(threadTitle(rootName), GENERAL_SPACE_ID);
      if (conversationUiStore.getState().activeConversationId !== previousActiveId) {
        conversationUiStore.setState({ activeConversationId: previousActiveId });
      }
      const bound = await bindThreadToFolder(id, root, api);
      if (!bound.ok) throw new Error(bound.error);
      // The create response predates the binding; the send path reads it.
      queryClient.setQueryData<Conversation>(conversationKeys.detail(id), (current) =>
        current ? { ...current, explorerRoot: root } : current);
      // Seeded so the chat shows the thread before the list refetch lands.
      queryClient.setQueryData<Conversation[]>(threadsKey(root), (current) => {
        const created = queryClient.getQueryData<Conversation>(conversationKeys.detail(id));
        return created ? [created, ...(current ?? []).filter((item) => item.id !== id)] : current;
      });
      setSelectedByRoot((prev) => ({ ...prev, [root]: id }));
      rememberThread(root, id);
      // The binding moved it into the folder's space after the create.
      void queryClient.invalidateQueries({ queryKey: conversationKeys.detail(id) });
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    } catch (cause) {
      if (conversationUiStore.getState().activeConversationId !== previousActiveId) {
        conversationUiStore.setState({ activeConversationId: previousActiveId });
      }
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      creatingForRef.current = null;
      setCreating(false);
    }
  }, [api, createConversation, queryClient, rememberThread, root, rootName]);

  // Persist the chosen/fallback thread on the folder row, or start one if empty.
  useEffect(() => {
    if (!root || !threadsQuery.isSuccess || foldersQuery.isPending || creating || error) return;
    if (activeId) {
      rememberThread(root, activeId);
      return;
    }
    void create();
  }, [activeId, create, creating, error, foldersQuery.isPending, rememberThread, root, threadsQuery.isSuccess]);

  const select = useCallback((id: string) => {
    if (!root) return;
    setSelectedByRoot((prev) => ({ ...prev, [root]: id }));
    rememberThread(root, id);
  }, [rememberThread, root]);

  const remove = useCallback(async (id: string) => {
    const removed = await deleteConversation(id);
    if (removed && root) {
      setSelectedByRoot((prev) => (prev[root] === id ? { ...prev, [root]: null } : prev));
      await queryClient.invalidateQueries({ queryKey: threadsKey(root) });
    }
    return removed;
  }, [deleteConversation, queryClient, root]);

  return {
    threads,
    activeId,
    preparing: Boolean(root) && (threadsQuery.isPending || creating || (!activeId && !error && !threadsQuery.isError)),
    error: error ?? (threadsQuery.isError ? threadsQuery.error.message : null),
    select,
    create,
    remove,
    retry: () => {
      setError(null);
      void threadsQuery.refetch();
    },
  };
}
