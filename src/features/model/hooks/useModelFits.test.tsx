import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import type { FitVerdictDto, ModelFileRefDto } from '@/lib/bindings';
import { createTestApi } from '@/tests/apiClient';

import { useModelFits } from './useModelFits';

const GB = 1_073_741_824;
const verdict: FitVerdictDto = {
  kind: 'fits',
  needsBytes: 11.2 * GB,
  budgetBytes: 15 * GB,
  device: { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated', totalBytes: 16 * GB },
};
const refs: ModelFileRefDto[] = [
  { repo: 'pub/alpha-GGUF', file: 'alpha-Q4_K_M.gguf', revision: null },
  { repo: 'pub/beta-GGUF', file: 'beta-Q4_K_M.gguf', revision: null },
];

function setup(readModelFits: ReturnType<typeof vi.fn>) {
  const api = createTestApi({ readModelFits: readModelFits as never });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ApiProvider client={api}><QueryClientProvider client={client}>{children}</QueryClientProvider></ApiProvider>
  );
  const mount = () => renderHook(() => useModelFits(refs), { wrapper });
  return { mount };
}

describe('useModelFits when the request itself fails', () => {
  it.each([
    ['rejects', () => Promise.reject(new Error('ipc dropped'))],
    ['answers not ok', () => Promise.resolve({ ok: false, error: 'ipc dropped' })],
  ])('does not settle on unknown when the call %s, and asks again later', async (_name, fail) => {
    const readModelFits = vi.fn().mockImplementationOnce(fail).mockResolvedValue({ ok: true, data: [verdict, null] });
    const { mount } = setup(readModelFits);

    const first = mount();
    await waitFor(() => expect(readModelFits).toHaveBeenCalledTimes(1));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 25)); });
    expect(first.result.current).not.toContain(null);
    first.unmount();

    const second = mount();
    await waitFor(() => expect(second.result.current).toEqual([verdict, null]));
    expect(readModelFits.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(readModelFits.mock.calls.at(-1)?.[0]).toEqual([
      expect.objectContaining(refs[0]),
      expect.objectContaining(refs[1]),
    ]);
  });

  it('keeps a null inside a successful answer as unknown, and does not ask for it again', async () => {
    const readModelFits = vi.fn().mockResolvedValue({ ok: true, data: [verdict, null] });
    const { mount } = setup(readModelFits);

    const first = mount();
    await waitFor(() => expect(first.result.current).toEqual([verdict, null]));
    first.unmount();

    const second = mount();
    expect(second.result.current).toEqual([verdict, null]);
    expect(readModelFits).toHaveBeenCalledTimes(1);
  });
});
