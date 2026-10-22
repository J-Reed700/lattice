import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonPreparationStatus, type ReviewOpener } from '@/features/learning/lessons/LessonPreparationProgress';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LearningGenerationJob } from '@/lib/bindings';


const api = vi.hoisted(() => ({ plan: vi.fn(), cancel: vi.fn(), retry: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, cancelLearningGenerationJob: api.cancel, retryLearningGenerationJob: api.retry,
  getLearningLessonClaimReview: api.review,
} }));

const job: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 44, status: 'running', progressCompleted: 0,
  activity: null, progressTotal: 1, progressMessage: 'Writing the first lesson · 420 response characters received',
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000,
  startedAt: Date.now() - 60_000, finishedAt: null,
};

function Harness({ onReview }: { onReview?: ReviewOpener }) {
  const plan = useLearningPlan('program-1');
  return <LessonPreparationStatus job={plan.data?.jobs[0]} pending={false} programId="program-1" revision={44} lessonTitles={{ 'lesson-1': 'Phase changes' }} onReview={onReview} />;
}

function show(onReview?: ReviewOpener) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const view = render(<QueryClientProvider client={client}><Harness onReview={onReview} /></QueryClientProvider>);
  return { ...view, client };
}

/** Everything but the status line, the bar and the button sits behind Details. */
const openDetails = () => userEvent.click(screen.getByRole('button', { name: 'Details' }));

describe('Durable lesson preparation progress', () => {
  beforeEach(() => vi.resetAllMocks());
  afterEach(() => vi.useRealTimers());

  it('shows a saved connection interruption and resumes without a manual retry', async () => {
    let current: LearningGenerationJob = { ...job, status: 'pending',
      error: 'The model or network is temporarily unavailable. Preparation retries automatically while Lattice is open.',
      progressMessage: 'Connection interrupted; saved work will resume automatically',
    };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    expect(await screen.findByRole('status')).toHaveTextContent('Preparation will resume automatically');
    expect(screen.getByText('Waiting to retry')).toBeVisible();
    await openDetails();
    expect(screen.getByRole('heading', { name: 'Preparation interrupted' })).toBeVisible();
    expect(screen.getByText(/Preparation retries automatically/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Retry lesson preparation' })).not.toBeInTheDocument();
    expect(screen.queryByText('Waiting for a preparation slot')).not.toBeInTheDocument();
    expect(screen.queryByText('Waiting for model output')).not.toBeInTheDocument();
    current = { ...job, progressMessage: 'Resumed from saved checks' };
    await view.client.invalidateQueries();
    expect(await screen.findByText('Resumed from saved checks')).toBeVisible();
    expect(screen.queryByText('Waiting to retry')).not.toBeInTheDocument();
    expect(api.retry).not.toHaveBeenCalled();
    view.unmount(); view.client.clear();
  });

  it('explains the live verification pass, saved progress, research transitions and reopening', async () => {
    let current: LearningGenerationJob = { ...job, activity: {
      phase: 'evidence', phaseStartedAt: Date.now() - 600_000, lastActivityAt: Date.now() - 120_000,
      lastCheckpointAt: Date.now() - 180_000, lessonTitle: 'Understanding observations', modelName: 'Test model',
      modelRunning: true, responseCharacters: 0, modelAttempt: 1, verificationPass: 2,
      checksCompleted: 77, checksTotal: 164, checksReused: 30, checksUnresolved: 3, checksSentBack: 0, newReferences: null, modelChecksTotal: 134, deeperEvidence: null, claims: null,
      recentSteps: [{ phase: 'coverage', startedAt: Date.now() - 700_000 }, { phase: 'evidence', startedAt: Date.now() - 600_000 }],
    } };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    // Collapsed: one status line and the bar; the details are not on the page.
    expect(await screen.findByRole('status')).toHaveTextContent('Preparing Understanding observations · Fact checks & repairs');
    expect(screen.getByRole('progressbar', { name: 'Claim checks this pass' })).toHaveAttribute('aria-valuenow', '77');
    expect(screen.queryByRole('heading', { name: 'Verifying factual claims' })).not.toBeInTheDocument();
    await openDetails();
    expect(screen.getByRole('heading', { name: 'Verifying factual claims' })).toBeVisible();
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    expect(screen.getByText('87')).toBeVisible();
    expect(screen.getByText('74')).toBeVisible();
    expect(screen.getByText(/Checkpoint saved at/)).toBeVisible();
    expect(screen.getByText(/No new output or completed step/)).toBeVisible();
    expect(screen.getByText('Waiting for model output')).toBeVisible();
    expect(within(screen.getByRole('list', { name: 'Preparation workflow' })).getByText('Fact checks & repairs')).toHaveAttribute('aria-current', 'step');
    await openDetails();
    expect(screen.getByRole('button', { name: 'Details' })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Waiting for model output')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeVisible();
    await openDetails();
    await userEvent.click(screen.getByText('Recent activity'));
    expect(screen.getByText('Checking every section is covered')).toBeVisible();
    current = { ...current, progressMessage: 'Saving additional references', activity: { ...current.activity!, phase: 'research', modelRunning: false } };
    await view.client.invalidateQueries();
    expect(await screen.findByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
    expect(screen.getByText(/Last verification pass/)).toBeVisible();
    expect(screen.queryByText('Waiting for model output')).not.toBeInTheDocument();
    view.unmount(); view.client.clear();
    const reopened = show();
    expect(await screen.findByRole('status')).toHaveTextContent('Fact checks & repairs');
    await openDetails();
    expect(screen.getByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    reopened.unmount(); reopened.client.clear();
  });

  it('rejoins an existing job, polls real progress, and refreshes the lesson on completion', async () => {
    let current = job;
    api.plan.mockImplementation(async () => ({ ok: true, data: { programId: 'program-1', jobs: [current] } }));
    const { client, unmount } = show();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    expect(await screen.findByRole('status')).toHaveTextContent('Preparing your lesson');
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    await openDetails();
    expect(screen.getByText(/420 response characters/)).toBeVisible();
    expect(screen.getByText(/quitting pauses it, and it resumes from saved checkpoints/)).toBeVisible();
    current = { ...job, status: 'completed', progressCompleted: 1 };
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Lesson preparation' })).not.toBeInTheDocument(), { timeout: 4_000 });
    expect(api.plan.mock.calls.length).toBeGreaterThan(1);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['learning-program', 'program-1'] });
    unmount(); client.clear();
  });

  it('waits for the complete reuse count and distinguishes 3 new checks from 170 lesson claims', async () => {
    let current: LearningGenerationJob = { ...job, activity: {
      phase: 'evidence', phaseStartedAt: Date.now(), lastActivityAt: Date.now(), lastCheckpointAt: Date.now(),
      lessonTitle: 'Understanding observations', modelName: 'Test model', modelRunning: false,
      responseCharacters: 0, modelAttempt: 0, verificationPass: 41, checksCompleted: 3,
      checksTotal: 170, checksReused: 3, checksUnresolved: 0, checksSentBack: 0, newReferences: null, modelChecksTotal: null, deeperEvidence: null, claims: null, recentSteps: [],
    } };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    await screen.findByRole('status');
    await openDetails();
    expect(await screen.findByText(/Looking for saved checks to reuse/)).toBeVisible();
    expect(screen.getByText('Calculating…')).toBeVisible();
    expect(screen.queryByText('167')).not.toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    current = { ...current, activity: { ...current.activity!, checksCompleted: 167, checksReused: 167, modelChecksTotal: 3, deeperEvidence: null, modelRunning: true } };
    await view.client.invalidateQueries();
    expect(await screen.findByText('167 saved checks reused. 3 of 3 model checks remaining in this pass.')).toBeVisible();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '167');
    expect(screen.queryByText('Calculating…')).not.toBeInTheDocument();
    current = { ...current, activity: { ...current.activity!, checksCompleted: 170, checksUnresolved: 1, modelRunning: false, phase: 'research' } };
    await view.client.invalidateQueries();
    expect(await screen.findByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
    expect(screen.getByText('170 / 170 checked')).toBeVisible();
    expect(screen.getByText(/Checking finished; 1 claim needed attention in the last pass/)).toBeVisible();
    expect(screen.queryByText(/Each pass fixes or hands over at least one claim/)).not.toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(screen.queryByText('Still to check')).not.toBeInTheDocument();
    view.unmount(); view.client.clear();
  });

  it('counts each claim once while unsupported claims are checked against deeper evidence pages', async () => {
    let current: LearningGenerationJob = { ...job, progressMessage: 'Checking 21 unsupported claims against their next page of evidence (page 3)', activity: {
      phase: 'evidence', phaseStartedAt: Date.now(), lastActivityAt: Date.now(), lastCheckpointAt: Date.now(),
      lessonTitle: 'Async/Await Under the Hood', modelName: 'Test model', modelRunning: true,
      responseCharacters: 0, modelAttempt: 1, verificationPass: 1, checksCompleted: 115,
      checksTotal: 115, checksReused: 115, checksUnresolved: 21, checksSentBack: 0, newReferences: null, modelChecksTotal: 0,
      deeperEvidence: { page: 3, claims: 21, checked: 8 }, claims: null, recentSteps: [],
    } };
    const count = (label: string) => screen.getByText(label).nextElementSibling?.textContent;
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    const bar = await screen.findByRole('progressbar', { name: 'Unsupported claims checked against evidence page 3' });
    expect(bar).toHaveAttribute('aria-valuenow', '8');
    expect(bar).toHaveAttribute('aria-valuemax', '21');
    await openDetails();
    expect(screen.getByText('115 / 115 checked')).toBeVisible();
    expect(screen.getByText(/Every claim has had its first check\. Checking 21 unsupported claims against evidence page 3; 13 left\./)).toBeVisible();
    expect([count('Supported'), count('Need attention'), count('Left on evidence page 3')]).toEqual(['94', '21', '13']);
    expect(screen.queryByText('Still to check')).not.toBeInTheDocument();
    current = { ...current, activity: { ...current.activity!, checksUnresolved: 20, deeperEvidence: { page: 3, claims: 21, checked: 9 } } };
    await view.client.invalidateQueries();
    await waitFor(() => expect(count('Left on evidence page 3')).toBe('12'));
    expect([count('Supported'), count('Need attention')]).toEqual(['95', '20']);
    current = { ...current, activity: { ...current.activity!, deeperEvidence: null } };
    await view.client.invalidateQueries();
    expect(await screen.findByText(/Checking finished\. Preparing the next step\./)).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Claim checks this pass' })).toHaveAttribute('aria-valuenow', '115');
    expect([count('Supported'), count('Need attention'), count('Still to check')]).toEqual(['95', '20', '0']);
    view.unmount(); view.client.clear();
  });

  it('keeps repair work and an advancing quiet-period clock visible instead of a finished check bar', () => {
    vi.useFakeTimers();
    const now = Date.now();
    const current: LearningGenerationJob = { ...job,
      progressMessage: 'Correcting affected section 4 of 9 · Waiting for the model’s response',
      activity: {
        phase: 'repair', phaseStartedAt: now - 600_000, lastActivityAt: now - 120_000,
        lastCheckpointAt: now - 120_000, lessonTitle: 'Understanding observations', modelName: 'Test model',
        modelRunning: true, responseCharacters: 0, modelAttempt: 1, verificationPass: 43,
        checksCompleted: 170, checksTotal: 170, checksReused: 55, checksUnresolved: 21, checksSentBack: 0, newReferences: null, modelChecksTotal: 115, deeperEvidence: null, claims: null, recentSteps: [],
      },
    };
    const client = new QueryClient();
    const panel = (value: LearningGenerationJob) => <QueryClientProvider client={client}><LessonPreparationStatus job={value} pending={false} programId="program-1" revision={44} /></QueryClientProvider>;
    const view = render(panel(current));
    expect(screen.getByRole('status')).toHaveTextContent('Preparing Understanding observations · Fact checks & repairs');
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText('Lesson not ready yet')).toBeVisible();
    expect(screen.getByText(/Checking finished; 21 claims needed attention in the last pass/)).toBeVisible();
    expect(screen.getByText(/Correcting affected section 4 of 9/)).toBeVisible();
    expect(screen.getByText('Last reported progress · 2m 0s ago')).toBeVisible();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(screen.queryByText('Still to check')).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Repairing the draft' })).toBeVisible();
    expect(screen.getByText(/Correcting affected section 4 of 9/)).toBeVisible();
    expect(screen.getByText('Waiting for model output')).toBeVisible();
    act(() => vi.advanceTimersByTime(65_000));
    expect(screen.getByText('Last reported progress · 3m 5s ago')).toBeVisible();
    expect(screen.getByText(/The request is still open, but the model has not reported more progress/)).toBeVisible();
    view.rerender(panel({ ...current, progressMessage: 'Correcting affected section 5 of 9', activity: { ...current.activity!, lastActivityAt: Date.now(), responseCharacters: 420 } }));
    expect(screen.getByText('Last reported progress · 0s ago')).toBeVisible();
    expect(screen.getByText('Receiving model output')).toBeVisible();
    expect(screen.getByText(/Correcting affected section 5 of 9/)).toBeVisible();
    expect(screen.queryByText(/No new output or completed step/)).not.toBeInTheDocument();
    view.unmount(); client.clear();
  });

  it.each(['running', 'pending'] as const)('pauses a %s job with details collapsed and resumes saved work only on request after reopening', async (status) => {
    const user = userEvent.setup();
    let current: LearningGenerationJob = { ...job, status,
      error: status === 'pending' ? 'The model is unavailable. Preparation will retry automatically.' : null,
      activity: {
        phase: 'evidence', phaseStartedAt: Date.now() - 60_000, lastActivityAt: Date.now(),
        lastCheckpointAt: Date.now(), lessonTitle: 'Understanding observations', modelName: 'Previous main model',
        modelRunning: true, responseCharacters: 420, modelAttempt: 1, verificationPass: 2,
        checksCompleted: 77, checksTotal: 164, checksReused: 30, checksUnresolved: 3, checksSentBack: 0, newReferences: null, modelChecksTotal: 134, deeperEvidence: null, claims: null, recentSteps: [],
      },
    };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    api.cancel.mockImplementation(async () => {
      current = { ...current, status: 'cancelled', error: null, progressMessage: 'Cancelled', finishedAt: Date.now() };
      return { ok: true, data: current };
    });
    api.retry.mockImplementation(async () => {
      current = { ...current, id: 'job-2', retryOfJobId: 'job-1', status: 'pending', progressMessage: 'Retry queued', finishedAt: null };
      return { ok: true, data: current };
    });
    const { client, unmount } = show();
    await user.click(await screen.findByRole('button', { name: 'Pause lesson preparation' }));
    expect(api.cancel).toHaveBeenCalledWith(expect.objectContaining({ programId: 'program-1', jobId: 'job-1', expectedRevision: 44 }));
    expect(await screen.findByRole('button', { name: 'Resume lesson preparation' })).toBeEnabled();
    expect(screen.getByRole('status')).toHaveTextContent('Lesson preparation paused');
    expect(screen.getByRole('status')).toHaveTextContent('at verifying factual claims');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText('Cancelled')).not.toBeInTheDocument();
    expect(screen.queryByText('Waiting to retry')).not.toBeInTheDocument();
    unmount(); client.clear();

    const reopened = show();
    const resumeButton = await screen.findByRole('button', { name: 'Resume lesson preparation' });
    await user.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText(/stays paused until you choose Resume, even after reopening/)).toBeVisible();
    expect(screen.getByText(/Lesson writing and fact-checking use the main model/)).toBeVisible();
    expect(screen.getByText(/Resume uses your current model settings/)).toBeVisible();
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    expect(screen.queryByText('Receiving model output')).not.toBeInTheDocument();
    expect(screen.queryByText(/quitting pauses it/)).not.toBeInTheDocument();
    expect(api.retry).not.toHaveBeenCalled();
    await user.click(resumeButton);
    expect(await screen.findByText('Retry queued')).toBeVisible();
    expect(api.retry).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'job-1' }));
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    reopened.unmount(); reopened.client.clear();
  });

  it('keeps failed jobs distinct from a user pause and offers Retry', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'failed', error: 'The draft needs another review.' }] } });
    const { client, unmount } = show();
    expect(await screen.findByRole('alert')).toHaveTextContent('The draft needs another review.');
    expect(screen.getByRole('status')).toHaveTextContent('Lesson preparation failed');
    expect(screen.getByRole('button', { name: 'Retry lesson preparation' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Resume lesson preparation' })).not.toBeInTheDocument();
    unmount(); client.clear();
  });

  it('shows a pause failure without pretending the job stopped', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [job] } });
    api.cancel.mockResolvedValue({ ok: false, error: 'The database is unavailable.' });
    const { client, unmount } = show();
    await userEvent.click(await screen.findByRole('button', { name: 'Pause lesson preparation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The database is unavailable.');
    expect(screen.getByRole('status')).toHaveTextContent('Preparing your lesson');
    unmount(); client.clear();
  });

  it('keeps a lesson paused when resuming fails and shows the error with details collapsed', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'cancelled', progressMessage: 'Cancelled' }] } });
    api.retry.mockResolvedValue({ ok: false, error: 'The database is unavailable.' });
    const { client, unmount } = show();
    await userEvent.click(await screen.findByRole('button', { name: 'Resume lesson preparation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The database is unavailable.');
    expect(screen.getByRole('status')).toHaveTextContent('Lesson preparation paused');
    expect(screen.getByRole('button', { name: 'Resume lesson preparation' })).toBeEnabled();
    unmount(); client.clear();
  });

  it('shows a held lesson as one line with Review, never as a failure', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'completed', resultId: 'needs_review:lesson-1', progressMessage: 'Needs your review: 1 claim', finishedAt: Date.now() - 1_000 }] } });
    api.review.mockResolvedValue({ ok: true, data: review([claimItem()]) });
    const onReview = vi.fn();
    const { client, unmount } = show(onReview);
    const region = await screen.findByRole('region', { name: 'Lesson preparation' });
    expect(await within(region).findByText('— 1 claim needs you')).toBeVisible();
    expect(within(region).getByText('Phase changes')).toBeVisible();
    expect(api.review).toHaveBeenCalledWith('program-1', 'lesson-1');
    // The claims themselves live on the review screen, never here.
    expect(screen.queryByText(claimItem().location.quote)).not.toBeInTheDocument();
    expect(screen.queryByText(/failed/i)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Retry lesson preparation' })).not.toBeInTheDocument();
    await userEvent.click(within(region).getByRole('button', { name: 'Review' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'lesson-1');
    unmount(); client.clear();
  });

  it('counts claims per state while preparation runs, in the review\'s words', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, activity: {
      phase: 'repair', phaseStartedAt: Date.now(), lastActivityAt: Date.now(), lastCheckpointAt: Date.now(),
      lessonTitle: 'Understanding observations', modelName: 'Test model', modelRunning: true,
      responseCharacters: 0, modelAttempt: 1, verificationPass: 3, checksCompleted: 115,
      checksTotal: 115, checksReused: 0, checksUnresolved: 7, checksSentBack: 0, newReferences: null, modelChecksTotal: 115, deeperEvidence: null,
      claims: { unchecked: 0, searching: 0, sentBack: 0, repairing: 2, waiting: 5, verified: 108, resolvedByYou: 0 }, recentSteps: [],
    } }] } });
    const { client, unmount } = show();
    await screen.findByRole('status');
    await openDetails();
    expect(screen.getByText('5 claims need you · 108 verified · 2 being checked')).toBeVisible();
    expect(screen.queryByText(/gives up on a claim/)).not.toBeInTheDocument();
    unmount(); client.clear();
  });

  it('names claims the source check sent back apart from those needing attention, and a re-check against new references', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, activity: {
      phase: 'evidence', phaseStartedAt: Date.now(), lastActivityAt: Date.now(), lastCheckpointAt: Date.now(),
      lessonTitle: 'Tool use', modelName: 'Test model', modelRunning: true,
      responseCharacters: 0, modelAttempt: 1, verificationPass: 14, checksCompleted: 8,
      checksTotal: 89, checksReused: 8, checksUnresolved: 0, checksSentBack: 0, newReferences: 72, modelChecksTotal: 81,
      deeperEvidence: null, claims: { unchecked: 0, searching: 9, sentBack: 72, repairing: 0, waiting: 3, verified: 5, resolvedByYou: 0 }, recentSteps: [],
    } }] } });
    const { client, unmount } = show();
    await screen.findByRole('status');
    await openDetails();
    expect(screen.getByText('Re-checking 81 claims against 72 new references · Pass 14')).toBeVisible();
    expect(screen.getByText('Still to re-check')).toBeVisible();
    expect(screen.getByText('3 claims need you · 5 verified · 81 being checked')).toBeVisible();
    expect(screen.queryByText(/being repaired/)).not.toBeInTheDocument();
    unmount(); client.clear();
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, activity: {
      phase: 'research', phaseStartedAt: Date.now(), lastActivityAt: Date.now(), lastCheckpointAt: Date.now(),
      lessonTitle: 'Tool use', modelName: 'Test model', modelRunning: false,
      responseCharacters: 0, modelAttempt: 1, verificationPass: 14, checksCompleted: 89,
      checksTotal: 89, checksReused: 8, checksUnresolved: 9, checksSentBack: 72, newReferences: null, modelChecksTotal: 81,
      deeperEvidence: null, claims: null, recentSteps: [],
    } }] } });
    const view = show();
    const count = (label: string) => screen.getByText(label).nextElementSibling?.textContent;
    await screen.findByRole('status');
    await openDetails();
    await screen.findByText('Sent back for stronger sources');
    expect([count('Supported'), count('Needed attention in this pass'), count('Sent back for stronger sources')]).toEqual(['8', '9', '72']);
    expect(screen.getByText(/9 claims needed attention and 72 were sent back for stronger sources in the last pass/)).toBeVisible();
    view.unmount(); view.client.clear();
  });

  it('shows a lesson held only by an audit stop as waiting, never "0 claims", with Review', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'completed', resultId: 'needs_review:lesson-1', progressMessage: 'Needs your review: 0 claims', finishedAt: Date.now() - 1_000 }] } });
    api.review.mockResolvedValue({ ok: true, data: review([], { stopReason: { code: 'verifier_unset', message: 'Audit cannot run: no Verifier model is set.', fix: 'Choose one in Settings → Models under Verifier, or turn the audit off for this course.', at: 1, request: null } }) });
    const { client, unmount } = show(vi.fn());
    expect(await screen.findByText('— waiting for the independent check')).toBeVisible();
    expect(screen.queryByText(/0 claims/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Review' })).toBeEnabled();
    unmount(); client.clear();
  });
});
