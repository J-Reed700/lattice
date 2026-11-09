import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { learningCoursePlanKey } from '@/features/learning/api/learningQueryKeys';
import { emptyPlan, entry, ok, planView, topics } from '@/features/learning/curriculum/coursePlan/coursePlanFixtures';
import {
  useChangeCoursePlanEntries,
  useCoursePlan,
  useEditCoursePlanEntry,
  usePlanCourse,
  useUndoCoursePlanEntry,
} from '@/features/learning/curriculum/coursePlan/useCoursePlan';

const api = vi.hoisted(() => ({ get: vi.fn(), plan: vi.fn(), edit: vi.fn(), change: vi.fn(), undo: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningCoursePlan: api.get,
  planLearningCourse: api.plan,
  editLearningCoursePlanEntry: api.edit,
  changeLearningCoursePlanEntries: api.change,
  undoLearningCoursePlanEntry: api.undo,
} }));

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, wrapper };
}

describe('course plan hooks', () => {
  beforeEach(() => vi.resetAllMocks());
  afterEach(() => vi.useRealTimers());

  it('reads the plan of the course', async () => {
    const view = planView([entry('lesson-1')]);
    api.get.mockResolvedValue(ok(view));
    const { wrapper } = setup();
    const { result } = renderHook(() => useCoursePlan('program-1'), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual(view));
    expect(api.get).toHaveBeenCalledWith('program-1');
  });

  it('polls while the plan is being made and stops once it is planned', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    api.get.mockResolvedValueOnce(ok(emptyPlan('planning'))).mockResolvedValue(ok(planView([entry('lesson-1')])));
    const { wrapper } = setup();
    const { result } = renderHook(() => useCoursePlan('program-1'), { wrapper });
    await waitFor(() => expect(result.current.data?.state).toBe('planning'));
    await act(async () => { await vi.advanceTimersByTimeAsync(2500); });
    await waitFor(() => expect(result.current.data?.state).toBe('planned'));
    const calls = api.get.mock.calls.length;
    expect(calls).toBeGreaterThanOrEqual(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(api.get).toHaveBeenCalledTimes(calls);
  });

  it('asks for the plan and writes the returned view into the cache', async () => {
    const view = emptyPlan('planning');
    api.plan.mockResolvedValue(ok(view));
    const { client, wrapper } = setup();
    const { result } = renderHook(() => usePlanCourse(), { wrapper });
    await act(async () => { await result.current.mutateAsync({ programId: 'program-1' }); });
    expect(api.plan).toHaveBeenCalledWith({ programId: 'program-1' });
    expect(client.getQueryData(learningCoursePlanKey('program-1'))).toEqual(view);
  });

  it('sends an edit, a change and an undo as the contract says and caches each view', async () => {
    const edited = planView([entry('lesson-1', { entryRevision: 2 })]);
    const changed = planView([entry('lesson-1', { entryRevision: 3 })]);
    const undone = planView([entry('lesson-1', { entryRevision: 4 })]);
    api.edit.mockResolvedValue(ok(edited));
    api.change.mockResolvedValue(ok(changed));
    api.undo.mockResolvedValue(ok(undone));
    const { client, wrapper } = setup();
    const { result } = renderHook(() => ({ edit: useEditCoursePlanEntry(), change: useChangeCoursePlanEntries(), undo: useUndoCoursePlanEntry() }), { wrapper });
    const editRequest = { programId: 'program-1', outlineRevision: 4, expectedEntryRevision: 1, topics: topics('lesson-1') };
    await act(async () => { await result.current.edit.mutateAsync(editRequest); });
    expect(api.edit).toHaveBeenCalledWith(editRequest);
    expect(client.getQueryData(learningCoursePlanKey('program-1'))).toEqual(edited);
    const changeRequest = { programId: 'program-1', outlineRevision: 4, lessons: [{ lessonId: 'lesson-1', expectedEntryRevision: 2 }], instruction: 'Simpler' };
    await act(async () => { await result.current.change.mutateAsync(changeRequest); });
    expect(api.change).toHaveBeenCalledWith(changeRequest);
    expect(client.getQueryData(learningCoursePlanKey('program-1'))).toEqual(changed);
    const undoRequest = { programId: 'program-1', outlineRevision: 4, lessonId: 'lesson-1', expectedEntryRevision: 3 };
    await act(async () => { await result.current.undo.mutateAsync(undoRequest); });
    expect(api.undo).toHaveBeenCalledWith(undoRequest);
    expect(client.getQueryData(learningCoursePlanKey('program-1'))).toEqual(undone);
  });

  it('refetches the plan when a change is refused as stale', async () => {
    api.get.mockResolvedValue(ok(planView([entry('lesson-1', { entryRevision: 5 })])));
    api.edit.mockResolvedValue({ ok: false, error: 'The topics changed since you opened them.' });
    const { wrapper } = setup();
    const { result } = renderHook(() => ({ plan: useCoursePlan('program-1'), edit: useEditCoursePlanEntry() }), { wrapper });
    await waitFor(() => expect(result.current.plan.data).toBeDefined());
    await act(async () => {
      await result.current.edit.mutateAsync({ programId: 'program-1', outlineRevision: 4, expectedEntryRevision: 1, topics: topics('lesson-1') }).catch(() => undefined);
    });
    await waitFor(() => expect(api.get).toHaveBeenCalledTimes(2));
    expect(result.current.edit.error?.message).toContain('changed since');
  });
});
