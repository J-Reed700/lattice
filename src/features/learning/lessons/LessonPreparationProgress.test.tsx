import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
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
  progressTotal: 1, progressMessage: 'Writing the first lesson · 420 response characters received',
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

  it('rejoins an existing job, polls real progress, and refreshes the lesson on completion', async () => {
    let current = job;
    api.plan.mockImplementation(async () => ({ ok: true, data: { programId: 'program-1', jobs: [current] } }));
    const { client, unmount } = show();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    expect(await screen.findByText(/420 response characters/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Cancel lesson preparation' })).toBeEnabled();
    current = { ...job, status: 'completed', progressCompleted: 1 };
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Lesson preparation progress' })).not.toBeInTheDocument(), { timeout: 4_000 });
    expect(api.plan.mock.calls.length).toBeGreaterThan(1);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['learning-program', 'program-1'] });
    unmount(); client.clear();
  });

  it('cancels the durable job from the lesson view and offers an explicit retry', async () => {
    const user = userEvent.setup();
    let current = job;
    api.plan.mockImplementation(async () => ({ ok: true, data: { jobs: [current] } }));
    api.cancel.mockImplementation(async () => {
      current = { ...job, status: 'cancelled', error: 'Cancelled by you.' };
      return { ok: true, data: current };
    });
    api.retry.mockImplementation(async () => {
      current = { ...job, id: 'job-2', status: 'pending', progressMessage: 'Retry queued' };
      return { ok: true, data: current };
    });
    const { client, unmount } = show();
    await user.click(await screen.findByRole('button', { name: 'Cancel lesson preparation' }));
    expect(api.cancel).toHaveBeenCalledWith(expect.objectContaining({ programId: 'program-1', jobId: 'job-1', expectedRevision: 44 }));
    await user.click(await screen.findByRole('button', { name: 'Retry lesson preparation' }));
    expect(await screen.findByText('Retry queued')).toBeVisible();
    expect(api.retry).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'job-1' }));
    unmount(); client.clear();
  });

  it('shows a cancellation failure without pretending the job stopped', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [job] } });
    api.cancel.mockResolvedValue({ ok: false, error: 'The database is unavailable.' });
    const { client, unmount } = show();
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel lesson preparation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The database is unavailable.');
    expect(screen.getByRole('status')).toHaveTextContent('Preparing your lesson');
    unmount(); client.clear();
  });
});
