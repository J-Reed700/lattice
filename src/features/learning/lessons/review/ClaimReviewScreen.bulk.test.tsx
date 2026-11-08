import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  review: vi.fn(), resolve: vi.fn(), resolveMany: vi.fn(), audit: vi.fn(), trust: vi.fn(), plan: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, resolveLearningClaims: api.resolveMany,
  getLearningLessonAudit: api.audit, getLearningSourceTrust: api.trust, getLearningPlan: api.plan,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });

function show(data: LessonClaimReviewDto | LessonClaimReviewDto[]) {
  const sequence = Array.isArray(data) ? data : [data];
  sequence.forEach((value) => api.review.mockResolvedValueOnce(ok(value)));
  api.review.mockResolvedValue(ok(sequence[sequence.length - 1]));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

const four = () => [1, 2, 3, 4].map((index) => waitingOnSource(index));
const confirmed = (claims: ClaimReviewItemDto[], ids: string[]) => claims.map((claim) => (ids.includes(claim.claimId) ? { ...claim, state: { state: 'confirmed' as const, at: 2 } } : claim));
const bar = () => screen.getByRole('toolbar', { name: /selected$/ });
const boxes = () => screen.getAllByRole('checkbox', { name: 'Select this statement' });
const selectAll = async () => userEvent.click((await screen.findAllByRole('button', { name: /^Select all \d+$/ }))[0]);

describe('Claim review screen, many statements at once', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });

  it('selects a range with shift and counts it in the bar', async () => {
    show(review(four()));
    await screen.findAllByRole('checkbox', { name: 'Select this statement' });
    fireEvent.click(boxes()[0]);
    expect(bar()).toHaveTextContent('1 statement selected');
    fireEvent.click(boxes()[2], { shiftKey: true });
    expect(bar()).toHaveTextContent('3 statements selected');
  });

  it('selects the whole group, and Select none clears it', async () => {
    show(review(four()));
    await selectAll();
    expect(bar()).toHaveTextContent('4 statements selected');
    await userEvent.click(screen.getByRole('button', { name: 'Select none' }));
    expect(screen.queryByRole('toolbar', { name: /selected$/ })).not.toBeInTheDocument();
  });

  it('accepts every selected statement in one call with the review revision, says so, and clears the selection', async () => {
    const claims = four();
    show([review(claims), review(confirmed(claims, ['claim-1', 'claim-2']), { revision: 8 })]);
    await screen.findAllByRole('checkbox', { name: 'Select this statement' });
    fireEvent.click(boxes()[0]);
    fireEvent.click(boxes()[1]);
    api.resolveMany.mockResolvedValue(ok({ revision: 8 }));
    await userEvent.click(within(bar()).getByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1', lessonId: 'lesson-1', claimIds: ['claim-1', 'claim-2'], expectedRevision: 7,
      action: { action: 'confirm', note: null, sourceVersionIds: [] },
    }));
    expect(api.resolve).not.toHaveBeenCalled();
    expect(await screen.findByRole('status')).toHaveTextContent('Accepted 2 statements.');
    expect(screen.queryByRole('toolbar', { name: /selected$/ })).not.toBeInTheDocument();
  });

  it('removes every selected statement in one call', async () => {
    const claims = four();
    show([review(claims), review(claims.slice(2), { revision: 8 })]);
    await selectAll();
    api.resolveMany.mockResolvedValue(ok({ revision: 8 }));
    await userEvent.click(within(bar()).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledWith(expect.objectContaining({
      claimIds: ['claim-1', 'claim-2', 'claim-3', 'claim-4'], expectedRevision: 7, action: { action: 'remove' },
    })));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed 4 statements.');
  });

  it('undoes the batch with the revision it returned, and says how many wait again', async () => {
    const claims = four();
    show([review(claims), review(confirmed(claims, ['claim-1', 'claim-2']), { revision: 8 }), review(claims, { revision: 9 })]);
    await screen.findAllByRole('checkbox', { name: 'Select this statement' });
    fireEvent.click(boxes()[0]);
    fireEvent.click(boxes()[1]);
    api.resolveMany.mockResolvedValueOnce(ok({ revision: 8 }));
    await userEvent.click(within(bar()).getByRole('button', { name: 'Accept' }));
    const line = await screen.findByRole('status');
    await waitFor(() => expect(line).toHaveTextContent('Accepted 2 statements.'));
    api.resolveMany.mockResolvedValueOnce(ok({ revision: 9 }));
    await userEvent.click(within(line).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledTimes(2));
    expect(api.resolveMany).toHaveBeenLastCalledWith(expect.objectContaining({
      claimIds: ['claim-1', 'claim-2'], expectedRevision: 8, action: { action: 'undo' },
    }));
    expect(await screen.findByRole('status')).toHaveTextContent('Undone. 2 statements wait for you again.');
  });

  it('shows the refusal in the status line and keeps the selection when a batch is refused', async () => {
    show(review(four()));
    await selectAll();
    api.resolveMany.mockResolvedValue({ ok: false, error: "1 of these can't be removed: It has already been removed or replaced. Nothing was changed." });
    await userEvent.click(within(bar()).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Nothing was changed.'));
    expect(bar()).toHaveTextContent('4 statements selected');
  });

  it('disables Accept with the hint while a selected statement needs a note, and Remove still works', async () => {
    const claims = four();
    claims[1] = claimItem({ ...claims[1], verdictKind: { kind: 'contradicted' } });
    show(review(claims));
    await selectAll();
    expect(within(bar()).getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(within(bar()).getByRole('button', { name: 'Remove' })).toBeEnabled();
    expect(bar()).toHaveTextContent('Some of these need a note. Accept them one at a time.');
  });

  it('disables an action a selected statement does not offer, and says so', async () => {
    const claims = four();
    claims[2] = { ...claims[2], availableActions: ['confirm', 'edit'] };
    show(review(claims));
    await selectAll();
    expect(within(bar()).getByRole('button', { name: 'Remove' })).toBeDisabled();
    expect(within(bar()).getByRole('button', { name: 'Accept' })).toBeEnabled();
    expect(bar()).toHaveTextContent("Some of these can't be removed here.");
  });

  it("offers Remove it on a published lesson's statement that can be removed", async () => {
    const claims = four().map((claim) => ({ ...claim, availableActions: ['edit', 'remove'] as ClaimReviewItemDto['availableActions'] }));
    show(review(claims, { state: 'ready' }));
    expect(await screen.findByRole('button', { name: 'Remove it' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
  });
});
