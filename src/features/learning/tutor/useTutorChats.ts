import { useCallback, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { Conversation } from '@/types/conversation';

/** Under the list prefix, so every list invalidation refreshes it too. */
const chatsKey = (programId: string) => [...conversationKeys.lists, 'tutor', programId] as const;

export interface TutorChats {
  chats: Conversation[];
  activeId: string | null;
  /** True while a new chat is being made. */
  creating: boolean;
  error: string | null;
  select: (_id: string) => void;
  /** Starts a chat on the course, and on a lesson when it is opened from one. */
  create: (_lesson?: { id: string; title: string }) => Promise<void>;
  remove: (_id: string) => Promise<boolean>;
}

function chatTitle(courseTitle: string, lessonTitle?: string): string {
  const stamp = new Date().toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  return `${lessonTitle ?? courseTitle} · ${stamp}`;
}

/**
 * The tutor chats of a course, and which of them the shared chat shows. Keeps
 * the selected tutor chat in local state (projected into `<ChatPanel />` via
 * `ScopedConversationProvider`) so Studio Tutor never overwrites `/chat`'s
 * `activeConversationId`.
 */
export function useTutorChats(programId: string, courseTitle: string): TutorChats {
  const api = useApi();
  const queryClient = useQueryClient();
  const createConversation = useConversationsStore((state) => state.createConversation);
  const deleteConversation = useConversationsStore((state) => state.deleteConversation);
  const [selectedByProgram, setSelectedByProgram] = useState<Record<string, string | null>>({});
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const creatingRef = useRef(false);

  const chatsQuery = useQuery({
    queryKey: chatsKey(programId),
    queryFn: async () => {
      const result = await api.listConversationsExplorer({
        spaceId: null,
        programId,
        query: null,
        savedOnly: null,
        bookmarkedOnly: null,
        pinnedOnly: null,
        hasMessageBookmarks: null,
        includeArchived: null,
        limit: null,
        offset: null,
      });
      if (!result.ok) throw new Error(result.error);
      return result.data.conversations as Conversation[];
    },
    staleTime: 15_000,
  });
  const chats = useMemo(() => chatsQuery.data ?? [], [chatsQuery.data]);
  const explicitId = selectedByProgram[programId] ?? null;
  const activeChat = useMemo(() => {
    if (explicitId) {
      const picked = chats.find((chat) => chat.id === explicitId);
      if (picked) return picked;
    }
    return chats[0] ?? null;
  }, [chats, explicitId]);
  const activeId = activeChat?.id ?? null;

  const create = useCallback(async (lesson?: { id: string; title: string }) => {
    if (creatingRef.current) return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    const previousActiveId = conversationUiStore.getState().activeConversationId;
    try {
      const id = await createConversation(chatTitle(courseTitle, lesson?.title), GENERAL_SPACE_ID);
      if (conversationUiStore.getState().activeConversationId !== previousActiveId) {
        conversationUiStore.setState({ activeConversationId: previousActiveId });
      }
      await api.setConversationCourse({ conversationId: id, programId, lessonId: lesson?.id ?? null });
      const bound = { programId, lessonId: lesson?.id ?? null };
      // The create response predates the binding.
      queryClient.setQueryData<Conversation>(conversationKeys.detail(id), (current) =>
        current ? { ...current, ...bound } : current);
      // Seeded so the chat shows before the list refetch lands.
      queryClient.setQueryData<Conversation[]>(chatsKey(programId), (current) => {
        const created = queryClient.getQueryData<Conversation>(conversationKeys.detail(id));
        return created ? [created, ...(current ?? []).filter((item) => item.id !== id)] : current;
      });
      setSelectedByProgram((prev) => ({ ...prev, [programId]: id }));
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    } catch (cause) {
      if (conversationUiStore.getState().activeConversationId !== previousActiveId) {
        conversationUiStore.setState({ activeConversationId: previousActiveId });
      }
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      creatingRef.current = false;
      setCreating(false);
    }
  }, [api, courseTitle, createConversation, programId, queryClient]);

  const select = useCallback((id: string) => {
    setSelectedByProgram((prev) => ({ ...prev, [programId]: id }));
  }, [programId]);

  const remove = useCallback(async (id: string) => {
    const removed = await deleteConversation(id);
    if (removed) {
      setSelectedByProgram((prev) => (prev[programId] === id ? { ...prev, [programId]: null } : prev));
      await queryClient.invalidateQueries({ queryKey: chatsKey(programId) });
    }
    return removed;
  }, [deleteConversation, programId, queryClient]);

  return {
    chats,
    activeId,
    creating,
    error: error ?? (chatsQuery.isError ? chatsQuery.error.message : null),
    select,
    create,
    remove,
  };
}
