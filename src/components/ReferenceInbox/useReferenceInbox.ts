import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  useDeletePassageReferenceMutation,
  usePassageReferencesQuery,
  useUpdatePassageReferenceMutation,
} from '@/hooks/queries/usePassageReferencesQuery';
import { useDebounce } from '@/hooks/useDebounce';
import { VaultAPI } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type {
  ConversationJournalDto,
  ConversationSpaceDto,
} from '@/types/api/conversation';
import type { PassageReferenceDto } from '@/types/api/references';
import { resolveBookmarkPayload, type BookmarkPayload } from '@/utils/chatBookmarks';
import { captureChatReferenceToWorkspaceNote } from '@/utils/chatReferenceCapture';
import {
  buildCapturedChatReferenceIndex,
  chatReferenceKey,
  type CapturedChatReference,
} from '@/utils/chatReferenceIndex';

import { mergeInboxItems, type InboxItem } from './inboxItems';

export type OriginFilter = 'all' | 'chat' | 'journal' | 'document';
export type StatusChip = 'all' | 'recent' | 'captured';

export interface CaptureDestination {
  type: 'journal' | 'daily';
  space: ConversationJournalDto | null;
  preferredNoteId: string | null;
}

export interface UseReferenceInboxResult {
  bookmarks: ConversationMessageBookmarkDto[];
  /** Message bookmarks and passage references merged, newest first, filtered. */
  filteredItems: InboxItem[];
  selectedItem: InboxItem | null;
  spacesById: Map<string, ConversationSpaceDto>;
  journalsById: Map<string, ConversationJournalDto>;
  capturedIndex: Map<string, CapturedChatReference>;
  isLoading: boolean;
  query: string;
  setQuery: (value: string) => void;
  originFilter: OriginFilter;
  setOriginFilter: (value: OriginFilter) => void;
  statusChip: StatusChip;
  setStatusChip: (value: StatusChip) => void;
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
  selectedBookmark: ConversationMessageBookmarkDto | null;
  selectedCapture: CapturedChatReference | null;
  selectedSpace: ConversationSpaceDto | null;
  selectedPayload: BookmarkPayload | null;
  isResolvingSelected: boolean;
  resolutionFailed: boolean;
  resolveCaptureDestination: (bookmark: ConversationMessageBookmarkDto) => CaptureDestination;
  saveAnnotations: (
    bookmark: ConversationMessageBookmarkDto,
    next: { title: string | null; note: string | null },
  ) => Promise<boolean>;
  captureReference: (bookmark: ConversationMessageBookmarkDto) => Promise<CapturedChatReference | null>;
  removeReference: (bookmark: ConversationMessageBookmarkDto) => Promise<boolean>;
  savePassageAnnotations: (
    passage: PassageReferenceDto,
    next: { title: string | null; note: string | null },
  ) => Promise<boolean>;
  removePassage: (passage: PassageReferenceDto) => Promise<boolean>;
  getPayload: (bookmark: ConversationMessageBookmarkDto) => Promise<BookmarkPayload>;
  resolveJournalSpaceIdForNote: (noteId: string) => string | null;
}

function readJournalNoteIdForSpace(spaceId: string): string | null {
  try {
    const value = localStorage.getItem(`journal.noteBySpace.${spaceId}`);
    const resolved = value?.trim() ?? '';
    return resolved || null;
  } catch {
    return null;
  }
}

/**
 * Owns the data-loading lifecycle and state orchestration for the
 * ReferenceInbox surface. Wraps listMessageBookmarks + listWorkspaceNotes +
 * listConversationSpaces + listJournals, payload resolution caching,
 * annotation save / capture / remove actions, and filter/search derivations.
 * Spec §10.
 */
export function useReferenceInbox(options: {
  requestedReferenceId: string | null;
}): UseReferenceInboxResult {
  const { requestedReferenceId } = options;

  const [query, setQuery] = useState('');
  const [originFilter, setOriginFilter] = useState<OriginFilter>('all');
  const [statusChip, setStatusChip] = useState<StatusChip>('all');

  const [bookmarks, setBookmarks] = useState<ConversationMessageBookmarkDto[]>([]);
  const [spacesById, setSpacesById] = useState<Map<string, ConversationSpaceDto>>(new Map());
  const [journalsById, setJournalsById] = useState<Map<string, ConversationJournalDto>>(new Map());
  const [capturedIndex, setCapturedIndex] = useState<Map<string, CapturedChatReference>>(new Map());
  const [payloadCache, setPayloadCache] = useState<Map<string, BookmarkPayload>>(new Map());
  const [resolutionFailures, setResolutionFailures] = useState<Set<string>>(new Set());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isResolvingSelected, setIsResolvingSelected] = useState(false);

  const debouncedQuery = useDebounce(query, 220);

  // React Query mirrors passage-reference state owned by the repository.
  const passagesQuery = usePassageReferencesQuery();
  const updatePassageMutation = useUpdatePassageReferenceMutation();
  const deletePassageMutation = useDeletePassageReferenceMutation();
  const passages = useMemo(() => passagesQuery.data ?? [], [passagesQuery.data]);

  const load = useCallback(
    async () => {
      setIsLoading(true);

      const [bookmarksResult, notesResult, spacesResult, journalsResult] = await Promise.all([
        VaultAPI.listMessageBookmarks({
          query: debouncedQuery.trim() ? debouncedQuery.trim() : undefined,
          limit: 200,
          offset: 0,
        }),
        VaultAPI.listWorkspaceNotes(),
        VaultAPI.listConversationSpaces(),
        VaultAPI.listJournals(),
      ]);

      if (!bookmarksResult.ok) {
        toast.error('Failed to load references', { message: bookmarksResult.error });
      } else {
        setBookmarks(bookmarksResult.data.bookmarks);
      }

      if (notesResult.ok) {
        setCapturedIndex(buildCapturedChatReferenceIndex(notesResult.data.notes));
      } else {
        setCapturedIndex(new Map());
      }

      if (spacesResult.ok) {
        setSpacesById(new Map(spacesResult.data.map((space) => [space.id, space])));
      } else {
        setSpacesById(new Map());
      }

      if (journalsResult.ok) {
        setJournalsById(new Map(journalsResult.data.map((journal) => [journal.id, journal])));
      } else {
        setJournalsById(new Map());
      }

      setIsLoading(false);
    },
    [debouncedQuery],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const items = useMemo(
    () => mergeInboxItems(bookmarks, passages),
    [bookmarks, passages],
  );

  const normalizedQuery = debouncedQuery.trim().toLowerCase();

  const filteredItems = useMemo(
    () =>
      items.filter((item) => {
        if (originFilter !== 'all') {
          if (originFilter === 'document') {
            if (item.kind !== 'passage') return false;
          } else {
            if (item.kind !== 'message') return false;
            const isJournalBookmark = journalsById.has(item.bookmark.spaceId);
            if (originFilter === 'journal' && !isJournalBookmark) return false;
            if (originFilter === 'chat' && isJournalBookmark) return false;
          }
        }
        // "Captured" is a chat-reference concept; a passage has none, so the
        // filter keeps meaning what it says.
        if (statusChip === 'captured') {
          if (item.kind !== 'message') return false;
          const key = chatReferenceKey(item.bookmark.conversationId, item.bookmark.messageId);
          if (!capturedIndex.has(key)) return false;
        }
        if (statusChip === 'recent') {
          const created = new Date(item.createdAt).getTime();
          if (!Number.isFinite(created)) return false;
          const weekMs = 7 * 24 * 60 * 60 * 1000;
          if (Date.now() - created > weekMs) return false;
        }
        // Bookmark search is server-side; passages are filtered here.
        if (normalizedQuery && item.kind === 'passage') {
          const haystack = [
            item.passage.title ?? '',
            item.passage.note ?? '',
            item.passage.text,
            item.passage.fileName,
          ]
            .join('\n')
            .toLowerCase();
          if (!haystack.includes(normalizedQuery)) return false;
        }
        return true;
      }),
    [capturedIndex, items, journalsById, normalizedQuery, originFilter, statusChip],
  );

  // The URL is a follower, not a second source of truth. `ReferenceInbox`
  // mirrors `selectedId` into `?referenceId=`, so if this effect kept forcing
  // the URL's value back onto the selection the two would trade places forever:
  // click B → URL still says A → selection reverts to A while the URL becomes B
  // → selection becomes B while the URL reverts to A → repeat, pinning the CPU.
  // A requested id is therefore applied once, when it arrives; after that the
  // user's clicks own the selection.
  const appliedRequestRef = useRef<string | null>(null);

  // Selection reconciliation: respect a newly requested reference id, fall back
  // to the first item.
  useEffect(() => {
    if (filteredItems.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (requestedReferenceId && requestedReferenceId !== appliedRequestRef.current) {
      const match = filteredItems.find((item) => item.id === requestedReferenceId);
      if (match) {
        appliedRequestRef.current = requestedReferenceId;
        if (selectedId !== match.id) setSelectedId(match.id);
        return;
      }
    }
    const stillValid = selectedId
      ? filteredItems.some((item) => item.id === selectedId)
      : false;
    if (!stillValid) {
      setSelectedId(filteredItems[0]?.id ?? null);
    }
  }, [filteredItems, requestedReferenceId, selectedId]);

  const selectedItem = useMemo(
    () => filteredItems.find((item) => item.id === selectedId) ?? null,
    [filteredItems, selectedId],
  );

  const selectedBookmark =
    selectedItem?.kind === 'message' ? selectedItem.bookmark : null;

  const selectedCapture = selectedBookmark
    ? capturedIndex.get(
        chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId),
      ) ?? null
    : null;

  const selectedSpace = selectedBookmark
    ? spacesById.get(selectedBookmark.spaceId) ?? null
    : null;

  const selectedPayloadKey = selectedBookmark
    ? chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId)
    : null;

  const selectedPayload = selectedPayloadKey
    ? payloadCache.get(selectedPayloadKey) ?? null
    : null;

  const resolutionFailed = selectedPayloadKey
    ? resolutionFailures.has(selectedPayloadKey)
    : false;

  // Resolve payload for the current selection lazily.
  useEffect(() => {
    if (!selectedBookmark) return;
    const key = chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId);
    if (payloadCache.has(key)) return;

    let cancelled = false;
    setIsResolvingSelected(true);
    setResolutionFailures((current) => {
      if (!current.has(key)) return current;
      const next = new Set(current);
      next.delete(key);
      return next;
    });
    void resolveBookmarkPayload(selectedBookmark)
      .then((payload) => {
        if (cancelled) return;
        setPayloadCache((current) => {
          const next = new Map(current);
          next.set(key, payload);
          return next;
        });
      })
      .catch(() => {
        if (cancelled) return;
        setResolutionFailures((current) => {
          const next = new Set(current);
          next.add(key);
          return next;
        });
      })
      .finally(() => {
        if (!cancelled) setIsResolvingSelected(false);
      });

    return () => {
      cancelled = true;
    };
  }, [payloadCache, selectedBookmark]);

  const getPayload = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<BookmarkPayload> => {
      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      const cached = payloadCache.get(key);
      if (cached) return cached;
      const payload = await resolveBookmarkPayload(bookmark);
      setPayloadCache((current) => {
        const next = new Map(current);
        next.set(key, payload);
        return next;
      });
      return payload;
    },
    [payloadCache],
  );

  const resolveCaptureDestination = useCallback(
    (bookmark: ConversationMessageBookmarkDto): CaptureDestination => {
      const journal = journalsById.get(bookmark.spaceId) ?? null;
      if (journal) {
        return {
          type: 'journal',
          space: journal,
          preferredNoteId: readJournalNoteIdForSpace(journal.id),
        };
      }
      return { type: 'daily', space: null, preferredNoteId: null };
    },
    [journalsById],
  );

  const resolveJournalSpaceIdForNote = useCallback(
    (noteId: string): string | null => {
      for (const journal of journalsById.values()) {
        if (readJournalNoteIdForSpace(journal.id) === noteId) {
          return journal.id;
        }
      }
      return null;
    },
    [journalsById],
  );

  const saveAnnotations = useCallback(
    async (
      bookmark: ConversationMessageBookmarkDto,
      next: { title: string | null; note: string | null },
    ): Promise<boolean> => {
      const result = await VaultAPI.bookmarkConversationMessage({
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
        title: next.title,
        note: next.note,
      });
      if (!result.ok) {
        toast.error('Failed to save annotation', { message: result.error });
        return false;
      }
      setBookmarks((current) =>
        current.map((b) =>
          b.id === bookmark.id
            ? { ...b, title: next.title, note: next.note }
            : b,
        ),
      );
      return true;
    },
    [],
  );

  const captureReference = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<CapturedChatReference | null> => {
      const destination = resolveCaptureDestination(bookmark);
      const payload = await getPayload(bookmark);
      const result = await captureChatReferenceToWorkspaceNote({
        conversationId: bookmark.conversationId,
        conversationTitle: bookmark.conversationTitle,
        messageId: bookmark.messageId,
        messageRole: bookmark.messageRole,
        messageContent: payload.content,
        referenceTitle: bookmark.title,
        referenceNote: bookmark.note,
        sourceReferences: payload.sourceReferences,
        preferredNoteId: destination.preferredNoteId,
      });
      if (!result.snapshotId) return null;
      const captured: CapturedChatReference = {
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
        snapshotId: result.snapshotId,
        capturedAt: new Date().toISOString(),
        noteId: result.noteId,
        noteTitle: result.noteTitle,
      };
      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      setCapturedIndex((current) => {
        const next = new Map(current);
        next.set(key, captured);
        return next;
      });
      return captured;
    },
    [getPayload, resolveCaptureDestination],
  );

  const savePassageAnnotations = useCallback(
    async (
      passage: PassageReferenceDto,
      next: { title: string | null; note: string | null },
    ): Promise<boolean> => {
      try {
        await updatePassageMutation.mutateAsync({
          id: passage.id,
          title: next.title,
          note: next.note,
        });
        return true;
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        toast.error('Failed to save annotation', { message });
        return false;
      }
    },
    [updatePassageMutation],
  );

  const removePassage = useCallback(
    async (passage: PassageReferenceDto): Promise<boolean> => {
      try {
        await deletePassageMutation.mutateAsync(passage.id);
        return true;
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        toast.error('Failed to remove reference', { message });
        return false;
      }
    },
    [deletePassageMutation],
  );

  const removeReference = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<boolean> => {
      const result = await VaultAPI.unbookmarkConversationMessage({
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
      });
      if (!result.ok) {
        toast.error('Failed to remove reference', { message: result.error });
        return false;
      }
      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      setBookmarks((current) => current.filter((b) => b.id !== bookmark.id));
      setCapturedIndex((current) => {
        if (!current.has(key)) return current;
        const next = new Map(current);
        next.delete(key);
        return next;
      });
      setPayloadCache((current) => {
        if (!current.has(key)) return current;
        const next = new Map(current);
        next.delete(key);
        return next;
      });
      return true;
    },
    [],
  );

  return {
    bookmarks,
    filteredItems,
    selectedItem,
    spacesById,
    journalsById,
    capturedIndex,
    isLoading,
    query,
    setQuery,
    originFilter,
    setOriginFilter,
    statusChip,
    setStatusChip,
    selectedId,
    setSelectedId,
    selectedBookmark,
    selectedCapture,
    selectedSpace,
    selectedPayload,
    isResolvingSelected,
    resolutionFailed,
    resolveCaptureDestination,
    saveAnnotations,
    captureReference,
    removeReference,
    savePassageAnnotations,
    removePassage,
    getPayload,
    resolveJournalSpaceIdForNote,
  };
}
