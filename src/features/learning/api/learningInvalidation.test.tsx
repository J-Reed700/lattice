import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { useLearningLessonEvidence, useLearningOutlineEvidence } from '@/features/learning/api/evidenceQueries';
import { learningLessonEvidenceKey, learningOutlineEvidenceKey, learningProgramKey, LEARNING_PROGRAMS_KEY } from '@/features/learning/api/learningQueryKeys';
import { useApplyLearningPackImport, useDeleteLearningSourceV2, useReimportLearningSource } from '@/features/learning/portability/useLearningPortability';
import { useAddLearningTextSource, useAdoptLearningSourceVersion, useRefreshLearningSource } from '@/features/learning/sources/useLearningSources';
import type { AddLearningTextSourceRequestDto, AdoptLearningSourceVersionRequestDto, ApplyLearningPackImportRequestDto, DeleteLearningSourceRequestDto, RefreshLearningSourceRequestDto, ReimportLearningSourceRequestDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ lesson: vi.fn(), outline: vi.fn(), source: vi.fn(), importPack: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonEvidence: api.lesson,
  getLearningOutlineEvidence: api.outline,
  addLearningTextSource: api.source,
  adoptLearningSourceVersion: api.source,
  refreshLearningSource: api.source,
  deleteLearningSource: api.source,
  reimportLearningSource: api.source,
  applyLearningPackImport: api.importPack,
} }));

beforeEach(() => {
  vi.resetAllMocks();
  api.source.mockResolvedValue({ ok: true, data: { programId: 'course', sources: [] } });
  api.importPack.mockResolvedValue({ ok: true, data: { programId: 'course', exports: [] } });
  api.lesson.mockResolvedValue({ ok: true, data: { sourcesCurrent: false } });
  api.outline.mockResolvedValue({ ok: true, data: { citations: [], contentSha256: 'updated' } });
});

function setup(enabled = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(learningLessonEvidenceKey('course', 'lesson', 1), { sourcesCurrent: true });
  client.setQueryData(learningOutlineEvidenceKey('course', 1), { citations: [], contentSha256: 'old' });
  client.setQueryData(learningLessonEvidenceKey('other', 'lesson', 1), { sourcesCurrent: true });
  client.setQueryData(learningProgramKey('course'), { summary: { revision: 1 } });
  client.setQueryData(LEARNING_PROGRAMS_KEY, []);
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const hook = renderHook(({ enabled }) => ({
    lesson: useLearningLessonEvidence('course', 'lesson', 1, enabled),
    outline: useLearningOutlineEvidence('course', 1, enabled),
    add: useAddLearningTextSource(), adopt: useAdoptLearningSourceVersion(), refresh: useRefreshLearningSource(),
    remove: useDeleteLearningSourceV2('course'), reimport: useReimportLearningSource('course'),
    importPack: useApplyLearningPackImport('course'),
  }), { initialProps: { enabled }, wrapper });
  return { ...hook, client };
}

it.each(['add', 'adopt', 'refresh', 'remove', 'reimport', 'importPack'] as const)('invalidates inactive citations after %s even when the program revision stays unchanged', async (mutation) => {
  const { result, rerender, client, unmount } = setup();
  await act(async () => {
    // These tests exercise mutation completion/cache ownership; IPC request
    // validation is covered by the API contract tests.
    const request = {} as AddLearningTextSourceRequestDto & AdoptLearningSourceVersionRequestDto & RefreshLearningSourceRequestDto & DeleteLearningSourceRequestDto & ReimportLearningSourceRequestDto & ApplyLearningPackImportRequestDto;
    await result.current[mutation].mutateAsync(request);
  });
  expect(client.getQueryState(learningLessonEvidenceKey('course', 'lesson', 1))?.isInvalidated).toBe(true);
  expect(client.getQueryState(learningOutlineEvidenceKey('course', 1))?.isInvalidated).toBe(true);
  expect(client.getQueryState(learningProgramKey('course'))?.isInvalidated).toBe(true);
  expect(client.getQueryState(learningLessonEvidenceKey('other', 'lesson', 1))?.isInvalidated).toBe(false);
  expect(api.lesson).not.toHaveBeenCalled();
  rerender({ enabled: true });
  await waitFor(() => expect(result.current.lesson.data?.sourcesCurrent).toBe(false));
  expect(result.current.outline.data?.contentSha256).toBe('updated');
  unmount();
  client.clear();
});

it('refreshes open evidence immediately after a source change', async () => {
  const { result, client, unmount } = setup(true);
  await act(async () => { await result.current.adopt.mutateAsync({} as AdoptLearningSourceVersionRequestDto); });
  await waitFor(() => expect(result.current.lesson.data?.sourcesCurrent).toBe(false));
  expect(api.lesson).toHaveBeenCalledTimes(1);
  expect(api.outline).toHaveBeenCalledTimes(1);
  unmount();
  client.clear();
});

it('invalidates the returned destination when an import creates a copy', async () => {
  const { result, client, unmount } = setup();
  api.importPack.mockResolvedValue({ ok: true, data: { programId: 'copy', exports: [] } });
  client.setQueryData(learningLessonEvidenceKey('copy', 'copied-lesson', 1), { sourcesCurrent: true });
  await act(async () => { await result.current.importPack.mutateAsync({} as ApplyLearningPackImportRequestDto); });
  expect(client.getQueryState(learningLessonEvidenceKey('copy', 'copied-lesson', 1))?.isInvalidated).toBe(true);
  expect(client.getQueryState(LEARNING_PROGRAMS_KEY)?.isInvalidated).toBe(true);
  unmount();
  client.clear();
});
