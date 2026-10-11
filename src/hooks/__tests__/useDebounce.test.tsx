import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { useDebounce } from '../useDebounce';

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

it('returns the initial value immediately and waits exactly the requested delay', () => {
  const { result, rerender } = renderHook(({ value }) => useDebounce(value, 250), { initialProps: { value: 'old' } });
  expect(result.current).toBe('old');
  rerender({ value: 'new' });
  act(() => { vi.advanceTimersByTime(249); });
  expect(result.current).toBe('old');
  act(() => { vi.advanceTimersByTime(1); });
  expect(result.current).toBe('new');
});

it('collapses rapid changes including clearing the search', () => {
  const { result, rerender } = renderHook(({ value }) => useDebounce(value, 100), { initialProps: { value: 'initial' } });
  for (const value of ['a', 'ab', 'abc', '']) {
    rerender({ value });
    act(() => { vi.advanceTimersByTime(99); });
    expect(result.current).toBe('initial');
  }
  act(() => { vi.advanceTimersByTime(1); });
  expect(result.current).toBe('');
});

it('restarts the timer when the delay changes', () => {
  const { result, rerender } = renderHook(({ value, delay }) => useDebounce(value, delay), { initialProps: { value: 0, delay: 100 } });
  rerender({ value: 1, delay: 100 });
  act(() => { vi.advanceTimersByTime(50); });
  rerender({ value: 1, delay: 200 });
  act(() => { vi.advanceTimersByTime(199); });
  expect(result.current).toBe(0);
  act(() => { vi.advanceTimersByTime(1); });
  expect(result.current).toBe(1);
});

it('preserves object identity and supports zero delay', () => {
  const initial = { query: 'old' };
  const next = { query: 'new' };
  const { result, rerender } = renderHook(({ value }) => useDebounce(value, 0), { initialProps: { value: initial } });
  rerender({ value: next });
  expect(result.current).toBe(initial);
  act(() => { vi.runOnlyPendingTimers(); });
  expect(result.current).toBe(next);
});

it('removes outstanding timers on unmount', () => {
  const { rerender, unmount } = renderHook(({ value }) => useDebounce(value, 500), { initialProps: { value: 'a' } });
  rerender({ value: 'b' });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});
