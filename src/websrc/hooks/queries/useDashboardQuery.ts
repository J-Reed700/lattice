import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { DocumentMetadata, RecentDocument } from '@/types';
import type {
  ConversationDto,
  ConversationJournalDto,
  ConversationMessageBookmarkDto,
} from '@/types/api/conversation';

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

const FILE_TYPE_LABELS: Record<string, string> = {
  pdf: 'PDF',
  md: 'Markdown',
  markdown: 'Markdown',
  txt: 'Text',
  text: 'Text',
  html: 'Web',
  htm: 'Web',
  web_article_html: 'Web',
  docx: 'Word',
  doc: 'Word',
  rtf: 'RTF',
  odt: 'ODT',
  csv: 'CSV',
  json: 'JSON',
  xlsx: 'Excel',
  pptx: 'PowerPoint',
  png: 'Images',
  jpg: 'Images',
  jpeg: 'Images',
  gif: 'Images',
  webp: 'Images',
};

function labelForFileType(fileType: string | null | undefined): string {
  const key = (fileType ?? '').toLowerCase().replace(/^\./, '').trim();
  if (!key) return 'Other';
  return FILE_TYPE_LABELS[key] ?? key.toUpperCase();
}

/** Counts documents by display label; returns the top five, descending. */
export function summarizeFileTypes(documents: DocumentMetadata[], limit = 5): DashboardFileTypeCount[] {
  const counts = new Map<string, number>();
  for (const doc of documents) {
    const label = labelForFileType(doc.fileType);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([label, count]) => ({ label, count }))
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label))
    .slice(0, limit);
}

function newestIndexedAt(documents: DocumentMetadata[]): string | null {
  let newest: string | null = null;
  let newestMs = Number.NEGATIVE_INFINITY;
  for (const doc of documents) {
    const ms = new Date(doc.indexedAt).getTime();
    if (!Number.isNaN(ms) && ms > newestMs) {
      newestMs = ms;
      newest = doc.indexedAt;
    }
  }
  return newest;
}

export const useDashboardQuery = (
  options?: Omit<UseQueryOptions<DashboardData, Error>, 'queryKey' | 'queryFn'>,
) =>
  useQuery<DashboardData, Error>({
    queryKey: ['dashboard'],
    queryFn: async () => {
      const [
        statsResult,
        docsResult,
        systemStatsResult,
        foldersResult,
        conversationsResult,
        journalsResult,
        bookmarksResult,
      ] = await Promise.all([
        VaultAPI.getIndexingStats(),
        VaultAPI.listAllDocuments(10000),
        VaultAPI.getSystemStats(),
        VaultAPI.getIndexedFolders(),
        VaultAPI.listConversations(),
        VaultAPI.listJournals(),
        VaultAPI.listMessageBookmarks({ limit: 10, offset: 0 }),
      ]);

      if (!statsResult.ok || !docsResult.ok) {
        throw new Error('Failed to fetch dashboard data');
      }

      const documents: DocumentMetadata[] = docsResult.data ?? [];

      const journals: ConversationJournalDto[] = journalsResult.ok ? journalsResult.data : [];
      const journalSpaceIds = new Set(journals.map((j) => j.id));
      const preferredJournal = pickPreferredJournal(journals);

      const allConversations: ConversationDto[] = conversationsResult.ok
        ? conversationsResult.data.conversations
        : [];
      const recentConversations = selectRecentChatConversations(allConversations, journalSpaceIds);

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
          documentCount: statsResult.data?.indexedDocuments || documents.length,
          storageBytes,
          folderCount,
          lastIndexedAt: newestIndexedAt(documents),
          fileTypes: summarizeFileTypes(documents),
        },
        recentDocuments: [...documents]
          .sort((a, b) => new Date(b.indexedAt).getTime() - new Date(a.indexedAt).getTime())
          .slice(0, 10)
          .map((doc) => ({
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
