import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { useReviewSelection } from '@/features/learning/lessons/review/useReviewSelection';

const KEYS = ['a', 'b', 'c', 'd', 'e'];

const selectedOf = (result: { current: ReturnType<typeof useReviewSelection> }) => [...result.current.selected].sort();

describe('useReviewSelection', () => {
  it('toggles one key on and off', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.toggle('b'));
    expect(selectedOf(result)).toEqual(['b']);
    expect(result.current.isSelected('b')).toBe(true);
    act(() => result.current.toggle('b'));
    expect(selectedOf(result)).toEqual([]);
  });

  it('selects the range from the last click with shift, in display order, either way', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.toggle('b'));
    act(() => result.current.toggle('d', { shiftKey: true }));
    expect(selectedOf(result)).toEqual(['b', 'c', 'd']);
    act(() => result.current.clear());
    act(() => result.current.toggle('e'));
    act(() => result.current.toggle('c', { shiftKey: true }));
    expect(selectedOf(result)).toEqual(['c', 'd', 'e']);
  });

  it('unselects the range when the shift-clicked key was selected', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.setMany(KEYS, true));
    act(() => result.current.toggle('b'));
    act(() => result.current.toggle('d', { shiftKey: true }));
    expect(selectedOf(result)).toEqual(['a', 'e']);
  });

  it('toggles only the clicked key on a shift-click with no earlier click', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.toggle('c', { shiftKey: true }));
    expect(selectedOf(result)).toEqual(['c']);
  });

  it('selects and unselects a group at once', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.setMany(['a', 'c'], true));
    expect(selectedOf(result)).toEqual(['a', 'c']);
    act(() => result.current.setMany(['c', 'd'], true));
    expect(selectedOf(result)).toEqual(['a', 'c', 'd']);
    act(() => result.current.setMany(['a', 'c'], false));
    expect(selectedOf(result)).toEqual(['d']);
    act(() => result.current.clear());
    expect(selectedOf(result)).toEqual([]);
  });

  it('drops keys that are no longer listed, and does not bring them back when they return', () => {
    const { result, rerender } = renderHook(({ keys }) => useReviewSelection(keys), { initialProps: { keys: KEYS } });
    act(() => result.current.setMany(['a', 'b', 'c'], true));
    rerender({ keys: ['a', 'c', 'd'] });
    expect(selectedOf(result)).toEqual(['a', 'c']);
    expect(result.current.isSelected('b')).toBe(false);
    rerender({ keys: KEYS });
    expect(selectedOf(result)).toEqual(['a', 'c']);
  });

  it('ignores keys that are not listed', () => {
    const { result } = renderHook(() => useReviewSelection(KEYS));
    act(() => result.current.toggle('z'));
    act(() => result.current.setMany(['y', 'a'], true));
    expect(selectedOf(result)).toEqual(['a']);
  });
});
