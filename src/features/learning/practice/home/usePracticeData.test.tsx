import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';
import { queryClient } from '@/lib/queryClient';
import { apiCall } from '@/shared/ipc/transport';

import { usePracticeFit } from './usePracticeData';
import { sessionApi } from '../session/sessionApi';

const api = vi.hoisted(() => ({ getFit: vi.fn(), checkFit: vi.fn() }));
vi.mock('./practiceHomeApi', () => ({ practiceHomeApi: api }));
vi.mock('@/shared/ipc/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/shared/ipc/transport')>()),
  apiCall: vi.fn().mockResolvedValue({ ok: true, data: { sessionId: 's-1' } }),
}));

const fit = (...fits: Wire.KindFit[]): Wire.PracticeFitDto => ({
  kinds: fits.map((state, at) => ({ kind: `kind-${at}`, fit: state, reason: null })),
});

function run() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(() => usePracticeFit('course-1'), { wrapper });
}

beforeEach(() => {
  api.getFit.mockReset();
  api.checkFit.mockReset();
});

describe('usePracticeFit', () => {
  it('asks for the check while any kind is checking', async () => {
    api.getFit.mockResolvedValue(fit('fits', 'checking'));
    api.checkFit.mockResolvedValue(fit('fits', 'does_not_fit'));
    const { result } = run();
    await waitFor(() => expect(result.current.fit).toEqual(fit('fits', 'does_not_fit')));
    expect(api.checkFit).toHaveBeenCalledWith('course-1');
  });

  it('does not ask when every kind is decided', async () => {
    api.getFit.mockResolvedValue(fit('fits', 'does_not_fit'));
    const { result } = run();
    await waitFor(() => expect(result.current.fit).toEqual(fit('fits', 'does_not_fit')));
    expect(api.checkFit).not.toHaveBeenCalled();
  });

  it('invalidates practice home queries when sessionApi mutates a coding session', async () => {
    vi.mocked(apiCall).mockResolvedValue({ ok: true, data: { sessionId: 's-1' } as never });
    const spy = vi.spyOn(queryClient, 'invalidateQueries');
    await sessionApi.finish({
      operationId: 'op-1',
      programId: 'course-1',
      sessionId: 's-1',
      piece: { kind: 'program', files: [] } as unknown as Parameters<typeof sessionApi.finish>[0]['piece'],
    });
    expect(spy).toHaveBeenCalledWith({ queryKey: ['learning-exercise-sessions', 'course-1'] });
    expect(spy).toHaveBeenCalledWith({ queryKey: ['learning-practice-due', 'course-1'] });
    expect(spy).toHaveBeenCalledWith({ queryKey: ['learning-practice-fit', 'course-1'] });
    spy.mockRestore();
  });
});
