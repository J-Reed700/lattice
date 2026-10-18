import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { claimItem, review, STOPPED_AT, unsupportedItem, userActionAt } from '@/features/learning/lessons/review/claimReviewTestData';
import { LessonClaimReviewPanel } from '@/features/learning/lessons/review/LessonClaimReviewPanel';
import type { LessonClaimReviewDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ review: vi.fn(), resolve: vi.fn(), resume: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, resumeLearningLessonReview: api.resume,
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const NOTHING_CHANGED = 'Nothing changed since the last pass. Resolve a claim or add evidence first.';

/** Each review read answers the next fixture; the last one repeats. */
function show(data: LessonClaimReviewDto | LessonClaimReviewDto[], props: Partial<Parameters<typeof LessonClaimReviewPanel>[0]> = {}) {
  const sequence = Array.isArray(data) ? data : [data];
  sequence.forEach((value) => api.review.mockResolvedValueOnce(ok(value)));
  api.review.mockResolvedValue(ok(sequence[sequence.length - 1]));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>
    <LessonClaimReviewPanel programId="program-1" lessonId="lesson-1" courseRevision={44} stoppedAt={STOPPED_AT} {...props} />
  </QueryClientProvider>);
}

/** 43 unsupported claims over 9 sections. */
const many = () => Array.from({ length: 43 }, (_, index) => unsupportedItem(index, index % 9));

describe('Lesson claim review panel', () => {
  beforeEach(() => vi.resetAllMocks());

  it('shows one claim inline, without filters, with what was compared and tried', async () => {
    show(review([claimItem()]));
    expect(await screen.findByRole('heading', { name: /Needs your review · 1 claim needs your review/ })).toBeVisible();
    expect(screen.queryByRole('group', { name: 'Show claims' })).not.toBeInTheDocument();
    expect(screen.getByText(/A passage says otherwise/)).toBeVisible();
    expect(screen.getByText(/The passage gives 99.97 °C at one atmosphere/)).toBeVisible();
    expect(screen.getByText('At one atmosphere, pure water boils at 99.97 °C.')).toBeVisible();
    expect(screen.getByText('Added by you')).toBeVisible();
    expect(screen.getByText(/Searched: “boiling point of water at sea level”/)).toHaveTextContent('No page added.');
    expect(screen.getByText(/Repair rejected: The rewrite still contradicts the passage/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Open in reader' })).toBeVisible();
    expect(screen.getByText(/not published yet/)).toBeVisible();
  });

  it('groups 43 claims by section with filters and counts, collapsed, and moves between them by keyboard', async () => {
    show(review(many()));
    expect(await screen.findByRole('heading', { name: /Needs your review · 43 claims/ })).toBeVisible();
    const filters = screen.getByRole('group', { name: 'Show claims' });
    expect(within(filters).getByRole('button', { name: 'Needs you (43)' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getAllByRole('region').filter((region) => /^Section \d/.test(region.getAttribute('aria-label') ?? ''))).toHaveLength(9);
    const headers = screen.getAllByRole('button', { expanded: false }).filter((button) => button.hasAttribute('data-claim-header'));
    expect(headers).toHaveLength(43);
    expect(headers.filter((header) => header.tabIndex === 0)).toHaveLength(1);

    headers[0].focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(headers[1]).toHaveFocus();
    await userEvent.keyboard('{End}');
    expect(headers[42]).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    expect(headers[42]).toHaveAttribute('aria-expanded', 'true');

    expect(within(filters).queryByRole('button', { name: /Confirmed by you/ })).not.toBeInTheDocument();
    await userEvent.click(within(filters).getByRole('button', { name: 'All (43)' }));
    expect(within(filters).getByRole('button', { name: 'All (43)' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('requires a note to confirm a contradicted claim and sends it with the review revision', async () => {
    const confirmed = claimItem({ state: { state: 'confirmed', at: STOPPED_AT + 5 }, human: { at: STOPPED_AT + 5, note: 'Rounded on purpose.', boundSources: [] }, availableActions: ['unconfirm', 'undo', 'attach_evidence'], history: [userActionAt(STOPPED_AT + 5)] });
    show([review([claimItem()]), review([confirmed], { revision: 8 })]);
    api.resolve.mockResolvedValue(ok({ revision: 8, attach: null }));
    await userEvent.click(await screen.findByRole('button', { name: 'Confirm anyway (note required)' }));
    const submit = screen.getByRole('button', { name: 'Confirm with this note' });
    expect(submit).toBeDisabled();
    await userEvent.type(screen.getByLabelText('Why the claim is right (required)'), 'Rounded on purpose.');
    await userEvent.click(screen.getByRole('checkbox', { name: 'Thermodynamics, ch. 4' }));
    await userEvent.click(submit);
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1', lessonId: 'lesson-1', claimId: 'claim-1', expectedRevision: 7,
      action: { action: 'confirm', note: 'Rounded on purpose.', sourceVersionIds: ['v1'] },
      operationId: expect.any(String),
    })));
    expect(await screen.findByText('Claim confirmed. 0 remain.')).toBeVisible();
    expect(screen.getByText(/Confirmed by you · .* · “Rounded on purpose.”/)).toBeVisible();

    await userEvent.click(screen.getAllByRole('button', { name: 'Undo' })[0]);
    await waitFor(() => expect(api.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: { action: 'undo' }, expectedRevision: 8 })));
  });

  it('keeps Resume disabled, saying why, until something changed, then resumes the job', async () => {
    const { unmount } = show(review([claimItem()]));
    const resume = await screen.findByRole('button', { name: 'Resume verification' });
    expect(resume).toBeDisabled();
    expect(resume).toHaveAccessibleDescription(NOTHING_CHANGED);
    unmount();

    api.resume.mockResolvedValue(ok({ id: 'job-2' }));
    show(review([claimItem({ history: [userActionAt(STOPPED_AT + 1)] })]));
    const enabled = await screen.findByRole('button', { name: 'Resume verification' });
    expect(enabled).toBeEnabled();
    await userEvent.click(enabled);
    await waitFor(() => expect(api.resume).toHaveBeenCalledWith({ programId: 'program-1', lessonId: 'lesson-1', expectedRevision: 44, operationId: expect.any(String) }));
    expect(await screen.findByText(/Verification resumed/)).toBeVisible();
  });

  it('shows the refusal when Resume finds nothing changed after all', async () => {
    api.resume.mockResolvedValue({ ok: false, error: NOTHING_CHANGED, details: { code: 'INVALID_STATE', message: NOTHING_CHANGED } });
    show(review([claimItem()]), { stoppedAt: null });
    await userEvent.click(await screen.findByRole('button', { name: 'Resume verification' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(NOTHING_CHANGED);
  });

  it('names a stopped review’s cause and offers Retry after its fix', async () => {
    show(review([], { state: 'stopped', stopReason: { code: 'model_rejected', message: 'llama.cpp rejected this request twice.', fix: 'lower the context window in Settings', at: 1, request: null } }));
    expect(await screen.findByText('llama.cpp rejected this request twice.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Retry after fixing: lower the context window in Settings' })).toBeEnabled();
  });

  it('accepts every general-knowledge proposal in one action', async () => {
    const proposal = (index: number) => unsupportedItem(index, 0);
    const proposals = [proposal(1), proposal(2)].map((item) => ({ ...item, verdictKind: { kind: 'general_knowledge_proposed' as const } }));
    show(review(proposals));
    api.resolve.mockResolvedValue(ok({ revision: 8, attach: null }));
    await userEvent.click(await screen.findByRole('button', { name: 'Accept all 2 general-knowledge proposals' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({ claimId: 'claim-1', action: { action: 'accept_all_general_knowledge' } })));
  });

  it('shows a stale-revision refusal on the card and reloads the review', async () => {
    api.resolve.mockResolvedValue({ ok: false, error: 'This claim changed; reload and act again.', details: { code: 'INVALID_STATE', message: 'This claim changed; reload and act again.' } });
    show(review([claimItem()]));
    await userEvent.click(await screen.findByRole('button', { name: 'Remove' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This claim changed; reload and act again.');
    await waitFor(() => expect(api.review).toHaveBeenCalledTimes(2));
  });
});
