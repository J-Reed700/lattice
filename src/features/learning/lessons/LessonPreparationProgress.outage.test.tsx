import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonPreparationStatus } from '@/features/learning/lessons/LessonPreparationProgress';
import type { LearningGenerationJob } from '@/lib/bindings';
import { expectPlainError, findAlertWith } from '@/tests/plainErrors';

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

/** What `lesson_failure` records for a pending job: the raw AppError text, then the recovery paragraph. */
const CAUSE = 'Network error: llama.cpp request failed: connection refused (os error 10061) Your draft and completed checks are saved. Preparation retries automatically while Lattice is open, with increasing delays of up to five minutes. If the same failure comes back with nothing saved in between, preparation stops and shows it instead of retrying again. You can leave this screen or cancel.';
const WAITING = "Can't reach the model server. Lattice keeps trying.";
const RATE_LIMITED = 'The model is getting too many requests. Lattice keeps trying.';

const job: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 44, status: 'running', progressCompleted: 0,
  activity: null, progressTotal: 1, progressMessage: 'Writing the first lesson · 420 response characters received',
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000,
  startedAt: Date.now() - 60_000, finishedAt: null,
};
const waiting: LearningGenerationJob = { ...job, status: 'pending', error: CAUSE, errorCode: 'NETWORK_ERROR', progressMessage: 'Connection interrupted; saved work will resume automatically' };

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

  it('says the wait in a plain sentence from the code, keeps the raw text under Details, and offers the model settings', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [waiting] } });
    show();
    expect(await screen.findByRole('status')).toHaveTextContent('Preparation will resume automatically');
    expect(screen.getByRole('button', { name: 'Details' })).toHaveAttribute('aria-expanded', 'false');
    await expectPlainError(await findAlertWith(WAITING), { headline: WAITING, raw: CAUSE });
    expect(screen.getByRole('button', { name: 'Pause lesson preparation' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Open model settings' }));
    expect(await screen.findByTestId('settings-page')).toHaveTextContent('/settings?tab=downloaded-models');
  });

  it('reads a rate-limited wait as its own sentence', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [{ ...waiting, error: 'Rate limit exceeded: llama.cpp returned 429 Your draft and completed checks are saved.', errorCode: 'RATE_LIMIT_EXCEEDED' }] } });
    show();
    expect(await screen.findByText(RATE_LIMITED)).toBeVisible();
    expect(screen.queryByText(WAITING)).not.toBeInTheDocument();
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
