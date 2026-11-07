import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonPreparationStatus } from '@/features/learning/lessons/LessonPreparationProgress';
import type { LearningGenerationJob } from '@/lib/bindings';

/**
 * A lesson preparation that hit a model or network outage waits and retries by
 * itself (`pending` with `error` set). Its cause belongs on the page the
 * learner is looking at, with the way to fix a model that is not reachable, not
 * inside the collapsed Details.
 */

const api = vi.hoisted(() => ({ plan: vi.fn(), cancel: vi.fn(), retry: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, cancelLearningGenerationJob: api.cancel, retryLearningGenerationJob: api.retry,
  getLearningLessonClaimReview: api.review,
} }));

const CAUSE = 'The model or network is temporarily unavailable. Preparation retries automatically while Lattice is open.';

const job: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 44, status: 'running', progressCompleted: 0,
  activity: null, progressTotal: 1, progressMessage: 'Writing the first lesson · 420 response characters received',
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000,
  startedAt: Date.now() - 60_000, finishedAt: null,
};
const waiting: LearningGenerationJob = { ...job, status: 'pending', error: CAUSE, progressMessage: 'Connection interrupted; saved work will resume automatically' };

function Harness() {
  const plan = useLearningPlan('program-1');
  return <LessonPreparationStatus job={plan.data?.jobs[0]} pending={false} programId="program-1" revision={44} />;
}

function SettingsProbe() {
  const location = useLocation();
  return <p data-testid="settings-page">{location.pathname}{location.search}</p>;
}

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><MemoryRouter><Routes>
    <Route path="/" element={<Harness />} />
    <Route path="/settings" element={<SettingsProbe />} />
  </Routes></MemoryRouter></QueryClientProvider>);
}

describe('lesson preparation waiting out an outage', () => {
  beforeEach(() => vi.resetAllMocks());

  it('shows the cause without opening Details, and offers the model settings', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [waiting] } });
    show();
    expect(await screen.findByRole('status')).toHaveTextContent('Preparation will resume automatically');
    expect(screen.getByRole('button', { name: 'Details' })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByText(CAUSE)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Open model settings' }));
    expect(await screen.findByTestId('settings-page')).toHaveTextContent('/settings?tab=downloaded-models');
  });

  it('reads as it does today for a preparation that is running', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [job] } });
    show();
    expect(await screen.findByRole('status')).toHaveTextContent('Preparing your lesson');
    expect(screen.queryByText(CAUSE)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open model settings' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
  });

  it('adds no model settings button to a paused preparation', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...job, status: 'cancelled', progressMessage: 'Cancelled' }] } });
    show();
    expect(await screen.findByRole('status')).toHaveTextContent('Lesson preparation paused');
    expect(screen.queryByRole('button', { name: 'Open model settings' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Resume lesson preparation' })).toBeEnabled();
  });
});
