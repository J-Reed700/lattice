import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimAttachProgressDto, ClaimAttachReportDto, LessonClaimReviewDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  review: vi.fn(), resolve: vi.fn(), attachLesson: vi.fn(), audit: vi.fn(), plan: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, resolveLearningClaim: api.resolve, attachLearningLessonSource: api.attachLesson,
  getLearningLessonAudit: api.audit, getLearningPlan: api.plan,
  getLearningSourceTrust: async () => ({ ok: true, data: { sources: [], claim: null } }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const URL_ = 'https://monarchwatch.example.org/tags';
const BUTTON = 'Add a source to the lesson';
const source = { sourceId: 'source-watch', sourceVersionId: 'version-watch', title: 'Monarch Watch', url: URL_, addedBy: 'user' as const };
const report = (overrides: Partial<ClaimAttachReportDto> = {}): ClaimAttachReportDto => ({
  revision: 8, source, alreadySaved: false, outcome: null, failure: null, snapshot: { state: 'not_applicable' },
  others: { tried: 2, verified: 1, notVerified: 0, noPassage: 1, stopped: null }, lines: [], ...overrides,
});

function show(data: LessonClaimReviewDto | null) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

/** The attach answers when the test says, with progress events in between. */
function pendingAttach() {
  let progress: ((update: ClaimAttachProgressDto) => void) | null = null;
  let finish: ((value: unknown) => void) | null = null;
  api.attachLesson.mockImplementation((_request: unknown, onProgress: (update: ClaimAttachProgressDto) => void) => {
    progress = onProgress;
    return new Promise((resolve) => { finish = resolve; });
  });
  return {
    progress: async (update: ClaimAttachProgressDto) => { await act(async () => { progress!(update); }); },
    finish: (value: ClaimAttachReportDto) => act(async () => finish!(ok(value))),
  };
}

const twoWaiting = () => review([waitingOnSource(1), waitingOnSource(2)]);

describe('Claim review screen, adding a source to the whole lesson', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
  });

  it('opens the sheet with the lesson title and the line that says what it checks', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    expect(screen.getByRole('heading', { name: BUTTON })).toBeVisible();
    expect(screen.getByText('Lattice checks every open statement in this lesson against it.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'A link' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Paste text instead' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Choose a file…' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Pick a saved source…' })).toBeVisible();
  });

  it.each([
    ['waiting statements', () => twoWaiting()],
    ['every statement resolved', () => review([])],
    ['a published lesson', () => review([], { state: 'ready' })],
    ['a stopped review', () => review([], { state: 'stopped', stopReason: { code: 'model_rejected', message: 'The model server rejected the request twice.', fix: 'lower the context window in Settings', at: 1, request: null } })],
  ])('is there for a review with %s', async (_name, data) => {
    show(data());
    expect(await screen.findByRole('button', { name: BUTTON })).toBeVisible();
  });

  it('is not there for a lesson with no review', async () => {
    show(null);
    expect(await screen.findByText('Not checked yet')).toBeVisible();
    expect(screen.queryByRole('button', { name: BUTTON })).not.toBeInTheDocument();
  });

  it('opens the sheet on a stopped review too', async () => {
    const user = userEvent.setup();
    show(review([], { state: 'stopped', stopReason: { code: 'model_rejected', message: 'The model server rejected the request twice.', fix: 'lower the context window in Settings', at: 1, request: null } }));
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    expect(screen.getByRole('heading', { name: BUTTON })).toBeVisible();
  });

  it('adds a source for the lesson with its evidence, shows progress, then the summary', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    const run = pendingAttach();
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(api.attachLesson).toHaveBeenCalledTimes(1);
    expect(api.attachLesson).toHaveBeenCalledWith(
      { programId: 'program-1', lessonId: 'lesson-1', expectedRevision: 7, evidence: { kind: 'attach_link', url: URL_ } },
      expect.any(Function),
    );

    await run.progress({ stage: 'fetching', host: 'monarchwatch.example.org' });
    expect(screen.getByText('Reading monarchwatch.example.org…')).toBeVisible();
    await run.progress({ stage: 'saving' });
    expect(screen.getByText('Saving it to your sources')).toBeVisible();
    await run.progress({ stage: 'checking', done: 0, total: 2, statement: 'Monarch fact number 1 is stated in the guide.' });
    expect(screen.getByText('Checking claim 1 of 2: Monarch fact number 1 is stated in the guide.')).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Claims checked' })).toHaveAttribute('aria-valuenow', '0');
    await run.progress({ stage: 'checking', done: 1, total: 2, statement: 'Monarch fact number 2 is stated in the guide.' });
    expect(screen.getByRole('progressbar', { name: 'Claims checked' })).toHaveAttribute('aria-valuenow', '1');

    await run.finish(report());
    expect(await screen.findByText('Added Monarch Watch. Verified 1 of 2 claims; 1 is not mentioned by it.')).toBeVisible();
    expect(screen.queryByRole('heading', { name: BUTTON })).not.toBeInTheDocument();
  });

  it('says so when the lesson has no open statements to check', async () => {
    const user = userEvent.setup();
    show(review([]));
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    api.attachLesson.mockResolvedValue(ok(report({ others: { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null } })));
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(await screen.findByText('Added Monarch Watch. No open claims in this lesson to check against it.')).toBeVisible();
  });

  it('reloads the review after the attach', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    api.attachLesson.mockResolvedValue(ok(report()));
    const reads = api.review.mock.calls.length;
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    await waitFor(() => expect(api.review.mock.calls.length).toBeGreaterThan(reads));
  });

  it('shows the failure and keeps the sheet open when the source could not be saved', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    api.attachLesson.mockResolvedValue(ok(report({
      source: null, others: { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null },
      failure: { kind: 'unreachable', message: 'Timed out.', nextSteps: [{ step: 'retry' }] },
    })));
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText('monarchwatch.example.org could not be reached.')).toBeVisible();
    expect(screen.getByRole('heading', { name: BUTTON })).toBeVisible();
  });

  it('shows a refused attach as a plain sentence and keeps the sheet open', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    await user.click(await screen.findByRole('button', { name: BUTTON }));
    const sentence = 'This lesson changed while you were adding the source. Open it again and retry.';
    const message = sentence;
    api.attachLesson.mockResolvedValue({ ok: false, error: message, details: { code: 'INVALID_STATE', message, details: null } });
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(sentence);
    expect(screen.getByRole('heading', { name: BUTTON })).toBeVisible();
  });

  it('is disabled while another action runs', async () => {
    const user = userEvent.setup();
    show(twoWaiting());
    const button = await screen.findByRole('button', { name: BUTTON });
    expect(button).toBeEnabled();
    let finish: ((value: unknown) => void) | null = null;
    api.resolve.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    await user.click(await screen.findByRole('button', { name: 'Remove it' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: BUTTON })).toBeDisabled();
    await act(async () => finish!(ok({ revision: 8 })));
    await waitFor(() => expect(screen.getByRole('button', { name: BUTTON })).toBeEnabled());
  });
});
