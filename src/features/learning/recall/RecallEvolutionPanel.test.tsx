import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { RecallEvolutionPanel } from '@/features/learning/recall/RecallEvolutionPanel';

const mocks = vi.hoisted(() => ({ recall: vi.fn(), sources: vi.fn(), save: vi.fn(), duplicate: vi.fn(), scheduler: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningRecallWorkspace: mocks.recall,
  getLearningSourceWorkspace: mocks.sources,
  saveLearningRecallCard: mocks.save,
  decideLearningRecallDuplicate: mocks.duplicate,
  changeLearningRecallScheduler: mocks.scheduler,
  reviewLearningRecallCard: mocks.review,
} }));
const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const content = { prompt: 'Which boundary makes retries safe?', answer: 'A stable operation identifier and a payload fingerprint.', explanation: 'The same operation can be replayed without creating a second write.', options: [], correctOptionIndex: null, language: null, clozeDeletions: [] };
const card = (overrides: Record<string, unknown> = {}) => ({ id: 'card-1', format: 'question_answer', content, sourceVersionIds: ['version-1'], contentRevision: 2, scheduler: { schedulerVersion: 'expanding_v1', stability: null, difficulty: null, lastReviewedAt: null, dueAt: 1, intervalDays: 2, reviewCount: 1 }, versions: [{ revision: 1, format: 'question_answer', content, sourceVersionIds: ['version-1'], changeReason: 'Created', createdAt: 1_790_000_000_000 }, { revision: 2, format: 'question_answer', content: { ...content, prompt: 'Which boundary makes retries safe?' }, sourceVersionIds: ['version-1'], changeReason: 'Tightened prompt', createdAt: 1_790_100_000_000 }], createdAt: 1_790_000_000_000, updatedAt: 1_790_100_000_000, ...overrides });
const workspace = (overrides: Record<string, unknown> = {}) => ({ programId: 'program-1', cards: [card()], duplicates: [], dueCount: 1, fsrsAvailable: true, schedulerDisclosure: 'Schedules are versioned and can be replayed.', ...overrides });
const sourceWorkspace = () => ({ programId: 'program-1', sources: [{ id: 'source-1', deletedAt: null, versions: [{ id: 'version-1', versionNumber: 1, title: 'Operations guide' }] }] });
function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<RecallEvolutionPanel programId="program-1" active />, { wrapper });
}

describe('Learning Studio versioned recall', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.recall.mockResolvedValue(ok(workspace()));
    mocks.sources.mockResolvedValue(ok(sourceWorkspace()));
    mocks.save.mockResolvedValue(ok(workspace()));
    mocks.duplicate.mockResolvedValue(ok(workspace({ duplicates: [{ id: 'duplicate-1', cardId: 'card-1', possibleDuplicateCardId: 'card-2', reason: 'Prompts overlap.', similarity: 0.91, status: 'confirmed', createdAt: 1, decidedAt: 2 }] })));
    mocks.scheduler.mockResolvedValue(ok(workspace({ cards: [card({ scheduler: { schedulerVersion: 'fsrs_6_v1', stability: 2.1, difficulty: 5, lastReviewedAt: 1, dueAt: 2, intervalDays: 2, reviewCount: 1 } })] })));
    mocks.review.mockResolvedValue(ok(workspace({ cards: [card({ scheduler: { schedulerVersion: 'expanding_v1', stability: null, difficulty: null, lastReviewedAt: 1_790_100_000_000, dueAt: Date.now() + 86_400_000, intervalDays: 3, reviewCount: 2 } })], dueCount: 0 })));
  });

  it('creates a source-linked cloze card and retries the identical version operation', async () => {
    const user = userEvent.setup();
    mocks.save.mockResolvedValueOnce(fail('Connection interrupted'));
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Create a typed card' }));
    await user.selectOptions(screen.getByLabelText('Card format'), 'cloze');
    await user.type(screen.getByRole('textbox', { name: 'Card prompt' }), 'A {{stable operation ID}} prevents duplicates.');
    await user.type(screen.getByRole('textbox', { name: 'Card answer' }), 'stable operation ID');
    await user.type(screen.getByRole('textbox', { name: 'Card explanation' }), 'A replay reuses its original request.');
    await user.type(screen.getByRole('textbox', { name: 'Cloze deletions' }), 'stable operation ID');
    await user.click(screen.getByLabelText(/Operations guide · v1/));
    await user.click(screen.getByRole('button', { name: 'Save card version' }));
    expect((await screen.findAllByRole('alert')).some((alert) => alert.textContent?.includes('Connection interrupted'))).toBe(true);
    const request = mocks.save.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'program-1', format: 'cloze', sourceVersionIds: ['version-1'], content: { prompt: expect.stringContaining('stable operation ID'), clozeDeletions: ['stable operation ID'] } });
    await user.click(screen.getByRole('button', { name: 'Retry card save' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][0]).toEqual(request);
  });

  it('hides a due answer until reveal and retries the same review ID after failure', async () => {
    const user = userEvent.setup();
    mocks.review.mockResolvedValueOnce(fail('Temporary review error'));
    renderPanel();
    expect((await screen.findAllByText('Which boundary makes retries safe?')).length).toBeGreaterThan(0);
    expect(screen.queryByText(/A stable operation identifier and a payload fingerprint/)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Reveal answer' }));
    expect(screen.getByText(/A stable operation identifier and a payload fingerprint/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'good' }));
    expect(await screen.findByText(/Temporary review error/)).toBeVisible();
    const request = mocks.review.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'program-1', cardId: 'card-1', expectedReviewCount: 1, rating: 'good', reviewId: expect.any(String) });
    await user.click(screen.getByRole('button', { name: 'Retry review' }));
    await waitFor(() => expect(mocks.review).toHaveBeenCalledTimes(2));
    expect(mocks.review.mock.calls[1][0]).toEqual(request);
    expect(await screen.findByRole('status')).toHaveTextContent(/Review saved/);
  });

  it('renders migrated multiple-choice cards and records the selected option with the review', async () => {
    const user = userEvent.setup();
    const multipleChoiceContent = {
      ...content,
      answer: 'A stable operation identifier.',
      options: ['A random request identifier.', 'A stable operation identifier.'],
      correctOptionIndex: 1,
    };
    mocks.recall.mockResolvedValue(ok(workspace({ cards: [card({ format: 'multiple_choice', content: multipleChoiceContent, versions: [{ revision: 1, format: 'multiple_choice', content: multipleChoiceContent, sourceVersionIds: ['version-1'], changeReason: 'Migrated from Study', createdAt: 1_790_000_000_000 }] })] })));
    renderPanel();

    await user.click(await screen.findByRole('radio', { name: 'A stable operation identifier.' }));
    await user.click(screen.getByRole('button', { name: 'Reveal answer' }));
    expect(screen.getByText('Your choice matches the saved answer.')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'good' }));

    await waitFor(() => expect(mocks.review).toHaveBeenCalledOnce());
    expect(mocks.review.mock.calls[0][0]).toMatchObject({ cardId: 'card-1', selectedOption: 1, rating: 'good' });
  });

  it('offers FSRS only when available and retries scheduler or duplicate decisions with their original requests', async () => {
    const user = userEvent.setup();
    mocks.scheduler.mockResolvedValueOnce(fail('Revision changed; reload and retry.'));
    mocks.scheduler.mockResolvedValueOnce(ok(workspace({ cards: [card({ scheduler: { schedulerVersion: 'fsrs_6_v1', stability: 2.1, difficulty: 5, lastReviewedAt: 1, dueAt: 2, intervalDays: 2, reviewCount: 1 } })], duplicates: [{ id: 'duplicate-1', cardId: 'card-1', possibleDuplicateCardId: 'card-2', reason: 'Prompts overlap.', similarity: 0.91, status: 'pending', createdAt: 1, decidedAt: null }] })));
    mocks.duplicate.mockResolvedValueOnce(fail('Duplicate decision failed'));
    mocks.recall.mockResolvedValue(ok(workspace({ duplicates: [{ id: 'duplicate-1', cardId: 'card-1', possibleDuplicateCardId: 'card-2', reason: 'Prompts overlap.', similarity: 0.91, status: 'pending', createdAt: 1, decidedAt: null }] })));
    renderPanel();
    await screen.findByText('A reversible scheduling choice');
    const fsrs = screen.getAllByRole('button', { name: 'FSRS 6 v1' })[0];
    expect(fsrs).toBeEnabled();
    await user.click(fsrs);
    expect(await screen.findByText(/Revision changed/)).toBeVisible();
    const schedulerRequest = mocks.scheduler.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry scheduler change' }));
    await waitFor(() => expect(mocks.scheduler).toHaveBeenCalledTimes(2));
    expect(mocks.scheduler.mock.calls[1][0]).toEqual(schedulerRequest);
    await user.click(await screen.findByRole('button', { name: 'Mark as duplicate' }));
    expect(await screen.findByText('Duplicate decision failed')).toBeVisible();
    const duplicateRequest = mocks.duplicate.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry duplicate decision' }));
    await waitFor(() => expect(mocks.duplicate).toHaveBeenCalledTimes(2));
    expect(mocks.duplicate.mock.calls[1][0]).toEqual(duplicateRequest);
  });

  it('keeps FSRS unavailable visibly disabled', async () => {
    mocks.recall.mockResolvedValue(ok(workspace({ fsrsAvailable: false })));
    renderPanel();
    await screen.findByText('A reversible scheduling choice');
    expect(screen.getAllByRole('button', { name: 'FSRS 6 v1' })[0]).toBeDisabled();
    expect(screen.getByText(/FSRS is currently unavailable/)).toBeVisible();
  });
});
