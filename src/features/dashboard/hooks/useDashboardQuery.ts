import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningApi } from '@/features/learning/api/client';
import type {
  LearningProgramDto,
  LearningProgramStatus,
  LearningProgramSummaryDto,
} from '@/lib/bindings';
import { describeFailure } from '@/lib/errorHandling';
import type { ConversationSpaceDto, RecentDocument } from '@/types';
import type {
  ConversationDto,
  ConversationJournalDto,
  ConversationMessageBookmarkDto,
} from '@/types/api/conversation';

const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const RECENT_DOCUMENTS = 10;
const TOP_FILE_TYPES = 5;
const TOP_COURSES = 3;

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

export interface DashboardCourseSummary {
  id: string;
  title: string;
  goal: string;
  status: LearningProgramStatus;
  moduleCount: number;
  lessonCount: number;
  completedLessons: number;
  currentLessonId: string | null;
  currentLessonTitle: string | null;
  currentModuleTitle: string | null;
  currentLessonMinutes: number | null;
  createdAt: number;
}

export interface DashboardSpaceSummary {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  accentColor: string | null;
}

export interface DashboardData {
  stats: DashboardStats;
  recentDocuments: RecentDocument[];
  recentConversations: DashboardConversationSummary[];
  todayJournalEntry: DashboardJournalEntrySummary | null;
  preferredJournalSpaceId: string | null;
  recentReferences: ConversationMessageBookmarkDto[];
  journalSpaceIds: string[];
  courses: DashboardCourseSummary[];
  spaces: DashboardSpaceSummary[];
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

function rankCourses(programs: readonly LearningProgramSummaryDto[]): LearningProgramSummaryDto[] {
  const priority = (program: LearningProgramSummaryDto): number => {
    const inProgress = program.status === 'active' && program.completedLessons < program.lessonCount;
    if (inProgress) return 0;
    if (program.status === 'active') return 1;
    return 2;
  };
  return [...programs]
    .sort((a, b) => priority(a) - priority(b) || b.createdAt - a.createdAt)
    .slice(0, TOP_COURSES);
}

function resolveCourseLessonContext(
  summary: LearningProgramSummaryDto,
  detail: LearningProgramDto | null,
): Pick<
  DashboardCourseSummary,
  'currentLessonId' | 'currentLessonTitle' | 'currentModuleTitle' | 'currentLessonMinutes'
> {
  if (!detail || !Array.isArray(detail.modules) || detail.modules.length === 0) {
    return {
      currentLessonId: summary.currentLessonId,
      currentLessonTitle: null,
      currentModuleTitle: null,
      currentLessonMinutes: null,
    };
  }

  if (summary.currentLessonId) {
    for (const mod of detail.modules) {
      const match = mod.lessons.find((lesson) => lesson.id === summary.currentLessonId);
      if (match) {
        return {
          currentLessonId: match.id,
          currentLessonTitle: match.title,
          currentModuleTitle: mod.title,
          currentLessonMinutes: match.estimatedMinutes > 0 ? match.estimatedMinutes : null,
        };
      }
    }
  }

  for (const mod of detail.modules) {
    const nextIncomplete = mod.lessons.find((lesson) => !lesson.completed);
    if (nextIncomplete) {
      return {
        currentLessonId: nextIncomplete.id,
        currentLessonTitle: nextIncomplete.title,
        currentModuleTitle: mod.title,
        currentLessonMinutes:
          nextIncomplete.estimatedMinutes > 0 ? nextIncomplete.estimatedMinutes : null,
      };
    }
  }

  for (let i = detail.modules.length - 1; i >= 0; i -= 1) {
    const mod = detail.modules[i];
    const lastLesson = mod.lessons[mod.lessons.length - 1];
    if (lastLesson) {
      return {
        currentLessonId: lastLesson.id,
        currentLessonTitle: lastLesson.title,
        currentModuleTitle: mod.title,
        currentLessonMinutes: lastLesson.estimatedMinutes > 0 ? lastLesson.estimatedMinutes : null,
      };
    }
  }

  return {
    currentLessonId: summary.currentLessonId,
    currentLessonTitle: null,
    currentModuleTitle: null,
    currentLessonMinutes: null,
  };
}

async function fetchDashboardCourses(): Promise<DashboardCourseSummary[]> {
  try {
    const listResult = await learningApi.listLearningPrograms();
    if (!listResult.ok || !Array.isArray(listResult.data) || listResult.data.length === 0) {
      return [];
    }

    const topPrograms = rankCourses(listResult.data);
    const details = await Promise.all(
      topPrograms.map(async (program) => {
        try {
          const detailResult = await learningApi.getLearningProgram(program.id);
          return detailResult.ok ? detailResult.data : null;
        } catch {
          return null;
        }
      }),
    );

    return topPrograms.map((summary, index) => {
      const lessonContext = resolveCourseLessonContext(summary, details[index]);
      return {
        id: summary.id,
        title: summary.title,
        goal: summary.goal,
        status: summary.status,
        moduleCount: summary.moduleCount,
        lessonCount: summary.lessonCount,
        completedLessons: summary.completedLessons,
        createdAt: summary.createdAt,
        ...lessonContext,
      };
    });
  } catch {
    return [];
  }
}

async function fetchDashboardSpaces(
  listSpaces?: () => Promise<{ ok: true; data: ConversationSpaceDto[] } | { ok: false; error: string }>,
): Promise<DashboardSpaceSummary[]> {
  if (typeof listSpaces !== 'function') return [];
  try {
    const result = await listSpaces();
    if (!result.ok || !Array.isArray(result.data)) return [];
    return [...result.data]
      .filter((space) => !space.isArchived)
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((space) => ({
        id: space.id,
        name: space.name,
        description: space.description,
        icon: space.icon,
        accentColor: space.accentColor,
      }));
  } catch {
    return [];
  }
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
        courses,
        spaces,
      ] = await Promise.all([
        api.getIndexingStats(),
        api.listAllDocuments(RECENT_DOCUMENTS),
        api.getCorpusShape(),
        api.getSystemStats(),
        api.getIndexedFolders(),
        api.listConversations(),
        api.listJournals(),
        api.listMessageBookmarks({ conversationId: null, query: null, limit: 10, offset: 0 }),
        fetchDashboardCourses(),
        fetchDashboardSpaces(
          typeof api.listConversationSpaces === 'function'
            ? () => api.listConversationSpaces()
            : undefined,
        ),
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
          query: null,
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
        courses,
        spaces,
      };
    },
    staleTime: 30_000,
    ...options,
  });
};
