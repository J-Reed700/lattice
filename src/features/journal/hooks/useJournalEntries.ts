import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import {
  journalEntryPinsKey,
  journalEntryPinsQueryOptions,
  useSetJournalEntryPinnedMutation,
} from '@/features/journal/api/queries';
import VaultAPI from '@/lib/api';
import { toast } from '@/stores/toastStore';
import { useVaultImportStore } from '@/stores/vaultImportStore';
import type { ConversationDto, MessageDto } from '@/types/api/conversation';
import type { SnapshotMessage } from '@/types/api/dailyNotes';
import type { ConversationMessage as ChatConversationMessage } from '@/types/conversation';

const CONVERSATION_LIMIT = 100;

export interface JournalEntrySummary {
  id: string;
  title: string;
  updatedAt: string;
  messageCount: number;
}

export type EntryFilter = 'all' | 'pinned' | 'today';

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function asRole(value: unknown): 'user' | 'assistant' | 'system' {
  if (value === 'user' || value === 'assistant' || value === 'system') return value;
  return 'assistant';
}

function makeId(prefix: string): string {
  return `${prefix}_${crypto.randomUUID()}`;
}

function nowIso(): string {
  return new Date().toISOString();
}

function normalizeConversation(raw: ConversationDto | Record<string, unknown>): JournalEntrySummary {
  const source = raw as Partial<ConversationDto> & Record<string, unknown>;
  const updatedAt = asString(
    source.updatedAt ?? source.updated_at ?? source.createdAt ?? source.created_at,
    nowIso(),
  );
  return {
    id: asString(source.id, makeId('conversation')),
    title: asString(source.title, 'Untitled entry'),
    updatedAt,
    messageCount: asNumber(source.messageCount ?? source.message_count, 0),
  };
}

function normalizeMessage(
  raw: MessageDto | ChatConversationMessage | Record<string, unknown>,
): SnapshotMessage {
  const source = raw as Partial<MessageDto> &
    Partial<ChatConversationMessage> &
    Record<string, unknown>;
  return {
    id: asString(source.id, makeId('message')),
    role: asRole(source.role),
    content: asString(source.content),
    createdAt: asString(source.createdAt ?? source.created_at, nowIso()),
    metadata: source.metadata ?? null,
  };
}

function parseRawConversations(
  data: unknown,
): Array<ConversationDto | Record<string, unknown>> {
  if (Array.isArray(data)) return data as Array<ConversationDto | Record<string, unknown>>;
  const wrapped = (data as { conversations?: unknown })?.conversations;
  if (Array.isArray(wrapped)) {
    return wrapped as Array<ConversationDto | Record<string, unknown>>;
  }
  return [];
}

function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

export interface UseJournalEntriesResult {
  entries: JournalEntrySummary[];
  isLoading: boolean;
  loadError: string | null;
  search: string;
  setSearch: (value: string) => void;
  filter: EntryFilter;
  setFilter: (value: EntryFilter) => void;
  pinnedIds: Set<string>;
  togglePinned: (conversationId: string) => void;
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
  messagesByConversation: Record<string, SnapshotMessage[]>;
  loadingByConversation: Record<string, boolean>;
  loadMessages: (conversationId: string) => Promise<SnapshotMessage[]>;
  reload: () => Promise<void>;
  removeEntry: (conversationId: string) => void;
}

/**
 * Owns journal entry list lifecycle, search/filter, selection, and
 * message-content caching for the selected entry.
 */
export function useJournalEntries(options: {
  journalSpaceId: string | null;
  requestedEntryId: string | null;
}): UseJournalEntriesResult {
  const { journalSpaceId, requestedEntryId } = options;

  const [entries, setEntries] = useState<JournalEntrySummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<EntryFilter>('all');
  const queryClient = useQueryClient();
  const pinsQuery = useQuery({
    ...journalEntryPinsQueryOptions(journalSpaceId ?? ''),
    enabled: Boolean(journalSpaceId),
  });
  const pinnedIds = useMemo(
    () => new Set(journalSpaceId ? pinsQuery.data ?? [] : []),
    [journalSpaceId, pinsQuery.data],
  );
  const setEntryPinned = useSetJournalEntryPinnedMutation();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messagesByConversation, setMessagesByConversation] = useState<
    Record<string, SnapshotMessage[]>
  >({});
  const [loadingByConversation, setLoadingByConversation] = useState<Record<string, boolean>>({});

  const appliedRequestedEntryKeyRef = useRef<string | null>(null);
  const requestRef = useRef(0);

  const reload = useCallback(async () => {
    const request = ++requestRef.current;
    if (!journalSpaceId) {
      setEntries([]);
      setIsLoading(false);
      return;
    }
    setIsLoading(true);
    setLoadError(null);
    const result = await VaultAPI.listJournalConversations({
      journalSpaceId,
      includeArchived: true,
      limit: CONVERSATION_LIMIT,
      offset: 0,
    });
    if (requestRef.current !== request) return;
    if (!result.ok) {
      setLoadError(result.error);
      setIsLoading(false);
      return;
    }
    const normalized = parseRawConversations(result.data).map((c) => normalizeConversation(c));
    setEntries(normalized);
    setIsLoading(false);
  }, [journalSpaceId]);

  useEffect(() => {
    setEntries([]);
    void reload();
    return () => { requestRef.current += 1; };
  }, [reload]);

  // Refetch on any external vault import.
  const importTick = useVaultImportStore((s) => s.importTick);
  useEffect(() => {
    if (importTick === 0) return;
    void reload();
  }, [importTick, reload]);

  useEffect(() => {
    appliedRequestedEntryKeyRef.current = null;
    setSelectedId(null);
  }, [journalSpaceId]);

  const { mutate: mutateEntryPinned } = setEntryPinned;
  const togglePinned = useCallback((conversationId: string) => {
    if (!journalSpaceId) return;
    const pinned = !pinnedIds.has(conversationId);
    mutateEntryPinned(
      { journalSpaceId, conversationId, pinned },
      {
        onError: (error) => toast.error(pinned ? "Couldn't pin that entry" : "Couldn't unpin that entry", {
          message: error instanceof Error ? error.message : String(error),
        }),
      },
    );
  }, [journalSpaceId, mutateEntryPinned, pinnedIds]);

  const sortedEntries = useMemo(() => {
    const withPinned = entries.map((entry, index) => ({
      entry,
      index,
      pinned: pinnedIds.has(entry.id),
    }));
    withPinned.sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      const aT = new Date(a.entry.updatedAt).getTime();
      const bT = new Date(b.entry.updatedAt).getTime();
      if (!Number.isNaN(aT) && !Number.isNaN(bT) && aT !== bT) return bT - aT;
      return a.index - b.index;
    });
    return withPinned.map((item) => item.entry);
  }, [entries, pinnedIds]);

  const filteredEntries = useMemo(() => {
    const query = search.trim().toLowerCase();
    let filtered = sortedEntries;
    if (query) {
      filtered = filtered.filter((entry) => entry.title.toLowerCase().includes(query));
    }
    if (filter === 'pinned') {
      filtered = filtered.filter((entry) => pinnedIds.has(entry.id));
    } else if (filter === 'today') {
      const today = new Date();
      filtered = filtered.filter((entry) => {
        const d = new Date(entry.updatedAt);
        return !Number.isNaN(d.getTime()) && isSameDay(d, today);
      });
    }
    return filtered;
  }, [sortedEntries, search, filter, pinnedIds]);

  // Apply URL-driven initial selection (once per journal space)
  useEffect(() => {
    if (!journalSpaceId) return;
    if (filteredEntries.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }

    const requestedKey = requestedEntryId ? `${journalSpaceId}:${requestedEntryId}` : null;
    if (
      requestedEntryId &&
      requestedKey &&
      appliedRequestedEntryKeyRef.current !== requestedKey
    ) {
      const match = filteredEntries.find((entry) => entry.id === requestedEntryId);
      if (match) {
        appliedRequestedEntryKeyRef.current = requestedKey;
        if (selectedId !== match.id) setSelectedId(match.id);
        return;
      }
    }

    const isStillValid = selectedId
      ? filteredEntries.some((entry) => entry.id === selectedId)
      : false;
    if (!isStillValid) {
      setSelectedId(filteredEntries[0]?.id ?? null);
    }
  }, [journalSpaceId, requestedEntryId, filteredEntries, selectedId]);

  const loadMessages = useCallback(
    async (conversationId: string): Promise<SnapshotMessage[]> => {
      const cached = messagesByConversation[conversationId];
      if (cached) return cached;

      setLoadingByConversation((current) => ({ ...current, [conversationId]: true }));
      const result = await VaultAPI.getConversationMessages(conversationId);
      const messages = result.ok
        ? result.data.messages.map((m) => normalizeMessage(m))
        : [];
      setMessagesByConversation((current) => ({ ...current, [conversationId]: messages }));
      setLoadingByConversation((current) => ({ ...current, [conversationId]: false }));
      return messages;
    },
    [messagesByConversation],
  );

  useEffect(() => {
    if (!selectedId) return;
    if (messagesByConversation[selectedId] !== undefined) return;
    void loadMessages(selectedId);
  }, [selectedId, messagesByConversation, loadMessages]);

  /** The entry left the journal, and its pin went with it. */
  const removeEntry = useCallback((conversationId: string) => {
    setEntries((current) => current.filter((e) => e.id !== conversationId));
    if (journalSpaceId) {
      queryClient.setQueryData<string[]>(journalEntryPinsKey(journalSpaceId), (current) =>
        current?.filter((id) => id !== conversationId));
    }
    setSelectedId((current) => (current === conversationId ? null : current));
  }, [journalSpaceId, queryClient]);

  return {
    entries: filteredEntries,
    isLoading,
    loadError,
    search,
    setSearch,
    filter,
    setFilter,
    pinnedIds,
    togglePinned,
    selectedId,
    setSelectedId,
    messagesByConversation,
    loadingByConversation,
    loadMessages,
    reload,
    removeEntry,
  };
}
