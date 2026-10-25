import { useCallback, useEffect, useMemo, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useJournalsQuery, useCapturedReferencesQuery, WORKSPACE_NOTES_QUERY_KEY } from '@/features/journal/api/queries';
import { rememberedJournalPageId } from '@/features/journal/hooks/useJournalNote';
import {
  useDeletePassageReferenceMutation,
  usePassageReferencesQuery,
  useUpdatePassageReferenceMutation,
} from '@/features/reading/hooks/usePassageReferencesQuery';
import {
  bookmarkPayloadKey,
  bookmarkPayloadQueryOptions,
  useBookmarkMutations,
  useInboxBookmarksQuery,
} from '@/features/references/api/queries';
import { mergeInboxItems, type InboxItem } from '@/features/references/model/inboxItems';
import { useSpacesQuery } from '@/features/spaces/api/queries';
import { useDebounce } from '@/hooks/useDebounce';
import { toast } from '@/stores/toastStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type {
  ConversationJournalDto,
  ConversationSpaceDto,
} from '@/types/api/conversation';
import type { PassageReferenceDto } from '@/types/api/references';
import type { BookmarkPayload } from '@/utils/chatBookmarks';
import { captureChatReferenceToWorkspaceNote } from '@/utils/chatReferenceCapture';
import {
  chatReferenceKey,
  type CapturedChatReference,
} from '@/utils/chatReferenceIndex';

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
  /** Saved passages or bookmarks failed to load, so the inbox may be missing items. */
  listLoadFailed: boolean;
  isRetryingLoad: boolean;
  /** Loads whichever list failed again. */
  retryLoad: () => void;
  /** `requestedReferenceId` is neither selected nor known to be gone yet. */
  isRequestPending: boolean;
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
  const api = useApi();
  const { requestedReferenceId } = options;

  const [query, setQuery] = useState('');
  const [originFilter, setOriginFilter] = useState<OriginFilter>('all');
  const [statusChip, setStatusChip] = useState<StatusChip>('all');

  const [selectedId, setSelectedId] = useState<string | null>(null);

  const debouncedQuery = useDebounce(query, 220);

  // React Query mirrors passage-reference state owned by the repository.
  const passagesQuery = usePassageReferencesQuery();
  const updatePassageMutation = useUpdatePassageReferenceMutation();
  const deletePassageMutation = useDeletePassageReferenceMutation();
  const passages = useMemo(() => passagesQuery.data ?? [], [passagesQuery.data]);

  const client = useQueryClient();
  const bookmarksQuery = useInboxBookmarksQuery(debouncedQuery);
  const { save: saveBookmark, remove: removeBookmark } = useBookmarkMutations();
  const spacesQuery = useSpacesQuery();
  const journalsQuery = useJournalsQuery();
  const capturesQuery = useCapturedReferencesQuery();
  const bookmarks = useMemo(() => bookmarksQuery.data ?? [], [bookmarksQuery.data]);
  const spacesById = useMemo(() => new Map((spacesQuery.data ?? []).map(space => [space.id, space])), [spacesQuery.data]);
  const journalsById = useMemo(() => new Map(journalsQuery.journals.map(journal => [journal.id, journal])), [journalsQuery.journals]);
  const capturedIndex = useMemo(() => capturesQuery.data ?? new Map<string, CapturedChatReference>(), [capturesQuery.data]);
  const isLoading = bookmarksQuery.isPending || spacesQuery.isPending || journalsQuery.isPending || capturesQuery.isPending || passagesQuery.isPending;
  const loadError = bookmarksQuery.error ?? passagesQuery.error ?? spacesQuery.error ?? journalsQuery.error ?? capturesQuery.error;
  // Both lists have answered, so a reference missing from them does not exist.
  const listsLoaded = bookmarksQuery.isSuccess && passagesQuery.isSuccess;
  // The inbox merges two lists; either one failing leaves it incomplete, even
  // when the other has items to show.
  const { isError: bookmarksFailed, refetch: refetchBookmarks } = bookmarksQuery;
  const { isError: passagesFailed, refetch: refetchPassages } = passagesQuery;
  const listLoadFailed = bookmarksFailed || passagesFailed;
  const isRetryingLoad = (bookmarksFailed && bookmarksQuery.isFetching) || (passagesFailed && passagesQuery.isFetching);
  const retryLoad = useCallback(() => {
    if (bookmarksFailed) void refetchBookmarks();
    if (passagesFailed) void refetchPassages();
  }, [bookmarksFailed, passagesFailed, refetchBookmarks, refetchPassages]);
  useEffect(() => {
    if (loadError) toast.error('Failed to load references', { message: loadError.message });
  }, [loadError]);

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
  const [appliedRequestId, setAppliedRequestId] = useState<string | null>(null);
  // Until it is opened or known to be gone, the URL must keep naming it.
  const isRequestPending = requestedReferenceId !== null && requestedReferenceId !== appliedRequestId;

  // Selection reconciliation: respect a newly requested reference id, fall back
  // to the first item.
  useEffect(() => {
    if (isRequestPending) {
      const match = filteredItems.find((item) => item.id === requestedReferenceId);
      if (match) {
        setAppliedRequestId(requestedReferenceId);
        if (selectedId !== match.id) setSelectedId(match.id);
        return;
      }
      // It may be in the list that has not answered yet. Falling back now
      // would open a different reference, and the URL would then lose the link.
      if (!listsLoaded) return;
      setAppliedRequestId(requestedReferenceId);
    }
    if (filteredItems.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    const stillValid = selectedId
      ? filteredItems.some((item) => item.id === selectedId)
      : false;
    if (!stillValid) {
      setSelectedId(filteredItems[0]?.id ?? null);
    }
  }, [filteredItems, isRequestPending, listsLoaded, requestedReferenceId, selectedId]);

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

  // The selected reference's content is read when it is opened.
  const payloadQuery = useQuery(bookmarkPayloadQueryOptions(selectedBookmark ?? null, api));
  const selectedPayload = selectedBookmark ? payloadQuery.data ?? null : null;
  const isResolvingSelected = Boolean(selectedBookmark) && payloadQuery.isLoading;
  const resolutionFailed = Boolean(selectedBookmark) && payloadQuery.isError;

  const getPayload = useCallback(
    (bookmark: ConversationMessageBookmarkDto): Promise<BookmarkPayload> =>
      client.fetchQuery(bookmarkPayloadQueryOptions(bookmark, api)),
    [api, client],
  );

  const resolveCaptureDestination = useCallback(
    (bookmark: ConversationMessageBookmarkDto): CaptureDestination => {
      const journal = journalsById.get(bookmark.spaceId) ?? null;
      if (journal) {
        return {
          type: 'journal',
          space: journal,
          preferredNoteId: rememberedJournalPageId(journal.id),
        };
      }
      return { type: 'daily', space: null, preferredNoteId: null };
    },
    [journalsById],
  );

  const saveAnnotations = useCallback(
    async (
      bookmark: ConversationMessageBookmarkDto,
      next: { title: string | null; note: string | null },
    ): Promise<boolean> => {
      try {
        await saveBookmark.mutateAsync({ conversationId: bookmark.conversationId, messageId: bookmark.messageId, ...next });
        return true;
      } catch (error) {
        toast.error('Failed to save annotation', { message: error instanceof Error ? error.message : String(error) });
        return false;
      }
    },
    [saveBookmark],
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
      }, api);
      if (!result.snapshotId) return null;
      const captured: CapturedChatReference = {
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
        snapshotId: result.snapshotId,
        capturedAt: new Date().toISOString(),
        noteId: result.noteId,
        noteTitle: result.noteTitle,
        // The page the journal was on, when the capture went there; the
        // refreshed notes list says for any other page.
        journalId: destination.space && result.noteId === destination.preferredNoteId ? destination.space.id : null,
      };
      await client.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY });
      return captured;
    },
    [api, client, getPayload, resolveCaptureDestination],
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
      try {
        await removeBookmark.mutateAsync({ conversationId: bookmark.conversationId, messageId: bookmark.messageId });
      } catch (error) {
        toast.error('Failed to remove reference', { message: error instanceof Error ? error.message : String(error) });
        return false;
      }
      client.removeQueries({ queryKey: bookmarkPayloadKey(bookmark.conversationId, bookmark.messageId) });
      return true;
    },
    [client, removeBookmark],
  );

  return {
    bookmarks,
    filteredItems,
    selectedItem,
    spacesById,
    journalsById,
    capturedIndex,
    isLoading,
    listLoadFailed,
    isRetryingLoad,
    retryLoad,
    isRequestPending,
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
  };
}
