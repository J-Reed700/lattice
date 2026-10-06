import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonPreparationProgress } from '@/features/learning/lessons/LessonPreparationProgress';
import type { LearningGenerationJob } from '@/lib/bindings';


const api = vi.hoisted(() => ({ plan: vi.fn(), cancel: vi.fn(), retry: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, cancelLearningGenerationJob: api.cancel, retryLearningGenerationJob: api.retry,
} }));

const job: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 44, status: 'running', progressCompleted: 0,
  activity: null, progressTotal: 1, progressMessage: 'Writing the first lesson · 420 response characters received',
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000,
  startedAt: Date.now() - 60_000, finishedAt: null,
};

function Harness() {
  const plan = useLearningPlan('program-1');
  return <LessonPreparationProgress job={plan.data?.jobs[0]} pending={false} programId="program-1" revision={44} />;
}

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const view = render(<QueryClientProvider client={client}><Harness /></QueryClientProvider>);
  return { ...view, client };
}

describe('Durable lesson preparation progress', () => {
  beforeEach(() => vi.resetAllMocks());

  it('shows a saved connection interruption and resumes without a manual retry', async () => {
    let current: LearningGenerationJob = { ...job, status: 'pending',
      error: 'The model or network is temporarily unavailable. Preparation retries automatically while Lattice is open.',
      progressMessage: 'Connection interrupted; saved work will resume automatically',
    };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    expect(await screen.findByRole('heading', { name: 'Preparation interrupted' })).toBeVisible();
    expect(screen.getByText(/Preparation retries automatically/)).toBeVisible();
    expect(screen.getByText('Waiting to retry')).toBeVisible();
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
      checksCompleted: 77, checksTotal: 164, checksReused: 30, checksUnresolved: 3,
      recentSteps: [{ phase: 'coverage', startedAt: Date.now() - 700_000 }, { phase: 'evidence', startedAt: Date.now() - 600_000 }],
    } };
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    const view = show();
    expect(await screen.findByRole('heading', { name: 'Verifying factual claims' })).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Claim checks this pass' })).toHaveAttribute('aria-valuenow', '77');
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    expect(screen.getByText('87')).toBeVisible();
    expect(screen.getByText('74')).toBeVisible();
    expect(screen.getByText(/Checkpoint saved at/)).toBeVisible();
    expect(screen.getByText(/No new output or completed step/)).toBeVisible();
    expect(screen.getByText('Waiting for model output')).toBeVisible();
    expect(within(screen.getByRole('list', { name: 'Preparation workflow' })).getByText('Fact checks & repairs')).toHaveAttribute('aria-current', 'step');
    await userEvent.click(screen.getByRole('button', { name: 'Hide details' }));
    expect(screen.queryByRole('heading', { name: 'Verifying factual claims' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Show details' })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Show details' }));
    await userEvent.click(screen.getByText('Recent activity'));
    expect(screen.getByText('Checking every section is covered')).toBeVisible();
    current = { ...current, progressMessage: 'Saving additional references', activity: { ...current.activity!, phase: 'research', modelRunning: false } };
    await view.client.invalidateQueries();
    expect(await screen.findByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
    expect(screen.getByText(/Last verification pass/)).toBeVisible();
    expect(screen.queryByText('Waiting for model output')).not.toBeInTheDocument();
    view.unmount(); view.client.clear();
    const reopened = show();
    expect(await screen.findByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    reopened.unmount(); reopened.client.clear();
  });

  it('rejoins an existing job, polls real progress, and refreshes the lesson on completion', async () => {
    let current = job;
    api.plan.mockImplementation(async () => ({ ok: true, data: { programId: 'program-1', jobs: [current] } }));
    const { client, unmount } = show();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    expect(await screen.findByText(/420 response characters/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    expect(screen.getByText(/Quitting pauses preparation; it resumes from saved checkpoints/)).toBeVisible();
    current = { ...job, status: 'completed', progressCompleted: 1 };
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Lesson preparation progress' })).not.toBeInTheDocument(), { timeout: 4_000 });
    expect(api.plan.mock.calls.length).toBeGreaterThan(1);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['learning-program', 'program-1'] });
    unmount(); client.clear();
  });

  it.each(['running', 'pending'] as const)('pauses a %s job with details collapsed and resumes saved work only on request after reopening', async (status) => {
    const user = userEvent.setup();
    let current: LearningGenerationJob = { ...job, status,
      error: status === 'pending' ? 'The model is unavailable. Preparation will retry automatically.' : null,
      activity: {
        phase: 'evidence', phaseStartedAt: Date.now() - 60_000, lastActivityAt: Date.now(),
        lastCheckpointAt: Date.now(), lessonTitle: 'Understanding observations', modelName: 'Previous main model',
        modelRunning: true, responseCharacters: 420, modelAttempt: 1, verificationPass: 2,
        checksCompleted: 77, checksTotal: 164, checksReused: 30, checksUnresolved: 3, recentSteps: [],
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
    await user.click(await screen.findByRole('button', { name: 'Hide details' }));
    await user.click(screen.getByRole('button', { name: 'Pause lesson preparation' }));
    expect(api.cancel).toHaveBeenCalledWith(expect.objectContaining({ programId: 'program-1', jobId: 'job-1', expectedRevision: 44 }));
    expect(await screen.findByRole('button', { name: 'Resume lesson preparation' })).toBeEnabled();
    expect(screen.getByRole('status')).toHaveTextContent('Lesson preparation paused');
    expect(screen.getByText(/Paused at: Verifying factual claims. Saved work is available/)).toBeVisible();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText('Cancelled')).not.toBeInTheDocument();
    expect(screen.queryByText('Waiting to retry')).not.toBeInTheDocument();
    unmount(); client.clear();

    const reopened = show();
    const resumeButton = await screen.findByRole('button', { name: 'Resume lesson preparation' });
    expect(screen.getByText(/stays paused until you choose Resume, even after reopening/)).toBeVisible();
    expect(screen.getByText(/Lesson writing and fact-checking use the main model/)).toBeVisible();
    expect(screen.getByText(/Resume uses your current model settings/)).toBeVisible();
    expect(screen.getByText('77 / 164 checked')).toBeVisible();
    expect(screen.queryByText('Receiving model output')).not.toBeInTheDocument();
    expect(screen.queryByText(/Quitting pauses preparation/)).not.toBeInTheDocument();
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
    await userEvent.click(await screen.findByRole('button', { name: 'Hide details' }));
    await userEvent.click(screen.getByRole('button', { name: 'Pause lesson preparation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The database is unavailable.');
    expect(screen.getByRole('status')).toHaveTextContent('Preparing your lesson');
    unmount(); client.clear();
  });

  it('keeps a lesson paused when resuming fails and shows the error with details collapsed', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'cancelled', progressMessage: 'Cancelled' }] } });
    api.retry.mockResolvedValue({ ok: false, error: 'The database is unavailable.' });
    const { client, unmount } = show();
    await userEvent.click(await screen.findByRole('button', { name: 'Hide details' }));
    await userEvent.click(screen.getByRole('button', { name: 'Resume lesson preparation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The database is unavailable.');
    expect(screen.getByRole('status')).toHaveTextContent('Lesson preparation paused');
    expect(screen.getByRole('button', { name: 'Resume lesson preparation' })).toBeEnabled();
    unmount(); client.clear();
  });
});
