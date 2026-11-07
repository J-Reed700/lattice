import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PlanPanel } from '@/features/learning/curriculum/PlanPanel';
import { expectPlainError, findAlertWith, NO_LEARNING_MODEL_TEXT } from '@/tests/plainErrors';

/**
 * A job that failed keeps its cause as text (`LearningGenerationJob.error`, the
 * backend's `AppError` text, no code). The Plan tab shows it as a plain
 * sentence, with that text under Details.
 */

const mocks = vi.hoisted(() => ({ getPlan: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: mocks.getPlan,
  getLearningProgramVerifierAudit: async (programId: string) => ({ ok: true, data: {
    programId, on: false, webSearch: false, consentAt: null, consentProvider: null, verifier: null,
    destination: null, consentRequiredFor: null, blocked: null, usage: { audits: 0, inputTokens: 0, outputTokens: 0 },
  } }),
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const lesson = { id: 'lesson-1', title: 'Build a source-backed explanation', objective: 'Connect an observation to a conclusion.', estimatedMinutes: 25, state: 'outline', assessmentStarted: false, replacementLessonId: null };
const module = { id: 'module-1', title: 'Reading evidence', purpose: 'Practice reasoning from observations.', prerequisiteModuleIds: [], outcomeIds: ['outcome-1'], lessons: [lesson] };
const acceptedRevision = { id: 'revision-1', programId: 'program-1', revisionNumber: 4, parentRevisionId: null, reason: 'Initial plan.', modules: [module], createdAt: 1_790_000_000_000 };
const failedJob = (kind: string, error: string, errorCode?: string) => ({
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind, payloadSha256: 'sha', baseRevisionNumber: 4, status: 'failed',
  progressCompleted: 2, progressTotal: 5, progressMessage: 'Stopped', activity: null, resultId: null, error, errorCode, retryOfJobId: null,
  createdAt: 1_790_000_000_000, startedAt: 1_790_000_000_000, finishedAt: 1_790_000_060_000,
});
const plan = (jobs: unknown[]) => ({
  programId: 'program-1', programRevision: 9, acceptedRevision, draftRevision: null, previewChanges: [],
  requiredLessonCountBefore: 1, requiredLessonCountAfter: 1, resumeLessonId: lesson.id, jobs, latestDiagnostic: null,
});

function renderPlan() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<PlanPanel programId="program-1" />, { wrapper });
}

const NETWORK = 'llama.cpp request failed: connection refused (os error 10061)';
const UNKNOWN = 'The quiz bank builder returned no usable questions after 3 attempts.';

const causes = [
  { name: 'the model server cannot be reached', error: NETWORK, errorCode: 'NETWORK_ERROR', headline: 'Lattice couldn\'t reach the model server.' },
  { name: 'no Learning model is set', error: NO_LEARNING_MODEL_TEXT, errorCode: 'LEARNING_MODEL_NOT_SET', headline: 'Courses need a model to write them.' },
  { name: 'the job recorded no code', error: UNKNOWN, errorCode: undefined, headline: 'Something went wrong.' },
];

describe('Plan tab: a failed job', () => {
  beforeEach(() => vi.resetAllMocks());

  describe.each(causes)('when $name', ({ error, errorCode, headline }) => {
    it.each(['quiz_bank', 'lesson_preparation'])('says so plainly for a failed %s job, with the backend text under Details', async (kind) => {
      mocks.getPlan.mockResolvedValue(ok(plan([failedJob(kind, error, errorCode)])));
      renderPlan();
      const alert = await findAlertWith(headline);
      await expectPlainError(alert, { headline, raw: error });
    });
  });

  it('shows a job that is running with no error at all', async () => {
    mocks.getPlan.mockResolvedValue(ok(plan([{ ...failedJob('quiz_bank', ''), status: 'running', error: null, progressMessage: 'Building the question bank' }])));
    renderPlan();
    expect(await screen.findByText('Building the question bank')).toBeVisible();
    expect(screen.queryByText('Something went wrong.')).not.toBeInTheDocument();
    expect(within(screen.getByTestId('learning-plan-panel')).queryByRole('alert')).not.toBeInTheDocument();
  });
});
