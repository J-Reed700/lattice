import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
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
 * The tutor chats of a course, and which of them the shared chat shows. Opening
 * the tab lands on the most recent one; a course with none waits for the learner
 * to start one.
 */
export function useTutorChats(programId: string, courseTitle: string): TutorChats {
  const api = useApi();
  const queryClient = useQueryClient();
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const deleteConversation = useConversationsStore((state) => state.deleteConversation);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const creatingRef = useRef(false);

  const chatsQuery = useQuery({
    queryKey: chatsKey(programId),
    queryFn: async () => {
      const result = await api.listConversationsExplorer({ programId });
      if (!result.ok) throw new Error(result.error);
      return result.data.conversations as Conversation[];
    },
    staleTime: 15_000,
  });
  const chats = useMemo(() => chatsQuery.data ?? [], [chatsQuery.data]);
  const activeIsChat = chats.some((chat) => chat.id === activeConversationId);

  const create = useCallback(async (lesson?: { id: string; title: string }) => {
    if (creatingRef.current) return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    try {
      const id = await createConversation(chatTitle(courseTitle, lesson?.title), GENERAL_SPACE_ID);
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
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      creatingRef.current = false;
      setCreating(false);
    }
  }, [api, courseTitle, createConversation, programId, queryClient]);

  useEffect(() => {
    if (activeIsChat || creating) return;
    const latest = chats[0];
    if (latest) void selectConversation(latest.id);
  }, [activeIsChat, chats, creating, selectConversation]);

  return {
    chats,
    activeId: activeIsChat ? activeConversationId : null,
    creating,
    error: error ?? (chatsQuery.isError ? chatsQuery.error.message : null),
    select: (id) => void selectConversation(id),
    create,
    remove: deleteConversation,
  };
}
