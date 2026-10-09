import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider, QueryObserver } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { learningAssessmentWorkspaceKey, learningCanvasKey, learningMemoryKey, learningPracticalWorkspaceKey, learningPracticeWorkspaceKey, learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import { useApplyLearningPackImport } from '@/features/learning/portability/useLearningPortability';
import type { ApplyLearningPackImportRequestDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';
import { deferred } from '@/tests/deferred';

const api = vi.hoisted(() => ({ apply: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { applyLearningPackImport: api.apply } }));
beforeEach(() => {
  api.apply.mockReset().mockResolvedValue({ ok: true, data: { programId: 'course', exports: [] } });
});

const workspaceKeys = [learningMemoryKey, learningCanvasKey, learningRecallKey, learningPracticeWorkspaceKey, learningPracticalWorkspaceKey, learningAssessmentWorkspaceKey];
const request = { operationId: 'operation', previewId: 'preview', expectedRootSha256: 'hash' } satisfies ApplyLearningPackImportRequestDto;
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, ...renderHook(() => useApplyLearningPackImport('course'), { wrapper }) };
}

it.each([true, false])('refreshes all imported workspaces (mounted: %s) without resetting another course', async (mounted) => {
  const { client, result, unmount } = setup();
  const reads = workspaceKeys.map(key => {
    const queryKey = key('course');
    client.setQueryData(queryKey, { programId: 'course', value: 'before' });
    client.setQueryData(key('other'), { programId: 'other', value: 'untouched' });
    const queryFn = vi.fn().mockResolvedValue({ programId: 'course', value: 'after' });
    const observer = new QueryObserver(client, { queryKey, queryFn });
    return { queryKey, queryFn, unsubscribe: mounted ? observer.subscribe(() => {}) : () => {} };
  });
  await act(async () => { await result.current.mutateAsync(request); });
  for (const { queryKey, queryFn, unsubscribe } of reads) {
    expect(client.getQueryData(queryKey)).toEqual(mounted ? { programId: 'course', value: 'after' } : undefined);
    expect(queryFn).toHaveBeenCalledTimes(mounted ? 1 : 0);
    unsubscribe();
  }
  for (const key of workspaceKeys) expect(client.getQueryData(key('other'))).toEqual({ programId: 'other', value: 'untouched' });
  unmount(); client.clear();
});

it.each(['learning-practice-session', 'learning-assessment-form'])('drops deleted %s snapshots and prevents an older in-flight read overwriting the import', async (kind) => {
  const { client, result, unmount } = setup();
  const late = deferred<{ programId: string; value: string }>();
  const key = [kind, 'same-id'];
  client.setQueryData(key, { programId: 'course', value: 'before' });
  client.setQueryData([kind, 'deleted-id'], { programId: 'course', value: 'deleted' });
  client.setQueryData([kind, 'other-id'], { programId: 'other', value: 'untouched' });
  const queryFn = vi.fn().mockReturnValueOnce(late.promise).mockResolvedValue({ programId: 'course', value: 'after' });
  const observer = new QueryObserver(client, { queryKey: key, queryFn });
  const unsubscribe = observer.subscribe(() => {});
  void observer.refetch();
  await waitFor(() => expect(queryFn).toHaveBeenCalledTimes(1));
  await act(async () => { await result.current.mutateAsync(request); });
  await act(async () => { late.resolve({ programId: 'course', value: 'late old data' }); });
  expect(queryFn).toHaveBeenCalledTimes(2);
  expect(client.getQueryData(key)).toEqual({ programId: 'course', value: 'after' });
  expect(client.getQueryData([kind, 'deleted-id'])).toBeUndefined();
  expect(client.getQueryData([kind, 'other-id'])).toEqual({ programId: 'other', value: 'untouched' });
  unsubscribe(); unmount(); client.clear();
});

it('targets the imported copy, leaving the original program workspace intact', async () => {
  const { client, result, unmount } = setup();
  api.apply.mockResolvedValue({ ok: true, data: { programId: 'copy', exports: [] } });
  client.setQueryData(learningMemoryKey('course'), 'original');
  client.setQueryData(learningMemoryKey('copy'), 'stale copy');
  await act(async () => { await result.current.mutateAsync(request); });
  expect(client.getQueryData(learningMemoryKey('copy'))).toBeUndefined();
  expect(client.getQueryData(learningMemoryKey('course'))).toBe('original');
  expect(client.getQueryData(['learning-portability', 'copy'])).toEqual({ programId: 'copy', exports: [] });
  unmount(); client.clear();
});

it('waits for pending saves and refuses to import when a save fails', async () => {
  const { client, result, unmount } = setup();
  const save = deferred<boolean>();
  const unregister = registerPendingSave(() => save.promise);
  try {
    let importing!: Promise<unknown>;
    await act(async () => { importing = result.current.mutateAsync(request).catch(error => error); });
    expect(api.apply).not.toHaveBeenCalled();
    await act(async () => { save.resolve(false); await importing; });
    expect(result.current.error?.message).toContain('could not be saved before importing');
    expect(api.apply).not.toHaveBeenCalled();
  } finally {
    unregister(); unmount(); client.clear();
  }
});
