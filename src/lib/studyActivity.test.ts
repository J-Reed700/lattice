import { invoke } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { queryClient } from './queryClient';
import { beginStudyActivity, dismissStudyActivity, STUDY_ACTIVITY_KEY, type StudyActivity } from './studyActivity';

const { VaultAPI } = await vi.importActual<typeof import('./api')>('./api');
const activities = () => queryClient.getQueryData<StudyActivity[]>(STUDY_ACTIVITY_KEY) ?? [];

beforeEach(() => { vi.useFakeTimers(); queryClient.clear(); vi.mocked(invoke).mockReset(); });
afterEach(() => { queryClient.clear(); vi.useRealTimers(); });

describe('Study request activity', () => {
  it('tracks a real IPC promise beyond long waits without aborting, retrying, or recording its payload', async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const request: Parameters<typeof VaultAPI.requestLearningTutorResponse>[0] = { programId: 'biology', sessionId: 'session', expectedRevision: 1, requestKind: 'question', prompt: 'PRIVATE ANSWER', hintLevel: null, operationId: 'operation' };
    const result = VaultAPI.requestLearningTutorResponse(request);
    expect(activities()).toEqual([]);
    await vi.advanceTimersByTimeAsync(800);
    expect(activities()).toMatchObject([{ status: 'pending', title: 'Preparing tutor feedback', destination: { programId: 'biology', tab: 'workbench' } }]);
    await vi.advanceTimersByTimeAsync(45 * 60_000);
    expect(activities()[0].status).toBe('pending');
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(activities())).not.toContain('PRIVATE ANSWER');
    resolve({ result: 'PRIVATE RESULT' });
    expect(await result).toEqual({ ok: true, data: { result: 'PRIVATE RESULT' } });
    expect(activities()[0].status).toBe('completed');
    expect(JSON.stringify(activities())).not.toContain('PRIVATE RESULT');
  });

  it('retains the actual failure and never retries a model mutation automatically', async () => {
    let reject!: (error: unknown) => void;
    vi.mocked(invoke).mockImplementationOnce(() => new Promise((_done, fail) => { reject = fail; }));
    const result = VaultAPI.generateLearningCardDrafts({ programId: 'rust' } as Parameters<typeof VaultAPI.generateLearningCardDrafts>[0]);
    await vi.advanceTimersByTimeAsync(900);
    reject('The model connection closed.');
    expect(await result).toMatchObject({ ok: false, error: 'The model connection closed.' });
    expect(activities()).toMatchObject([{ status: 'failed', error: 'The model connection closed.' }]);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('keeps fast reads quiet, but gives any slow Study command a fallback', async () => {
    const fast = beginStudyActivity('learning', 'get_learning_program', { id: 'course' })!;
    fast.finish();
    await vi.advanceTimersByTimeAsync(5000);
    expect(activities()).toEqual([]);
    const future = beginStudyActivity('learning', 'get_learning_future_workspace', { id: 'course' })!;
    await vi.advanceTimersByTimeAsync(4000);
    expect(activities()).toMatchObject([{ status: 'pending', destination: { programId: 'course' } }]);
    future.finish();
  });

  it('does not duplicate existing progress or mistake a queued job for completed work', () => {
    for (const command of ['prepare_learning_lesson', 'generate_learning_program', 'repair_learning_outline', 'start_learning_generation_job', 'retry_learning_generation_job', 'start_learning_practical_run']) {
      expect(beginStudyActivity('learning', command)).toBeUndefined();
    }
    expect(beginStudyActivity('conversation', 'send_message')).toBeUndefined();
  });

  it('distinguishes saving diagnostic answers from asking the model to grade them', async () => {
    const save = beginStudyActivity('learning', 'submit_learning_diagnostic', { request: { programId: 'course', saveOnly: true } })!;
    await vi.advanceTimersByTimeAsync(800);
    expect(activities()).toEqual([]);
    await vi.advanceTimersByTimeAsync(3200);
    expect(activities()[0].title).toBe('Saving your starting-point answers');
    expect(activities()[0].detail).not.toContain('model');
    save.finish();
  });

  it('bounds finished receipts without ever evicting or dismissing active requests', async () => {
    const pending = beginStudyActivity('learning', 'start_learning_diagnostic')!;
    await vi.advanceTimersByTimeAsync(800);
    const id = activities()[0].id;
    for (let i = 0; i < 15; i++) {
      const finished = beginStudyActivity('learning', 'start_learning_diagnostic')!;
      await vi.advanceTimersByTimeAsync(800);
      finished.finish();
    }
    expect(activities()).toHaveLength(13);
    dismissStudyActivity(id);
    expect(activities().some(item => item.id === id)).toBe(true);
    dismissStudyActivity();
    expect(activities()).toMatchObject([{ id, status: 'pending' }]);
    pending.finish();
  });

  it('links a completed flashcard generation to its saved deck', async () => {
    const generation = beginStudyActivity('study', 'generate_study_deck')!;
    await vi.advanceTimersByTimeAsync(800);
    generation.finish(undefined, { id: 'deck-biology', cards: ['private'] });
    expect(activities()[0].destination).toMatchObject({ flashcards: true, deckId: 'deck-biology' });
    expect(JSON.stringify(activities())).not.toContain('private');
  });
});
