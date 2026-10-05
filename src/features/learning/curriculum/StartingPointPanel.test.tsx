import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { StartingPointPanel } from '@/features/learning/curriculum/StartingPointPanel';
import type { LearningDiagnosticAttemptDto, SubmitLearningDiagnosticRequestDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';


const mocks = vi.hoisted(() => ({ plan: vi.fn(), program: vi.fn(), start: vi.fn(), submit: vi.fn(), skip: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningPlan: mocks.plan, getLearningProgram: mocks.program, startLearningDiagnostic: mocks.start, submitLearningDiagnostic: mocks.submit, skipLearningDiagnostic: mocks.skip } }));
let attempt: LearningDiagnosticAttemptDto;
const responses = new Map<string, LearningDiagnosticAttemptDto>();
const ok = <T,>(data: T) => ({ ok: true as const, data: structuredClone(data) });
function show(onStudy = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><StartingPointPanel programId="program" onStudy={onStudy} /></QueryClientProvider>);
}

describe('starting-point performance tasks', () => {
  beforeEach(() => {
    vi.resetAllMocks(); responses.clear();
    attempt = { id: 'diagnostic', programId: 'program', status: 'active', revision: 0, prompts: [{ id: 'prompt', prompt: 'Predict the result of adding each positive value to an accumulator initialized to zero.', outcomeId: 'outcome', outcomeTitle: 'Reason about accumulation', sourceVersionIds: [] }], responses: [], sourceCoverageGaps: [], findings: [], interpretation: 'Provisional starting-point feedback.', createdAt: 1, submittedAt: null };
    mocks.program.mockResolvedValue(ok({ summary: { id: 'program', revision: 9 } }));
    mocks.plan.mockImplementation(async () => ok({ latestDiagnostic: attempt, acceptedRevision: { modules: [{ outcomeIds: ['outcome'], lessons: [{ id: 'lesson' }] }] } }));
    mocks.submit.mockImplementation(async (request: SubmitLearningDiagnosticRequestDto) => {
      if (responses.has(request.operationId)) return ok(responses.get(request.operationId)!);
      if (request.expectedDiagnosticRevision !== attempt.revision) return { ok: false, error: 'The starting-point answers changed elsewhere. Reload before saving.' };
      attempt.responses = request.responses; attempt.revision = (attempt.revision ?? 0) + 1;
      if (!request.saveOnly) { attempt.status = 'submitted'; attempt.findings = [{ promptId: 'prompt', outcomeId: 'outcome', signal: 'needs_practice', feedback: 'Track the accumulator after each iteration and check the empty list.', evidenceQuote: 'The result' }]; }
      responses.set(request.operationId, structuredClone(attempt));
      return ok(attempt);
    });
  });

  it('autosaves answers and restores them after reopening without evaluating a draft', async () => {
    const user = userEvent.setup(); const view = show();
    await user.type(await screen.findByRole('textbox', { name: 'Your reasoning' }), 'The result accumulates the positive values.');
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledWith(expect.objectContaining({ saveOnly: true, expectedRevision: 9, expectedDiagnosticRevision: 0 })));
    await waitFor(() => expect(attempt.revision).toBe(1));
    view.unmount(); show();
    expect(await screen.findByRole('textbox', { name: 'Your reasoning' })).toHaveValue('The result accumulates the positive values.');
    expect(attempt.status).toBe('active');
  });

  it('flushes answers before grading and turns feedback into a study action', async () => {
    const user = userEvent.setup(); const study = vi.fn(); show(study);
    await user.type(await screen.findByRole('textbox', { name: 'Your reasoning' }), 'The result is always the last value.');
    await user.click(screen.getByRole('button', { name: 'Get my starting-point feedback' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledWith(expect.objectContaining({ saveOnly: false, expectedDiagnosticRevision: 1 })));
    await user.click(await screen.findByRole('button', { name: 'Study this topic' }));
    expect(study).toHaveBeenCalledWith('lesson');
  });


  it('recovers a submission conflict with fresh course and answer revisions', async () => {
    const user = userEvent.setup(); show();
    await user.type(await screen.findByRole('textbox', { name: 'Your reasoning' }), 'The result is a running total.');
    await waitFor(() => expect(attempt.revision).toBe(1));
    attempt.revision = 2;
    mocks.program.mockResolvedValue(ok({ summary: { id: 'program', revision: 10 } }));
    await user.click(screen.getByRole('button', { name: 'Get my starting-point feedback' }));
    await user.click(await screen.findByRole('button', { name: 'Keep my answers and retry' }));
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
    expect(screen.getByRole('textbox', { name: 'Your reasoning' })).toHaveValue('The result is a running total.');
    await user.click(screen.getByRole('button', { name: 'Get my starting-point feedback' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenLastCalledWith(expect.objectContaining({ saveOnly: false, expectedRevision: 10, expectedDiagnosticRevision: 2 })));
    expect(await screen.findByRole('button', { name: 'Study this topic' })).toBeVisible();
  });

  it('keeps edits after a save failure and retries the exact operation before navigation', async () => {
    const user = userEvent.setup();
    mocks.submit.mockImplementationOnce(async () => ({ ok: false, error: 'Connection interrupted' }));
    show();
    await user.type(await screen.findByRole('textbox', { name: 'Your reasoning' }), 'The result depends on the values.');
    await screen.findByText('Connection interrupted');
    const failed = mocks.submit.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry with my saved answers' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledTimes(2));
    expect(mocks.submit.mock.calls[1][0]).toEqual(failed);
    expect(await flushPendingSaves()).toBe(true);
  });
});
