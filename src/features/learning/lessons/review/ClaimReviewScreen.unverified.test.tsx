import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

// The review screen of a lesson that published with statements it could not verify says so, in place of
// "Every claim is verified ..." (design-publish-unverified.md, "Wording").
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 12, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };
const OLD = 'Every claim is verified, confirmed by you, removed or edited.';

const unverified = (index: number): ClaimReviewItemDto => claimItem({
  claimId: `claim-${index}`, statement: `Unverified statement ${index}.`, state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
});
const verified = claimItem({ claimId: 'claim-v', state: { state: 'resolved_by_evidence' }, verdictKind: null, reason: null, availableActions: [] });

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

describe('Claim review screen of a lesson with statements that are not verified', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });

  // The sentence is asserted wherever it renders, so it must show whether or not an unverified statement is in focus.
  it.each([
    [1, '1 statement could not be verified. You can check each one here; the lesson is already published.'],
    [2, '2 statements could not be verified. You can check each one here; the lesson is already published.'],
  ])('says %i unverified statement(s) could not be verified, and that the lesson is already published', async (count, sentence) => {
    show(review([verified, ...Array.from({ length: count }, (_, index) => unverified(index + 1))], { state: 'ready', counts }));
    expect(await screen.findByText(sentence)).toBeVisible();
    expect(screen.queryByText(OLD)).not.toBeInTheDocument();
  });

  it('keeps the old sentence for a ready lesson with no unverified statement', async () => {
    show(review([verified], { state: 'ready', counts }));
    expect(await screen.findByText(OLD)).toBeVisible();
    expect(screen.queryByText(/could not be verified/)).not.toBeInTheDocument();
  });
});
