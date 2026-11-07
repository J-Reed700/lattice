import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

// The review screen serves a published lesson's unverified statements too (design-settle-published.md): its
// Edit form says the user's sentence replaces the published one, where a draft's says it is checked on resume.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const PUBLISHED = 'Your sentence replaces this one in the lesson.';
const DRAFT = 'The new sentence is checked against your sources when you resume.';

const item = (index: number, overrides: Partial<ClaimReviewItemDto> = {}): ClaimReviewItemDto => {
  const statement = `Monarch fact number ${index}.`;
  return claimItem({
    claimId: `claim-${index}`, statement, availableActions: ['confirm', 'edit', 'attach_evidence'],
    presentation: { statement, sentence: statement, contextBefore: '', contextAfter: '', place: `Section ${index}` }, ...overrides,
  });
};
const unverified = (index: number) => item(index, { state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null });
const focused = () => document.querySelector<HTMLElement>('[data-claim-statement]');

function show(data: LessonClaimReviewDto, focusClaimId?: string) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} focusClaimId={focusClaimId} />
  </QueryClientProvider>);
}

describe('Claim review screen of a published lesson', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });

  it('says in the edit form of a ready lesson that the user’s sentence replaces the published one', async () => {
    show(review([unverified(1)], { state: 'ready' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Edit the sentence' }));
    expect(screen.getByText(PUBLISHED)).toBeVisible();
    expect(screen.queryByText(DRAFT)).not.toBeInTheDocument();
  });

  it('keeps the draft’s note in the edit form of a lesson held for review', async () => {
    show(review([item(1, { state: { state: 'queued' } })], { state: 'needs_review' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Edit the sentence' }));
    expect(screen.getByText(DRAFT)).toBeVisible();
    expect(screen.queryByText(PUBLISHED)).not.toBeInTheDocument();
  });

  it('opens on the claim it is asked to focus', async () => {
    show(review([unverified(1), unverified(2), unverified(3)], { state: 'ready' }), 'claim-2');
    await waitFor(() => expect(focused()).toHaveTextContent('Monarch fact number 2.'));
  });
});
