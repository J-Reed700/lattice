import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { isConflict } from '@/lib/api';

import { saveStatusOf, useAutosave } from './useAutosave';

vi.mock('@/lib/api', () => ({ isConflict: vi.fn(() => false) }));

const DELAY = 300;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

interface Props {
  value: string;
  revision?: number;
  enabled?: boolean;
}

function setup(save: (value: string, revision: number) => Promise<{ revision: number }>, initial: Props = { value: 'first' }) {
  return renderHook(
    ({ value, revision = 1, enabled = true }: Props) => useAutosave({ value, revision, save, delayMs: DELAY, enabled }),
    { initialProps: initial },
  );
}

describe('useAutosave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(isConflict).mockReset();
    vi.mocked(isConflict).mockReturnValue(false);
  });
  afterEach(() => vi.useRealTimers());

  it('saves nothing on mount and waits for the pause before saving a change', async () => {
    const save = vi.fn().mockResolvedValue({ revision: 2 });
    const { result, rerender } = setup(save);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 2);
    });
    expect(save).not.toHaveBeenCalled();
    expect(result.current.state).toBe('idle');

    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY - 1);
    });
    expect(save).not.toHaveBeenCalled();
    rerender({ value: 'third' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY - 1);
    });
    expect(save).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith('third', 1);
    expect(result.current.state).toBe('saved');
    expect(result.current.error).toBeNull();
  });

  it('passes the revision and moves on to the revision each save returns', async () => {
    const save = vi.fn().mockResolvedValueOnce({ revision: 2 }).mockResolvedValueOnce({ revision: 3 });
    const { rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    rerender({ value: 'third' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(save.mock.calls).toEqual([
      ['second', 1],
      ['third', 2],
    ]);
  });

  it('reports a conflict when isConflict says so, and not for a message that merely sounds like one', async () => {
    const stale = Object.assign(new Error('Revision changed; reload and retry.'), { details: { code: 'CONFLICT' } });
    vi.mocked(isConflict).mockImplementation((error) => error === stale);
    const save = vi.fn().mockRejectedValueOnce(stale).mockRejectedValueOnce(new Error('The revision changed on disk'));
    const { result, rerender } = setup(save);

    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(isConflict).toHaveBeenCalledWith(stale);
    expect(result.current.state).toBe('conflict');
    expect(result.current.error).toBe('Revision changed; reload and retry.');

    rerender({ value: 'third' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(result.current.state).toBe('error');
    expect(result.current.error).toBe('The revision changed on disk');
  });

  it('does not repeat a failed save by itself, and flush tries it again', async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error('Connection interrupted')).mockResolvedValueOnce({ revision: 2 });
    const { result, rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 5);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(result.current.state).toBe('error');

    await act(async () => {
      await result.current.flush();
    });
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]).toEqual(save.mock.calls[0]);
    expect(result.current.state).toBe('saved');
    expect(result.current.error).toBeNull();
  });

  it('saves again once a newer revision arrives after a conflict', async () => {
    vi.mocked(isConflict).mockReturnValue(true);
    const save = vi.fn().mockRejectedValueOnce(new Error('stale')).mockResolvedValueOnce({ revision: 6 });
    const { result, rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(result.current.state).toBe('conflict');

    rerender({ value: 'second', revision: 5 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]).toEqual(['second', 5]);
    expect(result.current.state).toBe('saved');
  });

  it('saves again against a newer revision that arrived while a save was in flight and then conflicted', async () => {
    vi.mocked(isConflict).mockReturnValue(true);
    const first = deferred<{ revision: number }>();
    const save = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce({ revision: 8 });
    const { result, rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(result.current.state).toBe('saving');

    rerender({ value: 'second', revision: 7 });
    await act(async () => {
      first.reject(new Error('stale'));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]).toEqual(['second', 7]);
    expect(result.current.state).toBe('saved');
  });

  it('stays in conflict, with no second save, when no newer revision arrived', async () => {
    vi.mocked(isConflict).mockReturnValue(true);
    const save = vi.fn().mockRejectedValue(new Error('stale'));
    const { result, rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 3);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(result.current.state).toBe('conflict');
  });

  it('flush saves at once without waiting for the pause, and skips an unchanged value', async () => {
    const save = vi.fn().mockResolvedValue({ revision: 2 });
    const { result, rerender } = setup(save);
    await act(async () => {
      await result.current.flush();
    });
    expect(save).not.toHaveBeenCalled();

    rerender({ value: 'second' });
    await act(async () => {
      await result.current.flush();
    });
    expect(save).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 2);
    });
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('keeps saves in order: a change made while one is in flight waits for it', async () => {
    const first = deferred<{ revision: number }>();
    const save = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce({ revision: 3 });
    const { result, rerender } = setup(save);
    rerender({ value: 'second' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(result.current.state).toBe('saving');

    rerender({ value: 'third' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(save).toHaveBeenCalledTimes(1);

    await act(async () => {
      first.resolve({ revision: 2 });
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(save.mock.calls).toEqual([
      ['second', 1],
      ['third', 2],
    ]);
  });

  it('saves nothing while disabled, and takes the value it is enabled with as already saved', async () => {
    const save = vi.fn().mockResolvedValue({ revision: 2 });
    const { rerender } = setup(save, { value: 'first', enabled: false });
    rerender({ value: 'second', enabled: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 3);
    });
    expect(save).not.toHaveBeenCalled();

    rerender({ value: 'second', enabled: true });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY * 3);
    });
    expect(save).not.toHaveBeenCalled();

    rerender({ value: 'third', enabled: true });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(DELAY);
    });
    expect(save).toHaveBeenCalledWith('third', 1);
  });
});

describe('saveStatusOf', () => {
  it('reads saving as saving, a pending or failed save as unsaved, and the rest as saved', () => {
    expect(saveStatusOf('saving', true)).toBe('saving');
    expect(saveStatusOf('idle', true)).toBe('unsaved');
    expect(saveStatusOf('error', false)).toBe('unsaved');
    expect(saveStatusOf('conflict', false)).toBe('unsaved');
    expect(saveStatusOf('saved', false)).toBe('saved');
    expect(saveStatusOf('idle', false)).toBe('saved');
  });
});
