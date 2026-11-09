import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useRetry } from './useRetry';

describe('useRetry', () => {
  it('returns the result and is not busy afterwards', async () => {
    const action = vi.fn(async (value: number) => value * 2);
    const { result } = renderHook(() => useRetry(action));
    let value: number | undefined;
    await act(async () => {
      value = await result.current.run(4);
    });
    expect(value).toBe(8);
    expect(result.current.busy).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('keeps the error text, and retry runs the same arguments again', async () => {
    const action = vi
      .fn<(value: string) => Promise<string>>()
      .mockRejectedValueOnce(new Error('Lost the connection.'))
      .mockResolvedValueOnce('done');
    const { result } = renderHook(() => useRetry(action));
    await act(async () => {
      expect(await result.current.run('same')).toBeUndefined();
    });
    expect(result.current.error).toBe('Lost the connection.');
    await act(async () => {
      result.current.retry();
    });
    expect(action).toHaveBeenNthCalledWith(2, 'same');
    expect(result.current.error).toBeNull();
  });

  it('is busy while the action runs and ignores a second run', async () => {
    let finish: () => void = () => {};
    const action = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const { result } = renderHook(() => useRetry(action));
    let first: Promise<void | undefined> = Promise.resolve(undefined);
    act(() => {
      first = result.current.run();
    });
    expect(result.current.busy).toBe(true);
    await act(async () => {
      await result.current.run();
    });
    expect(action).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish();
      await first;
    });
    expect(result.current.busy).toBe(false);
  });
});
