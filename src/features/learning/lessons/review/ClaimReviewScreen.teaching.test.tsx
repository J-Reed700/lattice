import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review, teachingFinding, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto, TeachingFindingDto } from '@/lib/bindings';

// What the screen says while teaching notes wait: never that everything is resolved.
const api = vi.hoisted(() => ({ review: vi.fn(), resolve: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, getLearningLessonAudit: api.audit,
  getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = (queued: number) => ({ verified: 12, confirmed: 0, queued, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 });

const note = (index: number, overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: `teaching:section-${index}`, unitIndex: index, unitTitle: `Section ${index + 1}: Part ${index + 1}`, ...overrides,
});

function show(data: LessonClaimReviewDto | LessonClaimReviewDto[]) {
  const sequence = Array.isArray(data) ? data : [data];
  sequence.forEach((value) => api.review.mockResolvedValueOnce(ok(value)));
  api.review.mockResolvedValue(ok(sequence[sequence.length - 1]));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

const takenOut = (claimId: string): ClaimReviewItemDto => ({
  ...unsupportedItem(1), claimId, state: { state: 'removed', at: 5, by: 'machine' }, availableActions: [], verdictKind: null, reason: null,
  location: { unitIndex: 2, unitKind: 'block', unitTitle: '', field: 'body', quote: `Sentence of ${claimId}.`, span: null },
  presentation: { statement: `Statement of ${claimId}.`, sentence: `Sentence of ${claimId}.`, contextBefore: '', contextAfter: '', place: '' },
});

describe('Claim review screen while teaching notes wait', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
  });

  it('says no statements wait and the teaching notes do, in the queue, the focus panel and the tally', async () => {
    show(review([], { state: 'ready', teachingFindings: [note(0), note(1)], counts: counts(0) }));

    expect(await screen.findByRole('heading', { name: '2 teaching notes wait for you' })).toBeVisible();
    expect(screen.getByText('Every statement is verified, confirmed by you, removed or edited.')).toBeVisible();
    expect(screen.getByText('No statements wait for you. 2 teaching notes do.')).toBeVisible();
    expect(screen.getByTestId('review-tally')).toHaveTextContent('2 teaching notes wait for you · 12 verified');
    expect(screen.queryByText(/Everything is resolved/)).not.toBeInTheDocument();
  });

  it('opens the teaching notes from the focus panel', async () => {
    show(review([], { state: 'ready', teachingFindings: [note(0), note(1)], counts: counts(0) }));
    await userEvent.click(await screen.findByRole('button', { name: 'Open the teaching notes' }));
    const notes = await screen.findByRole('region', { name: 'Teaching review' });
    expect(within(notes).getAllByRole('article')).toHaveLength(2);
  });

  it('says one teaching note waits in the singular', async () => {
    show(review([], { state: 'ready', teachingFindings: [note(0)], counts: counts(0) }));
    expect(await screen.findByRole('heading', { name: '1 teaching note waits for you' })).toBeVisible();
    expect(screen.getByText('No statements wait for you. 1 teaching note does.')).toBeVisible();
  });

  it('says everything is resolved when no teaching note waits', async () => {
    show(review([], { state: 'ready', teachingFindings: [note(0, { state: 'kept' })], counts: counts(0) }));
    expect(await screen.findByRole('heading', { name: 'Everything is resolved' })).toBeVisible();
    expect(screen.getByText('Everything is resolved.')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Open the teaching notes' })).not.toBeInTheDocument();
    expect(screen.getByTestId('review-tally')).not.toHaveTextContent('teaching note');
  });

  it('lists the statements taken out when nothing else waits', async () => {
    show(review([takenOut('a'), takenOut('b')], { state: 'ready', counts: counts(0) }));
    expect(await screen.findByRole('heading', { name: 'Everything is resolved' })).toBeVisible();
    expect(screen.getByText('2 statements were taken out of the lesson. They are listed under Taken out of the lesson.')).toBeVisible();
  });

  it('says one statement was taken out in the singular, and nothing when none was', async () => {
    show(review([takenOut('a')], { state: 'ready', counts: counts(0) }));
    expect(await screen.findByText('1 statement was taken out of the lesson. It is listed under Taken out of the lesson.')).toBeVisible();
  });

  it('adds nothing about statements taken out when none were', async () => {
    show(review([], { state: 'ready', counts: counts(0) }));
    expect(await screen.findByRole('heading', { name: 'Everything is resolved' })).toBeVisible();
    expect(screen.queryByText(/taken out of the lesson/)).not.toBeInTheDocument();
  });

  it('announces that no statements are left and the teaching notes wait, after the last statement', async () => {
    const claim = waitingOnSource(1);
    const waiting = review([claim], { teachingFindings: [note(0), note(1)], counts: counts(1) });
    const done = review([{ ...claim, state: { state: 'confirmed', at: 2 } }], { revision: 8, teachingFindings: [note(0), note(1)], counts: counts(0) });
    show([waiting, done]);
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(await screen.findByText('Accepted. No statements are left. 2 teaching notes wait for you.')).toBeVisible();
  });
});
