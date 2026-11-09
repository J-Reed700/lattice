import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { usePracticeSession } from '@/features/learning/practice/usePracticeSession';
import type * as Wire from '@/lib/bindings';

const mocks = vi.hoisted(() => ({
  getWorkspace: vi.fn(),
  getSession: vi.fn(),
  save: vi.fn(),
  tutor: vi.fn(),
  mode: vi.fn(),
  openSource: vi.fn(),
  reveal: vi.fn(),
  submit: vi.fn(),
}));

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  default: {
    getLearningPracticeWorkspace: mocks.getWorkspace,
    getLearningPracticeSession: mocks.getSession,
    saveLearningPracticeArtifact: mocks.save,
    requestLearningTutorResponse: mocks.tutor,
    changeLearningPracticeMode: mocks.mode,
    openLearningPracticeSource: mocks.openSource,
    revealLearningPracticeSolution: mocks.reveal,
    submitLearningPracticeAttempt: mocks.submit,
  },
}));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const lesson = { id: 'lesson-1', title: 'Read the evidence', preparation: 'ready', objective: 'Distinguish observation from inference.' } as Wire.LearningLessonDto;
const program = { summary: { id: 'program-1', title: 'Evidence', revision: 8 } } as Wire.LearningProgramDto;
const conflictError = (message: string) => Object.assign(new Error(message), { details: { code: 'CONFLICT' } });

let summary: Wire.LearningPracticeSessionSummaryDto;
let session: Wire.LearningPracticeSessionDto;

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function setup() {
  return renderHook(() => usePracticeSession({ program, lesson }), { wrapper });
}

async function ready(result: { current: ReturnType<typeof usePracticeSession> }) {
  await waitFor(() => expect(result.current.session?.summary.id).toBe('attempt-1'));
  await waitFor(() => expect(result.current.autosave.state).toBe('idle'));
}

describe('usePracticeSession', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    summary = {
      id: 'attempt-1', lessonId: 'lesson-1', lessonTitle: lesson.title, status: 'active', mode: 'explore', revision: 4,
      artifactRevision: 0, sourceVersionIds: [], createdAt: 1, updatedAt: 1, submittedAt: null, gradeStatus: null,
    } as Wire.LearningPracticeSessionSummaryDto;
    session = {
      summary, taskPrompt: 'Explain.', lessonObjective: lesson.objective, rubric: [],
      artifact: { revision: 0, text: 'Saved text', sha256: 'digest', updatedAt: 1 }, assistance: [], tutorTurns: [],
      proposals: [], revealedSolution: null, revealedSolutionCitations: [], result: null,
    } as unknown as Wire.LearningPracticeSessionDto;
    mocks.getWorkspace.mockImplementation(async () => ok({ programId: 'program-1', sessions: [summary] }));
    mocks.getSession.mockImplementation(async () => ok(session));
    for (const call of [mocks.mode, mocks.openSource, mocks.reveal, mocks.submit, mocks.tutor]) call.mockImplementation(async () => ok({ programId: 'program-1', sessions: [summary] }));
    mocks.save.mockImplementation(async (request: { text: string }) => {
      session.artifact.text = request.text;
      summary.revision += 1;
      return ok({ programId: 'program-1', sessions: [summary] });
    });
  });

  it('shows the saved response and saves an edit against the attempt revision', async () => {
    const { result } = setup();
    await ready(result);
    expect(result.current.text).toBe('Saved text');

    act(() => result.current.setText('Saved text, then more.'));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1), { timeout: 3_000 });
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: 'attempt-1', expectedRevision: 4, text: 'Saved text, then more.',
    }));
    await waitFor(() => expect(result.current.autosave.state).toBe('saved'));
  });

  it('replays a failed save with the same operation ID when the learner retries', async () => {
    mocks.save.mockRejectedValueOnce(new Error('Connection interrupted'));
    const { result } = setup();
    await ready(result);
    act(() => result.current.setText('Unsent words.'));
    await waitFor(() => expect(result.current.autosave.state).toBe('error'), { timeout: 3_000 });

    await act(async () => {
      await result.current.autosave.flush();
    });
    expect(mocks.save).toHaveBeenCalledTimes(2);
    expect(mocks.save.mock.calls[1][0]).toEqual(mocks.save.mock.calls[0][0]);
  });

  it('saves the response before an action, and says so through saveNow', async () => {
    const { result } = setup();
    await ready(result);
    act(() => result.current.setText('Words saved on demand.'));
    let saved = false;
    await act(async () => {
      saved = await result.current.saveNow();
    });
    expect(saved).toBe(true);
    expect(mocks.save).toHaveBeenCalledTimes(1);
  });

  it('names what it is busy with, and is idle again after', async () => {
    let finish!: () => void;
    mocks.tutor.mockImplementationOnce(() => new Promise((resolve) => {
      finish = () => resolve(ok({ programId: 'program-1', sessions: [summary] }));
    }));
    const { result } = setup();
    await ready(result);
    expect(result.current.busyState).toBe('idle');
    expect(result.current.busy).toBe(false);

    let asking!: Promise<boolean>;
    act(() => {
      asking = result.current.sendTutor('question', 'Why?');
    });
    await waitFor(() => expect(result.current.busyState).toBe('tutor'));
    expect(result.current.busy).toBe(true);
    expect(result.current.busyLabel).toBe('The tutor is responding…');

    await act(async () => {
      finish();
      await asking;
    });
    expect(result.current.busyState).toBe('idle');
    expect(result.current.busy).toBe(false);
  });

  it('treats an error as a conflict by its code, not by its wording', async () => {
    mocks.mode.mockRejectedValueOnce(new Error('The attempt changed elsewhere; reload and retry.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.switchMode('practice');
    });
    expect(result.current.failure?.message).toBe('The attempt changed elsewhere; reload and retry.');
    expect(result.current.conflict).toBe(false);

    mocks.mode.mockRejectedValueOnce(conflictError('Stale.'));
    await act(async () => {
      await result.current.switchMode('practice');
    });
    expect(result.current.conflict).toBe(true);
  });

  it('names the failed action and tries it again with the same operation ID', async () => {
    mocks.tutor.mockRejectedValueOnce(new Error('The tutor could not answer.'));
    mocks.tutor.mockImplementation(async () => ok({ programId: 'program-1', sessions: [summary] }));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.sendTutor('question', 'Why?');
    });
    expect(result.current.failure).toMatchObject({ kind: 'tutor', message: 'The tutor could not answer.' });

    await act(async () => {
      result.current.failure?.retry?.();
    });
    await waitFor(() => expect(mocks.tutor).toHaveBeenCalledTimes(2));
    expect(mocks.tutor.mock.calls[1][0]).toEqual(mocks.tutor.mock.calls[0][0]);
    await waitFor(() => expect(result.current.failure).toBeNull());
  });

  it('offers no Try again after a conflict, only the reload', async () => {
    mocks.mode.mockRejectedValueOnce(conflictError('Stale.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.switchMode('practice');
    });
    expect(result.current.conflict).toBe(true);
    expect(result.current.failure).toMatchObject({ kind: 'changing_mode', message: 'Stale.' });
    expect(result.current.failure?.retry).toBeUndefined();
  });

  it('saves the learner text against the refetched revision with one reload after an autosave conflict', async () => {
    mocks.save.mockImplementationOnce(async () => {
      summary.revision = 9;
      throw conflictError('Stale.');
    });
    const { result } = setup();
    await ready(result);
    act(() => result.current.setText('My own words.'));
    await waitFor(() => expect(result.current.conflict).toBe(true), { timeout: 3_000 });

    await act(async () => {
      await result.current.reloadAttempt();
    });
    await waitFor(() => expect(result.current.autosave.state).toBe('saved'));
    expect(mocks.save).toHaveBeenLastCalledWith(expect.objectContaining({ expectedRevision: 9, text: 'My own words.' }));
    expect(result.current.conflict).toBe(false);
    expect(result.current.text).toBe('My own words.');
  });

  it('clears the conflict and the failure once the attempt is reloaded', async () => {
    mocks.mode.mockRejectedValueOnce(conflictError('Stale.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.switchMode('practice');
    });
    await act(async () => {
      await result.current.reloadAttempt();
    });
    expect(result.current.conflict).toBe(false);
    expect(result.current.failure).toBeNull();
  });

  it('keeps the opened source selected when the open succeeds on Try again', async () => {
    mocks.openSource.mockRejectedValueOnce(new Error('Could not open it.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.doOpenSource({ id: 'source-1' } as Wire.LearningSourceLibraryItemDto, 'version-1');
    });
    expect(result.current.selectedSource).toBeNull();
    await act(async () => {
      result.current.failure?.retry?.();
    });
    await waitFor(() => expect(result.current.selectedSource).toEqual({ sourceId: 'source-1', versionId: 'version-1' }));
  });

  it('closes the reveal confirmation when the reveal succeeds on Try again', async () => {
    mocks.reveal.mockRejectedValueOnce(new Error('Could not reveal it.'));
    const { result } = setup();
    await ready(result);
    act(() => result.current.setSolutionConfirm(true));
    await act(async () => {
      await result.current.doReveal();
    });
    expect(result.current.solutionConfirm).toBe(true);
    await act(async () => {
      result.current.failure?.retry?.();
    });
    await waitFor(() => expect(result.current.solutionConfirm).toBe(false));
  });

  it('takes the chosen help when the change succeeds on Try again', async () => {
    mocks.mode.mockRejectedValueOnce(new Error('Could not change it.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.switchMode('practice');
    });
    expect(result.current.modeChoice).toBe('explore');
    await act(async () => {
      result.current.failure?.retry?.();
    });
    await waitFor(() => expect(result.current.modeChoice).toBe('practice'));
  });

  it('marks a hint used when the request succeeds on Try again', async () => {
    mocks.tutor.mockRejectedValueOnce(new Error('The tutor could not answer.'));
    const { result } = setup();
    await ready(result);
    await act(async () => {
      await result.current.sendTutor('hint', 'Give me a hint', 'orienting_question');
    });
    expect(result.current.hintUsed).toEqual([]);
    await act(async () => {
      result.current.failure?.retry?.();
    });
    await waitFor(() => expect(result.current.hintUsed).toEqual(['orienting_question']));
  });

  it('labels what it is doing in the learner words', async () => {
    let finishMode!: () => void;
    mocks.mode.mockImplementationOnce(() => new Promise((resolve) => {
      finishMode = () => resolve(ok({ programId: 'program-1', sessions: [summary] }));
    }));
    const { result } = setup();
    await ready(result);
    let changing!: Promise<void>;
    act(() => {
      changing = result.current.switchMode('practice');
    });
    await waitFor(() => expect(result.current.busyLabel).toBe('Changing help…'));
    await act(async () => {
      finishMode();
      await changing;
    });

    let finishSubmit!: () => void;
    mocks.submit.mockImplementationOnce(() => new Promise((resolve) => {
      finishSubmit = () => resolve(ok({ programId: 'program-1', sessions: [summary] }));
    }));
    act(() => result.current.setText('A response.'));
    let submitting!: Promise<void>;
    act(() => {
      submitting = result.current.doSubmit('A response.');
    });
    await waitFor(() => expect(result.current.busyLabel).toBe('Submitting response…'));
    await act(async () => {
      finishSubmit();
      await submitting;
    });
  });
});
