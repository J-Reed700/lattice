import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import { SourceMaintenancePanel } from '@/features/learning/sources/SourceMaintenancePanel';
import { useCourseIndexStatusEvents } from '@/features/learning/sources/useCourseIndexStatus';
import type { CourseIndexStatusDto, JobDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  status: vi.fn(), review: vi.fn(), audit: vi.fn(), workspace: vi.fn(), version: vi.fn(), semantic: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  courseIndexStatus: api.status,
  getLearningLessonClaimReview: api.review,
  getLearningLessonAudit: api.audit,
  getLearningPlan: async () => ({ ok: true, data: { jobs: [] } }),
  getLearningSourceTrust: async () => ({ ok: true, data: { sources: [], claim: null } }),
  getLearningSourceWorkspace: api.workspace,
  getLearningSourceVersion: api.version,
  searchLearningSourcesSemantically: api.semantic,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const INDEXING: CourseIndexStatusDto = { state: 'indexing', embedded: 10, total: 90, referencesNew: 2, passagesPerSecond: null, waitingTurn: false };
const MEASURING = 'Waits for the course index · measuring speed…';
const READY: CourseIndexStatusDto = { state: 'ready', mode: 'hybrid' };
const WORDS_ONLY: CourseIndexStatusDto = { state: 'noEmbeddingModel' };
const version = { id: 'version-1', versionNumber: 1, title: 'Field manual', publisher: null, resolvedUrl: null, excerpt: 'A method.', contentSha256: 'sha', wordCount: 2, truncated: false, extractionVersion: 'text-v1', acquiredAt: 1 };
const source = { id: 'source-1', kind: 'text', origin: 'Field manual', requestedUrl: null, freshnessPolicy: 'fixed', activeVersionId: 'version-1', pendingVersionId: null, revision: 1, activeVersion: version, pendingVersion: null, versions: [version], latestCheck: null, checks: [], createdAt: 1, updatedAt: 1, deletedAt: null, deletionReason: null };

function wrap(node: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

describe('Evidence actions while a course indexes', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.review.mockResolvedValue(ok(review([claimItem()], { changedSinceStop: true })));
    api.workspace.mockResolvedValue(ok({ programId: 'program-1', sources: [source] }));
    api.version.mockResolvedValue(ok({ sourceId: 'source-1', version, fullText: 'A method.', usage: [] }));
    api.semantic.mockResolvedValue(ok([]));
  });

  it('holds back resuming verification and says why, but leaves adding evidence open', async () => {
    api.status.mockResolvedValue(ok(INDEXING));
    wrap(<ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Reading field notes" courseRevision={4} onBack={() => undefined} />);
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    await waitFor(() => expect(resume).toBeDisabled());
    expect(screen.getByText(MEASURING)).toBeVisible();
    expect(resume).toHaveAccessibleDescription(MEASURING);
    // Adding a source reads the course's words at once, as do reading and judging the claim.
    const attach = screen.getByRole('button', { name: 'Add a source' });
    expect(attach).toBeEnabled();
    expect(attach).not.toHaveAttribute('title');
    expect(screen.getByRole('button', { name: 'Edit the sentence' })).toBeEnabled();
  });

  it('says how long verification waits, and opens Resume itself once the index finishes', async () => {
    api.status.mockResolvedValue(ok({ ...INDEXING, embedded: 4_210, total: 13_457, passagesPerSecond: 14 }));
    // The course workspace's notice listens for the build's progress; here, on its own.
    function StatusEvents() {
      useCourseIndexStatusEvents('program-1');
      return null;
    }
    wrap(<><StatusEvents /><ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Reading field notes" courseRevision={4} onBack={() => undefined} /></>);
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    expect(await screen.findByText('Waits for the course index · about 11 min left')).toBeVisible();
    expect(resume).toBeDisabled();
    const handler = vi.mocked(listen).mock.calls.find(([event]) => event === 'jobs://status')?.[1];
    expect(handler).toBeDefined();
    api.status.mockResolvedValue(ok(READY));
    handler!({ payload: { kind: 'learning.course_index', subjectId: 'program-1', status: 'completed' } as JobDto, event: 'jobs://status', id: 1 });
    await waitFor(() => expect(resume).toBeEnabled());
    expect(screen.queryByText(/Waits for the course index/)).not.toBeInTheDocument();
  });

  it.each([['ready', READY], ['searched by words only', WORDS_ONLY]])('leaves evidence actions open when the course is %s', async (_, status) => {
    api.status.mockResolvedValue(ok(status));
    wrap(<ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Reading field notes" courseRevision={4} onBack={() => undefined} />);
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    await waitFor(() => expect(api.status).toHaveBeenCalledWith('program-1'));
    expect(resume).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Add a source' })).toBeEnabled();
  });

  it('searches the course’s sources whether it is indexing, ready or searched by words', async () => {
    const user = userEvent.setup();
    for (const status of [INDEXING, READY, WORDS_ONLY]) {
      api.status.mockResolvedValue(ok(status));
      const { unmount } = wrap(<SourceMaintenancePanel programId="program-1" />);
      await user.type(await screen.findByLabelText('Search related saved material'), 'method');
      const search = screen.getByRole('button', { name: 'Search program source versions' });
      expect(search).toBeEnabled();
      await user.click(search);
      await waitFor(() => expect(api.semantic).toHaveBeenCalledWith({ programId: 'program-1', query: 'method', limit: 20 }));
      unmount();
      api.semantic.mockClear();
    }
  });

  it('says it adds the course’s new references to its word index while the first search waits, then reads the status again', async () => {
    const user = userEvent.setup();
    let answer: (value: unknown) => void = () => {};
    api.semantic.mockReturnValue(new Promise((resolve) => { answer = resolve; }));
    api.status.mockResolvedValue(ok({ ...INDEXING, embedded: 0, total: 0, referencesNew: 402 }));
    wrap(<SourceMaintenancePanel programId="program-1" />);
    await waitFor(() => expect(api.status).toHaveBeenCalledTimes(1));
    await user.type(await screen.findByLabelText('Search related saved material'), 'method');
    await user.click(screen.getByRole('button', { name: 'Search program source versions' }));
    expect(await screen.findByText('Adding 402 new references to the course’s word index, then searching…')).toBeVisible();
    api.status.mockResolvedValue(ok({ ...INDEXING, embedded: 0, total: 13457, referencesNew: 0 }));
    answer(ok([]));
    await waitFor(() => expect(api.status).toHaveBeenCalledTimes(2));
  });
});
