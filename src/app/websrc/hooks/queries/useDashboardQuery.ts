import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  ConversationDto,
  ConversationJournalDto,
  ConversationMessageBookmarkDto,
} from '@/types/api/conversation';
import type { RecentDocument, IndexingActivity } from '@/types';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';

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

export interface DashboardData {
  stats: {
    documentCount: number;
    storageUsed: number;
    searchCount: number;
    lastIndexed: string;
  };
  recentDocuments: RecentDocument[];
  recentActivities: IndexingActivity[];
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

function pickPreferredJournal(
  journals: ConversationJournalDto[]
): ConversationJournalDto | null {
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
  journalSpaceIds: Set<string>
): DashboardConversationSummary[] {
  return conversations
    .filter((conv) => !conv.isArchived)
    .filter((conv) => !conv.spaceId || !journalSpaceIds.has(conv.spaceId))
    .sort(
      (a, b) =>
        new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
    )
    .slice(0, 3)
    .map((conv) => ({
      id: conv.id,
      title: conv.title,
      lastMessagePreview: conv.lastMessagePreview ?? null,
      updatedAt: conv.updatedAt,
    }));
}

export const useDashboardQuery = (
  options?: Omit<UseQueryOptions<DashboardData, Error>, 'queryKey' | 'queryFn'>
) => useQuery<DashboardData, Error>({
    queryKey: ['dashboard'],
    queryFn: async () => {
      const [
        statsResult,
        docsResult,
        conversationsResult,
        journalsResult,
        bookmarksResult,
      ] = await Promise.all([
        VaultAPI.getIndexingStats(),
        VaultAPI.listAllDocuments(10000),
        VaultAPI.listConversations(),
        VaultAPI.listJournals(),
        VaultAPI.listMessageBookmarks({ limit: 10, offset: 0 }),
      ]);

      if (!statsResult.ok || !docsResult.ok) {
        throw new Error('Failed to fetch dashboard data');
      }

      const indexingStats = statsResult.data;

      // Resolve journals (best-effort — failure shouldn't block the dashboard).
      const journals: ConversationJournalDto[] = journalsResult.ok
        ? journalsResult.data
        : [];
      const journalSpaceIds = new Set(journals.map((j) => j.id));
      const preferredJournal = pickPreferredJournal(journals);

      // Resolve recent conversations (chat-origin only).
      const allConversations: ConversationDto[] = conversationsResult.ok
        ? conversationsResult.data.conversations
        : [];
      const recentConversations = selectRecentChatConversations(
        allConversations,
        journalSpaceIds
      );

      // Resolve today's journal entry, if one exists in the preferred journal.
      let todayJournalEntry: DashboardJournalEntrySummary | null = null;
      if (preferredJournal) {
        const journalEntriesResult = await VaultAPI.listJournalConversations({
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

      // Resolve recent references (message bookmarks).
      const recentReferences: ConversationMessageBookmarkDto[] = bookmarksResult.ok
        ? [...bookmarksResult.data.bookmarks]
            .sort(
              (a, b) =>
                new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
            )
            .slice(0, 5)
        : [];

      return {
        stats: {
          documentCount: indexingStats?.indexedDocuments || 0,
          storageUsed: 0,
          searchCount: 0,
          lastIndexed: 'Never',
        },
        recentDocuments: (docsResult.data || [])
          .sort((a, b) => new Date(b.indexedAt).getTime() - new Date(a.indexedAt).getTime())
          .slice(0, 10)
          .map(doc => ({
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
        recentActivities: [],
        recentConversations,
        todayJournalEntry,
        preferredJournalSpaceId: preferredJournal?.id ?? null,
        recentReferences,
        journalSpaceIds: Array.from(journalSpaceIds),
      };
    },
    staleTime: 30 * 1000,
    refetchInterval: 30 * 1000,
    ...options,
  });
