import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useDashboardQuery } from '@/features/dashboard/hooks/useDashboardQuery';

const getIndexingStats = vi.fn();
const listAllDocuments = vi.fn();
const getCorpusShape = vi.fn();
const listConversationSpaces = vi.fn();
const listLearningPrograms = vi.fn();
const getLearningProgram = vi.fn();

vi.mock('@/features/learning/api/client', () => ({
  learningApi: {
    listLearningPrograms: () => listLearningPrograms(),
    getLearningProgram: (id: string) => getLearningProgram(id),
  },
}));

vi.mock('@/lib/api', () => {
  const api = {
    getIndexingStats: () => getIndexingStats(),
    listAllDocuments: (limit?: number) => listAllDocuments(limit),
    getCorpusShape: () => getCorpusShape(),
    getSystemStats: async () => ({ ok: true, data: { total_documents: 0, total_chunks: 0, total_tags: 0, storage_size_bytes: 0 } }),
    getIndexedFolders: async () => ({ ok: true, data: [] }),
    listConversations: async () => ({ ok: true, data: { conversations: [] } }),
    listJournals: async () => ({ ok: true, data: [] }),
    listMessageBookmarks: async () => ({ ok: true, data: { bookmarks: [] } }),
    listConversationSpaces: () => listConversationSpaces(),
  };
  return { VaultAPI: api, default: api };
});

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

describe('useDashboardQuery failure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getIndexingStats.mockResolvedValue({ ok: true, data: { indexedDocuments: 0, totalChunks: 0 } });
    listAllDocuments.mockResolvedValue({ ok: true, data: [] });
    getCorpusShape.mockResolvedValue({
      ok: true,
      data: { total: 0, byType: [], grownLast7Days: 0, grownToday: 0, lastIndexedAt: null },
    });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listLearningPrograms.mockResolvedValue({ ok: true, data: [] });
    getLearningProgram.mockResolvedValue({ ok: false, error: 'not found' });
  });

  it('carries the backend reason when the library stats cannot be read', async () => {
    getIndexingStats.mockResolvedValue({ ok: false, error: 'The library database is locked.' });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.error?.message).toBe('The library database is locked.'));
  });

  it('carries the backend reason when the document list cannot be read', async () => {
    listAllDocuments.mockResolvedValue({ ok: false, error: 'Disk read failed' });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.error?.message).toBe('Disk read failed'));
  });

  it('carries the backend reason when the library summary cannot be read', async () => {
    getCorpusShape.mockResolvedValue({ ok: false, error: 'Summary query failed' });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.error?.message).toBe('Summary query failed'));
  });
});

describe('useDashboardQuery library readout', () => {
  const document = (id: string, indexedAt: string) => ({
    id,
    fileName: `${id}.md`,
    filePath: `/library/${id}.md`,
    fileType: 'md',
    indexedAt,
    modifiedAt: indexedAt,
  });

  beforeEach(() => {
    vi.clearAllMocks();
    getIndexingStats.mockResolvedValue({ ok: true, data: { indexedDocuments: 4321, totalChunks: 0 } });
    getCorpusShape.mockResolvedValue({
      ok: true,
      data: {
        total: 4321,
        byType: [
          { type: 'PDF', count: 9 },
          { type: 'Markdown', count: 8 },
          { type: 'Web', count: 7 },
          { type: 'Text', count: 6 },
          { type: 'Word', count: 5 },
          { type: 'CSV', count: 4 },
        ],
        grownLast7Days: 12,
        grownToday: 2,
        lastIndexedAt: '2026-05-14T09:30:00+00:00',
      },
    });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listLearningPrograms.mockResolvedValue({ ok: true, data: [] });
    getLearningProgram.mockResolvedValue({ ok: false, error: 'not found' });
  });

  it('asks for the ten newest documents and the library summary', async () => {
    listAllDocuments.mockResolvedValue({ ok: true, data: [] });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(listAllDocuments).toHaveBeenCalledWith(10);
    expect(getCorpusShape).toHaveBeenCalledTimes(1);
  });

  it('takes the file types and the last indexing time from the summary, not from the listed documents', async () => {
    listAllDocuments.mockResolvedValue({
      ok: true,
      data: [document('newer', '2026-06-01T00:00:00+00:00'), document('older', '2026-05-01T00:00:00+00:00')],
    });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.data).toBeDefined());
    const stats = result.current.data?.stats;
    expect(stats?.fileTypes).toEqual([
      { label: 'PDF', count: 9 },
      { label: 'Markdown', count: 8 },
      { label: 'Web', count: 7 },
      { label: 'Text', count: 6 },
      { label: 'Word', count: 5 },
    ]);
    expect(stats?.lastIndexedAt).toBe('2026-05-14T09:30:00+00:00');
    expect(stats?.documentCount).toBe(4321);
  });

  it('lists the documents in the order the server returned them', async () => {
    listAllDocuments.mockResolvedValue({
      ok: true,
      data: [document('first', '2026-06-01T00:00:00+00:00'), document('second', '2026-05-01T00:00:00+00:00')],
    });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data?.recentDocuments.map((doc) => doc.id)).toEqual(['first', 'second']);
  });

  it('loads active Learning Studio courses with current module and lesson details, plus open spaces', async () => {
    listAllDocuments.mockResolvedValue({ ok: true, data: [] });
    listLearningPrograms.mockResolvedValue({
      ok: true,
      data: [
        {
          id: 'p1',
          title: 'Distributed Systems',
          goal: 'Understand consensus',
          status: 'active',
          revision: 1,
          moduleCount: 2,
          lessonCount: 6,
          completedLessons: 2,
          currentLessonId: 'l-raft',
          createdAt: 1700000100,
        },
      ],
    });
    getLearningProgram.mockResolvedValue({
      ok: true,
      data: {
        summary: {
          id: 'p1',
          title: 'Distributed Systems',
          goal: 'Understand consensus',
          status: 'active',
          revision: 1,
          moduleCount: 2,
          lessonCount: 6,
          completedLessons: 2,
          currentLessonId: 'l-raft',
          createdAt: 1700000100,
        },
        modules: [
          {
            id: 'm2',
            title: 'Consensus Protocols',
            summary: '',
            outcomes: [],
            lessons: [
              { id: 'l-raft', title: 'Raft Leader Election', objective: '', estimatedMinutes: 18, preparation: 'ready', blocks: [], completed: false },
            ],
          },
        ],
      },
    });
    listConversationSpaces.mockResolvedValue({
      ok: true,
      data: [
        { id: 's-1', name: 'Systems', description: null, icon: null, accentColor: '#0f766e', spacePrompt: null, toolPreferencesJson: null, isArchived: false, sortOrder: 1, createdAt: '', updatedAt: '' },
        { id: 's-arch', name: 'Archived', description: null, icon: null, accentColor: null, spacePrompt: null, toolPreferencesJson: null, isArchived: true, sortOrder: 2, createdAt: '', updatedAt: '' },
      ],
    });

    const { result } = renderHook(() => useDashboardQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toBeDefined());

    expect(result.current.data?.courses).toEqual([
      expect.objectContaining({
        id: 'p1',
        title: 'Distributed Systems',
        currentLessonId: 'l-raft',
        currentLessonTitle: 'Raft Leader Election',
        currentModuleTitle: 'Consensus Protocols',
        currentLessonMinutes: 18,
      }),
    ]);
    expect(result.current.data?.spaces).toEqual([
      expect.objectContaining({ id: 's-1', name: 'Systems' }),
    ]);
  });

  it('degrades gracefully when learning or spaces endpoints fail', async () => {
    listAllDocuments.mockResolvedValue({ ok: true, data: [] });
    listLearningPrograms.mockRejectedValue(new Error('studio unavailable'));
    listConversationSpaces.mockResolvedValue({ ok: false, error: 'spaces locked' });

    const { result } = renderHook(() => useDashboardQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toBeDefined());

    expect(result.current.data?.courses).toEqual([]);
    expect(result.current.data?.spaces).toEqual([]);
  });
});
