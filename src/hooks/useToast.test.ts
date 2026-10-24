import { act, renderHook } from '@testing-library/react';
import { beforeEach, expect, it } from 'vitest';

import { useToast } from './useToast';
import { useToastStore } from '../stores/toastStore';


beforeEach(() => useToastStore.setState({ toasts: [] }));

it('adds each kind of toast and dismisses one or all', () => {
  const { result } = renderHook(() => useToast());
  let first = '';
  act(() => {
    first = result.current.toast.success('Saved');
    result.current.toast.error('Failed', { message: 'disk full' });
    result.current.toast.warning('Careful');
    result.current.toast.info('Heads up');
  });
  expect(useToastStore.getState().toasts.map((t) => [t.type, t.title])).toEqual(
    expect.arrayContaining([['success', 'Saved'], ['error', 'Failed'], ['warning', 'Careful'], ['info', 'Heads up']]),
  );
  act(() => result.current.toast.dismiss(first));
  expect(useToastStore.getState().toasts.some((t) => t.id === first)).toBe(false);
  act(() => result.current.toast.dismissAll());
  expect(useToastStore.getState().toasts).toEqual([]);
});
