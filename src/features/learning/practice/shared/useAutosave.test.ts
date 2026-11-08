import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { isConflictError, useAutosave } from './useAutosave';

describe('isConflictError', () => {
  it('returns true when details.code is CONFLICT', () => {
    expect(isConflictError({ details: { code: 'CONFLICT' } })).toBe(true);
    expect(
      isConflictError({
        message: 'Conflict occurred',
        details: { code: 'CONFLICT', message: 'Version mismatch' },
      }),
    ).toBe(true);
  });

  it('returns false when details.code is absent or different', () => {
    expect(isConflictError(null)).toBe(false);
    expect(isConflictError(undefined)).toBe(false);
    expect(isConflictError(new Error('revision conflict occurred'))).toBe(false);
    expect(isConflictError({ details: { code: 'NETWORK_ERROR' } })).toBe(false);
  });
});

describe('useAutosave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('transitions idle -> saving -> saved on value change', async () => {
    let resolveSave: () => void = () => {};
    const onSave = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        }),
    );

    const { result, rerender } = renderHook(
      ({ value }) => useAutosave({ value, onSave, delayMs: 300 }),
      { initialProps: { value: 'initial text' } },
    );

    expect(result.current.status).toBe('idle');

    // Update value to trigger autosave debounce
    rerender({ value: 'updated text' });

    // Status is still idle before debounce completes
    expect(result.current.status).toBe('idle');

    // Advance timer past debounce delay
    act(() => {
      vi.advanceTimersByTime(300);
    });

    expect(result.current.status).toBe('saving');
    expect(onSave).toHaveBeenCalledWith('updated text');

    // Complete the in-flight save promise
    await act(async () => {
      resolveSave();
    });

    expect(result.current.status).toBe('saved');
    expect(result.current.conflict).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('transitions to conflict when error has details.code === CONFLICT', async () => {
    const conflictError = {
      message: 'Revision conflict',
      details: { code: 'CONFLICT' },
    };
    const onSave = vi.fn().mockRejectedValue(conflictError);

    const { result, rerender } = renderHook(
      ({ value }) => useAutosave({ value, onSave, delayMs: 300 }),
      { initialProps: { value: 'initial' } },
    );

    rerender({ value: 'changed' });

    await act(async () => {
      vi.advanceTimersByTime(300);
    });

    expect(result.current.status).toBe('conflict');
    expect(result.current.conflict).toBe(true);
  });

  it('transitions to error when a non-conflict error occurs', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('Network error'));

    const { result, rerender } = renderHook(
      ({ value }) => useAutosave({ value, onSave, delayMs: 300 }),
      { initialProps: { value: 'initial' } },
    );

    rerender({ value: 'changed' });

    await act(async () => {
      vi.advanceTimersByTime(300);
    });

    expect(result.current.status).toBe('error');
    expect(result.current.conflict).toBe(false);
    expect(result.current.error).toBe('Network error');
  });

  it('flushes immediately on demand without waiting for debounce', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);

    const { result, rerender } = renderHook(
      ({ value }) => useAutosave({ value, onSave, delayMs: 500 }),
      { initialProps: { value: 'first' } },
    );

    rerender({ value: 'second' });

    // Flush immediately without waiting 500ms
    await act(async () => {
      const saved = await result.current.flush();
      expect(saved).toBe(true);
    });

    expect(onSave).toHaveBeenCalledWith('second');
    expect(result.current.status).toBe('saved');
  });
});
