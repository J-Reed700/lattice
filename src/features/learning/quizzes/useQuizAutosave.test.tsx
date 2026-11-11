import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { formItem, quizForm } from '@/features/learning/quizzes/quizTestData';
import { useQuizAutosave } from '@/features/learning/quizzes/useQuizAutosave';
import type { LearningAssessmentFormDto } from '@/lib/bindings';

const h = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock('@/features/learning/quizzes/useQuizzes', () => ({
  newOperationId: () => crypto.randomUUID(),
  useSaveQuizResponse: () => ({ mutateAsync: h.save, isPending: false }),
}));

const first = formItem('short_answer', { id: 'first' });
const second = formItem('short_answer', { id: 'second' });
const form = quizForm({ items: [first, second] });
const saved = (revision: number) => quizForm({ items: form.items, revision });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function setup(reload = vi.fn(async () => form)) {
  return renderHook(() => useQuizAutosave(form, form.programId, reload));
}

describe('quiz autosave', () => {
  beforeEach(() => { vi.useFakeTimers(); h.save.mockReset(); });
  afterEach(() => vi.useRealTimers());

  it('saves an edit even when flushed before React renders the edit', async () => {
    h.save.mockResolvedValue(saved(2));
    const { result } = setup();
    await act(async () => {
      result.current.update(first.id, { textResponse: 'Binding limits the rate.' });
      expect(await result.current.flush()).toBe(true);
    });
    expect(h.save).toHaveBeenCalledWith(expect.objectContaining({ response: expect.objectContaining({ text: 'Binding limits the rate.' }) }));
    expect(result.current.state).toBe('saved');
  });

  it('retries the original request before saving an answer edited after a failure', async () => {
    h.save.mockRejectedValueOnce(new Error('connection lost')).mockResolvedValueOnce(saved(2)).mockResolvedValueOnce(saved(3));
    const { result } = setup();
    act(() => result.current.update(first.id, { textResponse: 'First draft' }));
    await act(async () => { expect(await result.current.flush()).toBe(false); });
    act(() => result.current.update(first.id, { textResponse: 'Revised answer' }));
    await act(async () => { expect(await result.current.retry()).toBe(true); });

    expect(h.save).toHaveBeenCalledTimes(3);
    expect(h.save.mock.calls[1][0]).toEqual(h.save.mock.calls[0][0]);
    expect(h.save.mock.calls[2][0]).toMatchObject({ expectedRevision: 2, response: { text: 'Revised answer' } });
    expect(h.save.mock.calls[2][0].operationId).not.toBe(h.save.mock.calls[0][0].operationId);
    expect(result.current.state).toBe('saved');
  });

  it('resolves an uncertain write even when the answer was reverted', async () => {
    h.save.mockRejectedValueOnce(new Error('response lost')).mockResolvedValueOnce(saved(2)).mockResolvedValueOnce(saved(3));
    const { result } = setup();
    act(() => result.current.update(first.id, { textResponse: 'Temporary answer' }));
    await act(async () => { expect(await result.current.flush()).toBe(false); });
    act(() => result.current.update(first.id, { textResponse: '' }));
    await act(async () => { expect(await result.current.retry()).toBe(true); });
    expect(h.save).toHaveBeenCalledTimes(3);
    expect(h.save.mock.calls[2][0]).toMatchObject({ expectedRevision: 2, response: { text: '' } });
    expect(result.current.state).toBe('saved');
  });

  it('revisits earlier answers edited while a later answer is saving', async () => {
    const pending = deferred<LearningAssessmentFormDto>();
    h.save.mockResolvedValueOnce(saved(2)).mockReturnValueOnce(pending.promise).mockResolvedValueOnce(saved(4));
    const { result } = setup();
    let flight!: Promise<boolean>;
    await act(async () => {
      result.current.update(first.id, { textResponse: 'First answer' });
      result.current.update(second.id, { textResponse: 'Second answer' });
      flight = result.current.flush();
    });
    expect(h.save).toHaveBeenCalledTimes(2);
    await act(async () => {
      result.current.update(first.id, { textResponse: 'Edited first answer' });
      pending.resolve(saved(3));
      expect(await flight).toBe(true);
    });
    expect(h.save.mock.calls[2][0]).toMatchObject({ expectedRevision: 3, response: { itemId: first.id, text: 'Edited first answer' } });
    expect(result.current.state).toBe('saved');
  });

  it('serializes an explicit retry with a concurrent flush', async () => {
    const pending = deferred<LearningAssessmentFormDto>();
    h.save.mockRejectedValueOnce(new Error('response lost')).mockReturnValueOnce(pending.promise);
    const { result } = setup();
    act(() => result.current.update(first.id, { textResponse: 'Answer' }));
    await act(async () => { expect(await result.current.flush()).toBe(false); });
    await act(async () => {
      const retry = result.current.retry();
      const flush = result.current.flush();
      pending.resolve(saved(2));
      expect(await retry).toBe(true);
      expect(await flush).toBe(true);
    });
    expect(h.save).toHaveBeenCalledTimes(2);
  });

  it('waits for an explicit reload after a revision conflict and keeps local answers', async () => {
    h.save.mockResolvedValueOnce(saved(3)).mockResolvedValueOnce(saved(4));
    const reload = vi.fn(async () => saved(3));
    const { result } = setup(reload);
    act(() => result.current.update(first.id, { textResponse: 'My answer' }));
    await act(async () => { expect(await result.current.flush()).toBe(false); });
    expect(result.current.conflict).toBe(true);
    await act(async () => { expect(await result.current.flush()).toBe(false); });
    expect(h.save).toHaveBeenCalledTimes(1);
    await act(async () => { await result.current.reloadKeepingAnswers(); });
    expect(h.save.mock.calls[1][0]).toMatchObject({ expectedRevision: 3, response: { text: 'My answer' } });
    expect(result.current.conflict).toBe(false);
    expect(result.current.state).toBe('saved');
  });
});
