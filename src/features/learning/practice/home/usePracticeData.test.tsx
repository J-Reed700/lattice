import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { usePracticeFit } from './usePracticeData';

const api = vi.hoisted(() => ({ getFit: vi.fn(), checkFit: vi.fn() }));
vi.mock('./practiceHomeApi', () => ({ practiceHomeApi: api }));

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
});
