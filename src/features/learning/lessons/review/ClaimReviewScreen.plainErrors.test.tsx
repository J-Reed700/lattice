import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review } from '@/features/learning/lessons/review/claimReviewTestData';
import { expectPlainError, findAlertWith, PLAIN_ERROR_CASES } from '@/tests/plainErrors';

/**
 * A claim action that fails (Remove it, Accept, ...) says so in a plain
 * sentence on the claim; the backend's text is under Details.
 */

const api = vi.hoisted(() => ({
  review: vi.fn(), resolve: vi.fn(), audit: vi.fn(), trust: vi.fn(), plan: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve,
  getLearningLessonAudit: api.audit, getLearningSourceTrust: api.trust, getLearningPlan: api.plan,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });

function show() {
  api.review.mockResolvedValue(ok(review([waitingOnSource(1)])));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

describe('Claim review: a failed claim action', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });

  describe.each(PLAIN_ERROR_CASES)('when $name', (errorCase) => {
    it('says so plainly on the claim, with the backend text under Details', async () => {
      api.resolve.mockResolvedValue(errorCase.failure);
      show();
      await userEvent.click(await screen.findByRole('button', { name: 'Remove it' }));
      const alert = await findAlertWith(errorCase.headline);
      await expectPlainError(alert, errorCase);
    });
  });

  it('shows no alert while the action succeeds', async () => {
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    show();
    await userEvent.click(await screen.findByRole('button', { name: 'Remove it' }));
    expect(api.resolve).toHaveBeenCalledOnce();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
