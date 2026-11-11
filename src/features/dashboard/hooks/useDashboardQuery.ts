import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { describeFailure } from '@/lib/errorHandling';
import type { RecentDocument } from '@/types';
import type {
  ConversationDto,
  ConversationJournalDto,
  ConversationMessageBookmarkDto,
} from '@/types/api/conversation';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const RECENT_DOCUMENTS = 10;
const TOP_FILE_TYPES = 5;

export interface DashboardConversationSummary {
  id: string;
  title: string;
  lastMessagePreview: string | null;
  updatedAt: string;
}

export interface DashboardJournalEntrySummary {
  journalSpaceId: string;
  entryId: string;
  title: string;
  lastMessagePreview: string | null;
  messageCount: number;
  updatedAt: string;
}

export interface DashboardFileTypeCount {
  label: string;
  count: number;
}

export interface DashboardStats {
  documentCount: number;
  /** Bytes on disk for the index, or null when the backend can't report it. */
  storageBytes: number | null;
  folderCount: number | null;
  /** ISO timestamp of the newest indexed document, or null when nothing is indexed. */
  lastIndexedAt: string | null;
  /** Top file types by count, descending. */
  fileTypes: DashboardFileTypeCount[];
}

export interface DashboardData {
  stats: DashboardStats;
  recentDocuments: RecentDocument[];
  recentConversations: DashboardConversationSummary[];
  todayJournalEntry: DashboardJournalEntrySummary | null;
  preferredJournalSpaceId: string | null;
  recentReferences: ConversationMessageBookmarkDto[];
  journalSpaceIds: string[];
}

function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function pickPreferredJournal(journals: ConversationJournalDto[]): ConversationJournalDto | null {
  const active = journals.filter((j) => !j.isArchived);
  if (active.length === 0) return null;

  let preferredId: string | null = null;
  try {
    preferredId = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
  } catch {
    preferredId = null;
  }

  return active.find((j) => j.id === preferredId) ?? active[0];
}

function selectRecentChatConversations(
  conversations: ConversationDto[],
  journalSpaceIds: Set<string>,
): DashboardConversationSummary[] {
  return conversations
    .filter((conv) => !conv.isArchived)
    .filter((conv) => !conv.spaceId || !journalSpaceIds.has(conv.spaceId))
    .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
    .slice(0, 3)
    .map((conv) => ({
      id: conv.id,
      title: conv.title,
      lastMessagePreview: conv.lastMessagePreview ?? null,
      updatedAt: conv.updatedAt,
    }));
}

export const useDashboardQuery = (
  options?: Omit<UseQueryOptions<DashboardData, Error>, 'queryKey' | 'queryFn'>,
) => {
  const api = useApi();
  return useQuery<DashboardData, Error>({
    queryKey: ['dashboard'],
    queryFn: async () => {
      const [
        statsResult,
        docsResult,
        shapeResult,
        systemStatsResult,
        foldersResult,
        conversationsResult,
        journalsResult,
        bookmarksResult,
      ] = await Promise.all([
        api.getIndexingStats(),
        api.listAllDocuments(RECENT_DOCUMENTS),
        api.getCorpusShape(),
        api.getSystemStats(),
        api.getIndexedFolders(),
        api.listConversations(),
        api.listJournals(),
        api.listMessageBookmarks({ limit: 10, offset: 0 }),
      ]);

      // Say why Home could not load: the backend's reason, not a stock line.
      if (!statsResult.ok) throw new Error(describeFailure(statsResult));
      if (!docsResult.ok) throw new Error(describeFailure(docsResult));
      if (!shapeResult.ok) throw new Error(describeFailure(shapeResult));

      const documents = docsResult.data ?? [];

      const journals: ConversationJournalDto[] = journalsResult.ok ? journalsResult.data : [];
      const journalSpaceIds = new Set(journals.map((j) => j.id));
      const preferredJournal = pickPreferredJournal(journals);

      const allConversations: ConversationDto[] = conversationsResult.ok
        ? conversationsResult.data.conversations
        : [];
      const recentConversations = selectRecentChatConversations(allConversations, journalSpaceIds);

      let todayJournalEntry: DashboardJournalEntrySummary | null = null;
      if (preferredJournal) {
        const journalEntriesResult = await api.listJournalConversations({
          journalSpaceId: preferredJournal.id,
          includeArchived: false,
          limit: 25,
          offset: 0,
        });
        if (journalEntriesResult.ok) {
          const entries = Array.isArray(journalEntriesResult.data)
            ? (journalEntriesResult.data as ConversationDto[])
            : journalEntriesResult.data.conversations;
          const today = new Date();
          const candidate = entries.find((entry) => {
            const updated = new Date(entry.updatedAt);
            return !Number.isNaN(updated.getTime()) && isSameDay(updated, today);
          });
          if (candidate) {
            todayJournalEntry = {
              journalSpaceId: preferredJournal.id,
              entryId: candidate.id,
              title: candidate.title,
              lastMessagePreview: candidate.lastMessagePreview ?? null,
              messageCount: candidate.messageCount,
              updatedAt: candidate.updatedAt,
            };
          }
        }
      }

      const recentReferences: ConversationMessageBookmarkDto[] = bookmarksResult.ok
        ? [...bookmarksResult.data.bookmarks]
            .sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
            .slice(0, 5)
        : [];

      const storageBytes =
        systemStatsResult.ok && typeof systemStatsResult.data?.storage_size_bytes === 'number'
          ? systemStatsResult.data.storage_size_bytes
          : null;
      const folderCount = foldersResult.ok && Array.isArray(foldersResult.data) ? foldersResult.data.length : null;

      return {
        stats: {
          documentCount: statsResult.data.indexedDocuments,
          storageBytes,
          folderCount,
          lastIndexedAt: shapeResult.data.lastIndexedAt,
          fileTypes: shapeResult.data.byType
            .slice(0, TOP_FILE_TYPES)
            .map(({ type, count }) => ({ label: type, count })),
        },
        recentDocuments: documents.map((doc) => ({
          id: doc.id,
          documentId: doc.id,
          documentName: doc.fileName,
          documentPath: doc.filePath,
          fileType: doc.fileType,
          lastAccessedAt: doc.indexedAt,
          accessCount: 0,
          fileName: doc.fileName,
          filePath: doc.filePath,
          indexedAt: doc.indexedAt,
          modifiedAt: doc.modifiedAt,
          sizeBytes: 0,
        })),
        recentConversations,
        todayJournalEntry,
        preferredJournalSpaceId: preferredJournal?.id ?? null,
        recentReferences,
        journalSpaceIds: Array.from(journalSpaceIds),
      };
    },
    staleTime: 30_000,
    ...options,
  });
};
