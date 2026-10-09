import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { useLearningOutlineEvidence, useLearningLessonEvidence } from '@/features/learning/api/evidenceQueries';

const mocks = vi.hoisted(() => ({ outline: vi.fn(), lesson: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningOutlineEvidence: mocks.outline,
  getLearningLessonEvidence: mocks.lesson,
} }));

it('refetches outline and lesson evidence after a curriculum revision without displaying old evidence', async () => {
  mocks.outline.mockResolvedValueOnce({ ok: true, data: { contentSha256: 'old-outline' } });
  mocks.lesson.mockResolvedValueOnce({ ok: true, data: { contentSha256: 'old-lesson' } });
  mocks.outline.mockImplementationOnce(() => new Promise(() => {}));
  mocks.lesson.mockImplementationOnce(() => new Promise(() => {}));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const { result, rerender } = renderHook(({ revision }) => ({
    outline: useLearningOutlineEvidence('course', revision),
    lesson: useLearningLessonEvidence('course', 'lesson', revision),
  }), { initialProps: { revision: 1 }, wrapper });

  await waitFor(() => {
    expect(result.current.outline.data?.contentSha256).toBe('old-outline');
    expect(result.current.lesson.data?.contentSha256).toBe('old-lesson');
  });
  rerender({ revision: 2 });

  await waitFor(() => {
    expect(mocks.outline).toHaveBeenCalledTimes(2);
    expect(mocks.lesson).toHaveBeenCalledTimes(2);
  });
  expect(result.current.outline.data).toBeUndefined();
  expect(result.current.lesson.data).toBeUndefined();
});
