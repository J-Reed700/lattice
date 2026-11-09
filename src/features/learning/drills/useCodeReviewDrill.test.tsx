import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodeReviewResultDto, StartedCodeReviewDto } from '@/lib/bindings';

import { useCodeReviewDrill } from './useCodeReviewDrill';

const api = vi.hoisted(() => ({
  codeReviewLanguages: vi.fn(),
  startCourseDrill: vi.fn(),
  submitCodeReview: vi.fn(),
  codeReviewResult: vi.fn(),
  regradeCodeReview: vi.fn(),
}));
vi.mock('./api', () => ({ drillsApi: api }));

const DRILL = {
  title: 'A rate limiter',
  material: { kind: 'code', language: 'python' } as const,
  files: [{ path: 'limiter.py', text: 'allow = True\n' }],
  lesson: null,
  issues: 4,
};
const PASSAGE_DRILL = {
  title: 'Why spacing helps',
  material: { kind: 'passage', sourceTitle: 'Spacing and memory' } as const,
  files: [{ path: 'explanation.md', text: 'Spacing helps recall.\n' }],
  lesson: null,
  issues: 3,
};

const started: StartedCodeReviewDto = { status: 'started', attemptId: 'attempt-1', mode: 'practice', startedAt: 1, drill: DRILL };
const building: StartedCodeReviewDto = { status: 'building', jobId: 'job-9' };

function attempt(status: CodeReviewResultDto['status'], overrides: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
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
    ...overrides,
  };
}

function render(programId = 'course-1') {
  return renderHook(() => useCodeReviewDrill(programId));
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
    const { result } = render();
    act(() => result.current.restore('attempt-1'));
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
    const { result } = render();
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    await elapse(10_000);
    expect(result.current.failure).toEqual({ message: 'The database is busy.', retry: 'follow' });
    await elapse(10_000);
    expect(result.current.failure).toBeNull();
    expect(result.current.stage).toBe('grading');
  });

  it('does not read an attempt that could not be graded until the learner asks for the grade again', async () => {
    api.codeReviewResult.mockResolvedValue({ ...attempt('couldnt_grade'), error: 'No assessment.' });
    const { result } = render();
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    expect(result.current.stage).toBe('grading');
    await elapse(120_000);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);
  });

  it('stops reading when the screen is left', async () => {
    api.codeReviewResult.mockResolvedValue(attempt('submitted'));
    const { result, unmount } = render();
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    unmount();
    await elapse(120_000);
    expect(api.codeReviewResult).toHaveBeenCalledTimes(1);
  });
});

describe('waiting for a build', () => {
  it('asks for the drill again every thirty seconds, in case a report was missed, and opens it when it is ready', async () => {
    api.startCourseDrill.mockResolvedValueOnce(building).mockResolvedValueOnce(building).mockResolvedValueOnce(started);
    const { result } = render();
    act(() => result.current.start({ language: 'python' }));
    await elapse(0);
    expect(result.current.stage).toBe('starting');
    expect(result.current.building?.jobId).toBe('job-9');
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'python' }, 'practice', 80, 4);

    await elapse(29_999);
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
    expect(result.current.stage).toBe('starting');

    await elapse(30_000);
    expect(result.current.stage).toBe('reviewing');
    expect(result.current.session?.attemptId).toBe('attempt-1');
    await elapse(120_000);
    expect(api.startCourseDrill).toHaveBeenCalledTimes(3);
  });

  it('does not ask again once the learner stops waiting, nor after a failure', async () => {
    api.startCourseDrill.mockResolvedValueOnce(building);
    const { result } = render();
    act(() => result.current.start({ language: 'python' }));
    await elapse(0);
    act(() => result.current.stopWaiting());
    expect(result.current.stage).toBe('choose');
    await elapse(120_000);
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);

    api.startCourseDrill.mockRejectedValueOnce(new Error('No model is loaded.'));
    act(() => result.current.start());
    await elapse(0);
    expect(result.current.failure).toEqual({ message: 'No model is loaded.', retry: 'start' });
    await elapse(120_000);
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
  });

  it('refuses to start an interview whose time limit is not a number of minutes', async () => {
    const { result } = render();
    act(() => result.current.setOptions({ mode: 'interview', minutes: Number.NaN }));
    act(() => result.current.start({ language: 'python' }));
    await elapse(0);
    expect(result.current.stage).toBe('choose');
    expect(api.startCourseDrill).not.toHaveBeenCalled();
  });
});

describe('the language of a drill', () => {
  it('is not started in code before a language is chosen', async () => {
    const { result } = render();
    expect(result.current.stage).toBe('choose');
    expect(result.current.options.language).toBeNull();
    act(() => result.current.start());
    await elapse(0);
    expect(result.current.stage).toBe('choose');
    expect(api.startCourseDrill).not.toHaveBeenCalled();
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBeNull();
  });

  it('is written for the course when a drill is started in it, and a new hook for the course starts from it', async () => {
    api.startCourseDrill.mockResolvedValue(building);
    const first = render();
    act(() => first.result.current.start({ language: 'rust' }));
    await elapse(0);
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBe('rust');
    first.unmount();

    expect(render('course-1').result.current.options.language).toBe('rust');
    expect(render('course-2').result.current.options.language).toBeNull();
  });

  it('is left alone when a drill is started on a passage', async () => {
    api.startCourseDrill.mockResolvedValue(building);
    const { result } = render();
    act(() => result.current.start({ material: 'passage', language: 'rust' }));
    await elapse(0);
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'passage' }, 'practice', 10, 3);
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBeNull();
  });
});

describe('the lesson of a drill', () => {
  it('is the one picked, or none for the one next up', async () => {
    api.startCourseDrill.mockResolvedValue(building);
    const { result } = render();
    act(() => result.current.start({ language: 'python', lessonId: 'lesson-3' }));
    await elapse(0);
    expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', 'lesson-3', { kind: 'code', language: 'python' }, 'practice', 80, 4);
    act(() => result.current.stopWaiting());
    act(() => result.current.start({ lessonId: null }));
    await elapse(0);
    expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', null, { kind: 'code', language: 'python' }, 'practice', 80, 4);
  });

  it('is the one next up for the next drill, in the same language and mode as the one just done', async () => {
    api.codeReviewResult.mockResolvedValue(
      attempt('graded', { mode: 'interview', drill: { ...DRILL, material: { kind: 'code', language: 'rust' } } }),
    );
    api.startCourseDrill.mockResolvedValue(building);
    const { result } = render();
    act(() => result.current.setOptions({ lessonId: 'lesson-3', size: 'long' }));
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    expect(result.current.stage).toBe('revealed');
    act(() => result.current.nextDrill());
    await elapse(0);
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'rust' }, 'interview', 140, 5);
  });

  it('is the one next up for the next drill on a passage', async () => {
    api.codeReviewResult.mockResolvedValue(attempt('graded', { drill: PASSAGE_DRILL }));
    api.startCourseDrill.mockResolvedValue(building);
    const { result } = render();
    act(() => result.current.setOptions({ lessonId: 'lesson-3' }));
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    act(() => result.current.nextDrill());
    await elapse(0);
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'passage' }, 'practice', 10, 3);
  });
});

describe('leaving a drill', () => {
  it('goes back to the choices from a review, which is kept for when it is opened again', async () => {
    api.codeReviewResult.mockResolvedValue(attempt('open'));
    const { result } = render();
    act(() => result.current.restore('attempt-1'));
    await elapse(0);
    expect(result.current.stage).toBe('reviewing');
    act(() => result.current.toChoices());
    expect(result.current.stage).toBe('choose');
    expect(result.current.session).toBeNull();
  });
});
