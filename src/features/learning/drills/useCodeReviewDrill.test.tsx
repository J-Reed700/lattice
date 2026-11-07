import type { ReactNode } from 'react';

import { act, renderHook } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodeReviewResultDto, StartedCodeReviewDto } from '@/lib/bindings';

import { useCodeReviewDrill } from './useCodeReviewDrill';

const api = vi.hoisted(() => ({
  codeReviewLanguages: vi.fn(),
  startReviewDrill: vi.fn(),
  submitCodeReview: vi.fn(),
  codeReviewResult: vi.fn(),
  regradeCodeReview: vi.fn(),
}));
vi.mock('./api', () => ({ drillsApi: api }));

const DRILL = { title: 'A rate limiter', material: { kind: 'code', language: 'python' } as const, files: [{ path: 'limiter.py', text: 'allow = True\n' }] };

const started: StartedCodeReviewDto = { status: 'started', attemptId: 'attempt-1', mode: 'practice', startedAt: 1, drill: DRILL };
const building: StartedCodeReviewDto = { status: 'building', jobId: 'job-9' };

function attempt(status: CodeReviewResultDto['status']): CodeReviewResultDto {
  return {
    attemptId: 'attempt-1',
    mode: 'practice',
    status,
    startedAt: 1,
    submittedAt: 2,
    timeSpentMs: 1,
    drill: DRILL,
    findings: [],
    fixFiles: null,
    error: null,
    graderModel: null,
    gradedAt: null,
    reveal: null,
  };
}

function render(path: string) {
  const Wrapper = ({ children }: { children: ReactNode }) => <MemoryRouter initialEntries={[path]}>{children}</MemoryRouter>;
  return renderHook(() => useCodeReviewDrill(), { wrapper: Wrapper });
}

/** Lets the timers up to `ms` fire, and the promises they start settle. */
const elapse = (ms: number) => act(async () => {
  await vi.advanceTimersByTimeAsync(ms);
});

beforeEach(() => {
  localStorage.clear();
  for (const mock of Object.values(api)) mock.mockReset();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('waiting for a grade', () => {
  it('reads the attempt again every ten seconds, in case a report was missed, until it is graded', async () => {
    api.codeReviewResult
      .mockResolvedValueOnce(attempt('submitted'))
      .mockResolvedValueOnce(attempt('submitted'))
      .mockResolvedValue(attempt('graded'));
    const { result } = render('/studio?drills=open&attempt=attempt-1');
    await elapse(0);
    expect(result.current.stage).toBe('grading');
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);

    await elapse(9_999);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(2);
    expect(result.current.stage).toBe('grading');

    await elapse(10_000);
    expect(result.current.stage).toBe('revealed');
    expect(api.codeReviewResult).toHaveBeenCalledTimes(3);
    await elapse(120_000);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(3);
  });

  it('keeps reading while a read fails, shows the message, and clears it with the next one that works', async () => {
    api.codeReviewResult
      .mockResolvedValueOnce(attempt('submitted'))
      .mockRejectedValueOnce(new Error('The database is busy.'))
      .mockResolvedValue(attempt('submitted'));
    const { result } = render('/studio?drills=open&attempt=attempt-1');
    await elapse(0);
    await elapse(10_000);
    expect(result.current.failure).toEqual({ message: 'The database is busy.', retry: 'follow' });
    await elapse(10_000);
    expect(result.current.failure).toBeNull();
    expect(result.current.stage).toBe('grading');
  });

  it('does not read an attempt that could not be graded until the learner asks for the grade again', async () => {
    api.codeReviewResult.mockResolvedValue({ ...attempt('couldnt_grade'), error: 'No assessment.' });
    const { result } = render('/studio?drills=open&attempt=attempt-1');
    await elapse(0);
    expect(result.current.stage).toBe('grading');
    await elapse(120_000);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);
  });

  it('stops reading when the screen is left', async () => {
    api.codeReviewResult.mockResolvedValue(attempt('submitted'));
    const { unmount } = render('/studio?drills=open&attempt=attempt-1');
    await elapse(0);
    unmount();
    await elapse(120_000);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);
  });
});

describe('waiting for a build', () => {
  it('asks for the drill again every thirty seconds, in case a report was missed, and opens it when it is ready', async () => {
    api.startReviewDrill.mockResolvedValueOnce(building).mockResolvedValueOnce(building).mockResolvedValueOnce(started);
    const { result } = render('/studio?drills=open');
    act(() => result.current.start());
    await elapse(0);
    expect(result.current.stage).toBe('starting');
    expect(result.current.building?.jobId).toBe('job-9');
    expect(api.startReviewDrill).toHaveBeenCalledTimes(1);

    await elapse(29_999);
    expect(api.startReviewDrill).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect(api.startReviewDrill).toHaveBeenCalledTimes(2);
    expect(result.current.stage).toBe('starting');

    await elapse(30_000);
    expect(result.current.stage).toBe('reviewing');
    expect(result.current.session?.attemptId).toBe('attempt-1');
    await elapse(120_000);
    expect(api.startReviewDrill).toHaveBeenCalledTimes(3);
  });

  it('does not ask again once the learner stops waiting, nor after a failure', async () => {
    api.startReviewDrill.mockResolvedValueOnce(building);
    const { result } = render('/studio?drills=open');
    act(() => result.current.start());
    await elapse(0);
    act(() => result.current.stopWaiting());
    expect(result.current.stage).toBe('choose');
    await elapse(120_000);
    expect(api.startReviewDrill).toHaveBeenCalledTimes(1);

    api.startReviewDrill.mockRejectedValueOnce(new Error('No model is loaded.'));
    act(() => result.current.start());
    await elapse(0);
    expect(result.current.failure).toEqual({ message: 'No model is loaded.', retry: 'start' });
    await elapse(120_000);
    expect(api.startReviewDrill).toHaveBeenCalledTimes(2);
  });

  it('refuses to start an interview whose time limit is not a number of minutes', async () => {
    const { result } = render('/studio?drills=open');
    act(() => result.current.setOptions({ mode: 'interview', minutes: Number.NaN }));
    act(() => result.current.start());
    await elapse(0);
    expect(result.current.stage).toBe('choose');
    expect(api.startReviewDrill).not.toHaveBeenCalled();
  });
});
