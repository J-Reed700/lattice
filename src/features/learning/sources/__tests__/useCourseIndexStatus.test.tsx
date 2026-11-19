import type { ReactNode } from 'react';

import { render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import type { ApiClient } from '@/lib/api';

import { useCourseIndexOpen } from '../useCourseIndexStatus';

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function Harness({ programId }: { programId: string | null }) {
  useCourseIndexOpen(programId);
  return null;
}

describe('useCourseIndexOpen', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('re-issues courseIndexClose when slow open(course-a) settles after unmount', async () => {
    const openADeferred = deferred<{ ok: true; data: null }>();
    const courseIndexOpen = vi.fn(() => openADeferred.promise);
    const courseIndexClose = vi.fn(() => Promise.resolve({ ok: true as const, data: null }));

    const api = {
      courseIndexOpen,
      courseIndexClose,
    } as unknown as ApiClient;

    const wrapper = ({ children }: { children: ReactNode }) => (
      <ApiProvider client={api}>{children}</ApiProvider>
    );

    const { unmount } = render(<Harness programId="course-a" />, { wrapper });
    expect(courseIndexOpen).toHaveBeenCalledExactlyOnceWith('course-a');

    unmount();
    expect(courseIndexClose).toHaveBeenCalledExactlyOnceWith('course-a');

    // Late resolution of open('course-a') must reconcile by closing 'course-a' again.
    openADeferred.resolve({ ok: true, data: null });

    await vi.waitFor(() => {
      expect(courseIndexClose).toHaveBeenCalledTimes(2);
    });
    expect(courseIndexClose).toHaveBeenLastCalledWith('course-a');
  });

  it('re-issues courseIndexOpen(course-b) when slow open(course-a) settles after switching', async () => {
    const openADeferred = deferred<{ ok: true; data: null }>();
    const courseIndexOpen = vi.fn((programId: string) => {
      if (programId === 'course-a' && courseIndexOpen.mock.calls.length === 1) {
        return openADeferred.promise;
      }
      return Promise.resolve({ ok: true as const, data: null });
    });
    const courseIndexClose = vi.fn(() => Promise.resolve({ ok: true as const, data: null }));

    const api = {
      courseIndexOpen,
      courseIndexClose,
    } as unknown as ApiClient;

    const wrapper = ({ children }: { children: ReactNode }) => (
      <ApiProvider client={api}>{children}</ApiProvider>
    );

    const { rerender, unmount } = render(<Harness programId="course-a" />, { wrapper });
    expect(courseIndexOpen).toHaveBeenCalledExactlyOnceWith('course-a');

    rerender(<Harness programId="course-b" />);
    expect(courseIndexClose).toHaveBeenCalledWith('course-a');
    expect(courseIndexOpen).toHaveBeenLastCalledWith('course-b');

    // Late resolution of the stale open('course-a') must re-assert open('course-b').
    openADeferred.resolve({ ok: true, data: null });

    await vi.waitFor(() => {
      expect(courseIndexOpen).toHaveBeenCalledTimes(3);
    });
    expect(courseIndexOpen.mock.calls).toEqual([
      ['course-a'],
      ['course-b'],
      ['course-b'],
    ]);

    unmount();
    expect(courseIndexClose).toHaveBeenLastCalledWith('course-b');
  });
});
