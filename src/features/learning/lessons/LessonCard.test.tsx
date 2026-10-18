import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { review, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LearningGenerationJob, LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: async () => ({ ok: true, data: null }),
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));

const lesson: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'outline', blocks: [], questions: [], completed: false };
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;
const held: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 44,
  status: 'completed', progressCompleted: 1, progressTotal: 1, progressMessage: 'Needs your review: 3 claims', activity: null,
  resultId: 'needs_review:lesson-1', error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: 2,
};
const notFound = { ok: false, error: 'This lesson has no claim review yet.', details: { code: 'NOT_FOUND', message: 'This lesson has no claim review yet.' } };

function show(card: LearningLessonDto) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <LessonCard lesson={card} moduleId="module-1" program={program} isCurrent={false} selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} />
  </QueryClientProvider>);
}

describe('Lesson card review state', () => {
  beforeEach(() => vi.resetAllMocks());

  it('says "Needs your review" with the count, never failed, and opens onto the claim list', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [held] } });
    api.review.mockResolvedValue({ ok: true, data: review([unsupportedItem(1), unsupportedItem(2), unsupportedItem(3)]) });
    show(lesson);
    const card = screen.getByRole('button', { name: /Phase changes/ });
    expect(await within(card).findByText('Needs your review · 3 claims')).toBeVisible();
    expect(within(card).getByText(/Open the lesson to see each one/)).toBeVisible();
    expect(within(card).queryByText(/failed|Outline/)).not.toBeInTheDocument();
    await userEvent.click(card);
    expect(await screen.findByRole('region', { name: 'Claim review' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Prepare lesson' })).not.toBeInTheDocument();
  });

  it('keeps an outline lesson with no review as it was', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue(notFound);
    show(lesson);
    expect(await screen.findByText('Outline')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: /Phase changes/ }));
    expect(await screen.findByRole('button', { name: 'Prepare lesson' })).toBeVisible();
    expect(screen.queryByRole('region', { name: 'Claim review' })).not.toBeInTheDocument();
  });
});
