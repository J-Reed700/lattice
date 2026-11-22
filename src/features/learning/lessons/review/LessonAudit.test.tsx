import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { learningClaimReviewKey } from '@/features/learning/api/learningQueryKeys';
import { AuditLessonNow, LessonAuditLine } from '@/features/learning/lessons/review/LessonAudit';
import { resetLessonAuditStore } from '@/features/learning/lessons/review/lessonAuditStore';
import type { LearningLessonAuditDto, LearningLessonAuditResultDto } from '@/lib/bindings';
import { deferred } from '@/tests/deferred';

const api = vi.hoisted(() => ({
  verifier: vi.fn(),
  lessonAudit: vi.fn(),
  auditNow: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  default: {
    getLearningProgramVerifierAudit: api.verifier,
    getLearningLessonAudit: api.lessonAudit,
    auditLearningLessonNow: api.auditNow,
  },
}));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const optIn = {
  programId: 'program-1',
  on: true,
  webSearch: false,
  consentAt: 1,
  consentProvider: null,
  verifier: null,
  destination: 'your Verifier',
  consentRequiredFor: null,
  blocked: null,
  usage: { audits: 0, inputTokens: 0, outputTokens: 0 },
};

describe('LessonAudit persistence and polling', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    resetLessonAuditStore();
    api.verifier.mockResolvedValue(ok(optIn));
    api.lessonAudit.mockResolvedValue(ok(null));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('preserves AuditLessonNow pending, result, and error state across unmount and remount', async () => {
    const user = userEvent.setup();
    const run = deferred<{ ok: true; data: LearningLessonAuditResultDto }>();
    api.auditNow.mockReturnValueOnce(run.promise);

    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

    const view1 = render(<AuditLessonNow programId="program-1" lessonId="lesson-1" />, { wrapper });
    await user.click(await screen.findByRole('button', { name: 'Audit this lesson now' }));
    expect(screen.getByRole('button', { name: 'Auditing…' })).toBeDisabled();
    view1.unmount();

    const view2 = render(<AuditLessonNow programId="program-1" lessonId="lesson-1" />, { wrapper });
    expect(await screen.findByRole('button', { name: 'Auditing…' })).toBeDisabled();
    expect(api.auditNow).toHaveBeenCalledTimes(1);

    await act(async () => {
      run.resolve(ok({ outcome: 'completed', disputes: 2, message: null, fix: null } as unknown as LearningLessonAuditResultDto));
    });

    expect(await screen.findByRole('status')).toHaveTextContent('The Verifier disputes 2 statements');
    view2.unmount();

    render(<AuditLessonNow programId="program-1" lessonId="lesson-1" />, { wrapper });
    expect(await screen.findByRole('status')).toHaveTextContent('The Verifier disputes 2 statements');
  });

  it('polls running LessonAuditLine and invalidates claim review when audit completes', async () => {
    vi.useFakeTimers();
    const runningAudit: LearningLessonAuditDto = {
      lessonId: 'lesson-1',
      verifierModel: 'qwen-verifier',
      batchesDone: 1,
      batchesPlanned: 4,
      inputTokens: 1_000,
      outputTokens: 200,
      lastError: null,
      webNote: null,
      startedAt: 1,
      finishedAt: null,
      state: 'running',
    };
    const completedAudit: LearningLessonAuditDto = {
      ...runningAudit,
      batchesDone: 4,
      finishedAt: 10,
      state: 'completed',
    };
    api.lessonAudit.mockResolvedValueOnce(ok(runningAudit)).mockResolvedValue(ok(completedAudit));

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries');
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

    render(<LessonAuditLine programId="program-1" lessonId="lesson-1" />, { wrapper });
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.getByText(/batch 2 of 4/)).toBeVisible();

    await act(async () => { await vi.advanceTimersByTimeAsync(2_100); });
    expect(screen.getByText(/Audit: 4 batches/)).toBeVisible();
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: learningClaimReviewKey('program-1', 'lesson-1') });
  });
});
