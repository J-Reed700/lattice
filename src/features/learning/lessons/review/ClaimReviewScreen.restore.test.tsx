import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ review: vi.fn(), resolve: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, getLearningLessonAudit: api.audit,
  getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const sections = [{ index: 0, title: 'Basics' }, { index: 2, title: 'Loops' }];

const cut = (claimId: string, sentence: string, place: 'known' | 'choose' = 'known'): ClaimReviewItemDto => claimItem({
  claimId, state: { state: 'removed', at: 5, by: 'machine' }, availableActions: ['put_back', 'attach_evidence'], verdictKind: null, reason: null, place,
  location: { unitIndex: 2, unitKind: 'block', unitTitle: '', field: 'body', quote: sentence, span: null },
  presentation: { statement: sentence, sentence, contextBefore: '', contextAfter: '', place: '' },
});
const rewrite = (): ClaimReviewItemDto => claimItem({
  claimId: 'r1', state: { state: 'consistent', at: 5 }, availableActions: ['restore_original'], verdictKind: null, reason: null,
  rewritten: { original: 'Loops always run once.', rewrite: 'Loops can run zero times.', why: 'The first was contradicted.' },
});

function show(claims: ClaimReviewItemDto[], after?: LessonClaimReviewDto) {
  const first = review(claims, { state: 'ready', sections });
  api.review.mockResolvedValueOnce(ok(first));
  api.review.mockResolvedValue(ok(after ?? first));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Loops" courseRevision={1} onBack={vi.fn()} />
  </QueryClientProvider>);
}

const open = async () => {
  await userEvent.click(await screen.findByRole('button', { name: /Taken out of the lesson/ }));
  return screen.findByRole('region', { name: 'Statements taken out of the lesson' });
};

describe('Putting back and restoring from the review', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.resolve.mockResolvedValue(ok({ revision: 8 }));
  });

  it('puts a sentence back where it was and announces it with Undo', async () => {
    show([cut('a', 'Loops repeat a block.')], review([], { state: 'ready', revision: 8, sections }));
    const list = await open();
    expect(within(list).getByLabelText('Put it back at the end of')).toHaveValue('where-it-was');
    await userEvent.click(within(list).getByRole('button', { name: 'Put it back' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(api.resolve.mock.calls[0]).toContainEqual(expect.objectContaining({ claimId: 'a', action: { action: 'put_back', text: null, section: null } }));
    expect(await screen.findByText('Put back in the lesson.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Undo' })).toBeVisible();
  });

  it('asks for a section when the place is not known, and sends it 0-based', async () => {
    show([cut('a', 'Loops repeat a block.', 'choose')]);
    const list = await open();
    const put = within(list).getByRole('button', { name: 'Put it back' });
    expect(put).toBeDisabled();
    await userEvent.selectOptions(within(list).getByLabelText('Put it back at the end of'), 'Section 3: Loops');
    await userEvent.click(put);
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(JSON.stringify(api.resolve.mock.calls[0])).toContain('"section":2');
  });

  it('puts back the sentence as the user edited it', async () => {
    show([cut('a', 'Loops repeat a block.')]);
    const list = await open();
    await userEvent.click(within(list).getByRole('button', { name: 'Edit and put back' }));
    const box = within(list).getByLabelText('The sentence to put back');
    await userEvent.clear(box);
    await userEvent.type(box, 'Loops repeat a body.');
    await userEvent.click(within(list).getByRole('button', { name: 'Put it back' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(JSON.stringify(api.resolve.mock.calls[0])).toContain('"text":"Loops repeat a body."');
  });

  it('puts back every selected sentence one after the other', async () => {
    show([cut('a', 'First one.'), cut('b', 'Second one.')]);
    const list = await open();
    await userEvent.click(within(list).getByRole('checkbox', { name: 'Select: First one.' }));
    await userEvent.click(within(list).getByRole('checkbox', { name: 'Select: Second one.' }));
    await userEvent.click(within(list).getByRole('button', { name: 'Put back' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('Put back 2 sentences in the lesson.')).toBeVisible();
  });

  it('restores the original of a rewritten sentence', async () => {
    show([rewrite()]);
    await userEvent.click(await screen.findByRole('button', { name: /Rewritten in the lesson/ }));
    expect(await screen.findByText('Loops can run zero times.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Restore the original' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(JSON.stringify(api.resolve.mock.calls[0])).toContain('"action":"restore_original"');
    expect(await screen.findByText('Original restored.')).toBeVisible();
  });

  it('opens Add a source on a taken-out sentence', async () => {
    show([cut('a', 'Loops repeat a block.')]);
    const list = await open();
    await userEvent.click(within(list).getByRole('button', { name: 'Add a source' }));
    expect(await screen.findByText('Add a source for this claim')).toBeVisible();
  });
});
