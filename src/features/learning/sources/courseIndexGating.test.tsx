import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import { LessonClaimReviewPanel } from '@/features/learning/lessons/review/LessonClaimReviewPanel';
import { SourceMaintenancePanel } from '@/features/learning/sources/SourceMaintenancePanel';
import { EVIDENCE_WAITS_FOR_INDEX } from '@/features/learning/sources/useCourseIndexStatus';
import type { CourseIndexStatusDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  status: vi.fn(), review: vi.fn(), audit: vi.fn(), workspace: vi.fn(), version: vi.fn(), semantic: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  courseIndexStatus: api.status,
  getLearningLessonClaimReview: api.review,
  getLearningLessonAudit: api.audit,
  getLearningSourceTrust: async () => ({ ok: true, data: { sources: [], claim: null } }),
  getLearningSourceWorkspace: api.workspace,
  getLearningSourceVersion: api.version,
  searchLearningSourcesSemantically: api.semantic,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const INDEXING: CourseIndexStatusDto = { state: 'indexing', embedded: 10, total: 90, referencesNew: 2, passagesPerSecond: null };
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

  it('holds back adding evidence and resuming verification, and says why', async () => {
    api.status.mockResolvedValue(ok(INDEXING));
    wrap(<LessonClaimReviewPanel programId="program-1" lessonId="lesson-1" courseRevision={4} />);
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    await waitFor(() => expect(resume).toBeDisabled());
    expect(resume).toHaveAttribute('title', EVIDENCE_WAITS_FOR_INDEX);
    expect(screen.getByText(EVIDENCE_WAITS_FOR_INDEX)).toBeVisible();
    const attach = screen.getByRole('button', { name: 'Add evidence' });
    expect(attach).toBeDisabled();
    expect(attach).toHaveAttribute('title', EVIDENCE_WAITS_FOR_INDEX);
    // Reading and judging the claim stay open.
    expect(screen.getByRole('button', { name: 'Edit the sentence' })).toBeEnabled();
  });

  it.each([['ready', READY], ['searched by words only', WORDS_ONLY]])('leaves evidence actions open when the course is %s', async (_, status) => {
    api.status.mockResolvedValue(ok(status));
    wrap(<LessonClaimReviewPanel programId="program-1" lessonId="lesson-1" courseRevision={4} />);
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    await waitFor(() => expect(api.status).toHaveBeenCalledWith('program-1'));
    expect(resume).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Add evidence' })).toBeEnabled();
  });

  it('holds back source search while indexing, and opens it when the course is ready or searched by words', async () => {
    const user = userEvent.setup();
    for (const [status, held] of [[INDEXING, true], [READY, false], [WORDS_ONLY, false]] as const) {
      api.status.mockResolvedValue(ok(status));
      const { unmount } = wrap(<SourceMaintenancePanel programId="program-1" />);
      await user.type(await screen.findByLabelText('Search related saved material'), 'method');
      const search = screen.getByRole('button', { name: 'Search program source versions' });
      await waitFor(() => expect(api.status).toHaveBeenCalled());
      if (held) {
        await waitFor(() => expect(search).toBeDisabled());
        expect(search).toHaveAttribute('title', EVIDENCE_WAITS_FOR_INDEX);
      } else {
        await waitFor(() => expect(search).toBeEnabled());
      }
      unmount();
      api.status.mockClear();
    }
  });
});
