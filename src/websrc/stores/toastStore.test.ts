import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

import { toast, useToastStore } from './toastStore';

describe('toastStore - Zustand Implementation', () => {
  beforeEach(() => {
    useToastStore.setState({
      toasts: [],
      config: {
        position: 'top-right',
        maxToasts: 5,
        defaultDuration: 4000,
        pauseOnHover: true,
      },
    });
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe('addToast', () => {
    it('adds a toast to the store', () => {
      const id = useToastStore.getState().addToast({
        type: 'success',
        title: 'Success!',
      });

      const toasts = useToastStore.getState().toasts;
      expect(toasts).toHaveLength(1);
      expect(toasts[0].id).toBe(id);
      expect(toasts[0].title).toBe('Success!');
    });

    it('generates unique IDs for each toast', () => {
      const id1 = useToastStore.getState().addToast({ type: 'info', title: 'First' });
      const id2 = useToastStore.getState().addToast({ type: 'info', title: 'Second' });

      expect(id1).not.toBe(id2);
    });

    it('adds toasts to the beginning of the array', () => {
      useToastStore.getState().addToast({ type: 'info', title: 'First' });
      useToastStore.getState().addToast({ type: 'info', title: 'Second' });

      const toasts = useToastStore.getState().toasts;
      expect(toasts[0].title).toBe('Second');
      expect(toasts[1].title).toBe('First');
    });

    it('sets default duration from config', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.duration).toBe(4000);
    });

    it('respects custom duration', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
        duration: 10000,
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.duration).toBe(10000);
    });

    it('sets dismissible to true by default', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.dismissible).toBe(true);
    });

    it('respects custom dismissible value', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
        dismissible: false,
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.dismissible).toBe(false);
    });

    it('auto-dismisses toast after duration', () => {
      useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
        duration: 3000,
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);

      act(() => {
        vi.advanceTimersByTime(3000);
      });

      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('does not auto-dismiss when duration is 0', () => {
      useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
        duration: 0,
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);

      act(() => {
        vi.advanceTimersByTime(10000);
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);
    });

    it('enforces max toasts limit', () => {
      for (let i = 0; i < 10; i++) {
        useToastStore.getState().addToast({
          type: 'info',
          title: `Toast ${i}`,
        });
      }

      const toasts = useToastStore.getState().toasts;
      expect(toasts).toHaveLength(5);
    });

    it('removes oldest toasts when exceeding limit', () => {
      for (let i = 0; i < 6; i++) {
        useToastStore.getState().addToast({
          type: 'info',
          title: `Toast ${i}`,
        });
      }

      const toasts = useToastStore.getState().toasts;
      expect(toasts[0].title).toBe('Toast 5');
      expect(toasts[4].title).toBe('Toast 1');
    });

    it('includes message when provided', () => {
      const id = useToastStore.getState().addToast({
        type: 'error',
        title: 'Error!',
        message: 'Something went wrong',
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.message).toBe('Something went wrong');
    });

    it('includes action when provided', () => {
      const onClick = vi.fn();
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
        action: {
          label: 'Retry',
          onClick,
        },
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.action?.label).toBe('Retry');
      toastItem?.action?.onClick();
      expect(onClick).toHaveBeenCalled();
    });
  });

  describe('dismissToast', () => {
    it('removes toast by id', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test',
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);

      useToastStore.getState().dismissToast(id);

      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('does not error when dismissing non-existent toast', () => {
      expect(() => {
        useToastStore.getState().dismissToast('non-existent');
      }).not.toThrow();
    });

    it('only removes specified toast', () => {
      const id1 = useToastStore.getState().addToast({ type: 'info', title: 'First' });
      const id2 = useToastStore.getState().addToast({ type: 'info', title: 'Second' });

      useToastStore.getState().dismissToast(id1);

      const toasts = useToastStore.getState().toasts;
      expect(toasts).toHaveLength(1);
      expect(toasts[0].id).toBe(id2);
    });
  });

  describe('dismissAll', () => {
    it('removes all toasts', () => {
      useToastStore.getState().addToast({ type: 'info', title: 'First' });
      useToastStore.getState().addToast({ type: 'info', title: 'Second' });
      useToastStore.getState().addToast({ type: 'info', title: 'Third' });

      expect(useToastStore.getState().toasts).toHaveLength(3);

      useToastStore.getState().dismissAll();

      expect(useToastStore.getState().toasts).toHaveLength(0);
    });
  });

  describe('updateConfig', () => {
    it('updates config values', () => {
      useToastStore.getState().updateConfig({
        maxToasts: 10,
        defaultDuration: 5000,
      });

      const config = useToastStore.getState().config;
      expect(config.maxToasts).toBe(10);
      expect(config.defaultDuration).toBe(5000);
    });

    it('preserves unmodified config values', () => {
      useToastStore.getState().updateConfig({
        maxToasts: 10,
      });

      const config = useToastStore.getState().config;
      expect(config.maxToasts).toBe(10);
      expect(config.position).toBe('top-right');
    });
  });

  describe('toast helpers', () => {
    it('toast.success creates success toast', () => {
      const id = toast.success('Success!');

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.type).toBe('success');
      expect(toastItem?.title).toBe('Success!');
    });

    it('toast.error creates error toast', () => {
      const id = toast.error('Error!');

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.type).toBe('error');
      expect(toastItem?.title).toBe('Error!');
    });

    it('toast.warning creates warning toast', () => {
      const id = toast.warning('Warning!');

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.type).toBe('warning');
      expect(toastItem?.title).toBe('Warning!');
    });

    it('toast.info creates info toast', () => {
      const id = toast.info('Info!');

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.type).toBe('info');
      expect(toastItem?.title).toBe('Info!');
    });

    it('toast helpers accept options', () => {
      const id = toast.success('Success!', {
        message: 'Details here',
        duration: 10000,
      });

      const toastItem = useToastStore.getState().toasts.find(t => t.id === id);
      expect(toastItem?.message).toBe('Details here');
      expect(toastItem?.duration).toBe(10000);
    });

    it('toast.dismiss dismisses toast by id', () => {
      const id = toast.info('Test');

      expect(useToastStore.getState().toasts).toHaveLength(1);

      toast.dismiss(id);

      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('toast.dismissAll dismisses all toasts', () => {
      toast.info('First');
      toast.info('Second');

      expect(useToastStore.getState().toasts).toHaveLength(2);

      toast.dismissAll();

      expect(useToastStore.getState().toasts).toHaveLength(0);
    });
  });

  describe('useToastStore hook', () => {
    it('returns current toasts and config', () => {
      useToastStore.getState().addToast({ type: 'info', title: 'Test' });

      const { result } = renderHook(() => useToastStore());

      expect(result.current.toasts).toHaveLength(1);
      expect(result.current.config.position).toBe('top-right');
    });

    it('updates when toasts change', () => {
      const { result } = renderHook(() => useToastStore());

      expect(result.current.toasts).toHaveLength(0);

      act(() => {
        result.current.addToast({ type: 'info', title: 'Test' });
      });

      expect(result.current.toasts).toHaveLength(1);
    });

    it('updates when config changes', () => {
      const { result } = renderHook(() => useToastStore());

      expect(result.current.config.maxToasts).toBe(5);

      act(() => {
        result.current.updateConfig({ maxToasts: 10 });
      });

      expect(result.current.config.maxToasts).toBe(10);
    });
  });

  describe('memory leak prevention', () => {
    it('dismissAll() clears all auto-dismiss timers', () => {
      useToastStore.getState().addToast({
        type: 'info',
        title: 'Toast 1',
        duration: 5000,
      });
      useToastStore.getState().addToast({
        type: 'info',
        title: 'Toast 2',
        duration: 5000,
      });
      useToastStore.getState().addToast({
        type: 'info',
        title: 'Toast 3',
        duration: 5000,
      });

      expect(useToastStore.getState().toasts).toHaveLength(3);

      // Dismiss all toasts
      act(() => {
        useToastStore.getState().dismissAll();
      });

      expect(useToastStore.getState().toasts).toHaveLength(0);

      // Verify timers don't fire after dismissAll
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      // Toasts should not reappear
      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('dismissToast() clears individual timer', () => {
      const id = useToastStore.getState().addToast({
        type: 'info',
        title: 'Test Toast',
        duration: 5000,
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);

      // Manually dismiss toast
      act(() => {
        useToastStore.getState().dismissToast(id);
      });

      expect(useToastStore.getState().toasts).toHaveLength(0);

      // Verify timer doesn't fire after manual dismissal
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      // Toast should not reappear
      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('clears timers for oldest toasts when exceeding maxToasts limit', () => {
      for (let i = 0; i < 5; i++) {
        useToastStore.getState().addToast({
          type: 'info',
          title: `Toast ${i}`,
          duration: 5000,
        });
      }

      expect(useToastStore.getState().toasts).toHaveLength(5);

      useToastStore.getState().addToast({
        type: 'info',
        title: 'Toast 5',
        duration: 5000,
      });

      expect(useToastStore.getState().toasts).toHaveLength(5);
      expect(useToastStore.getState().toasts[0].title).toBe('Toast 5');

      // Advance timers - the removed toast's timer should not cause issues
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      // All toasts should be dismissed by their timers
      expect(useToastStore.getState().toasts).toHaveLength(0);
    });

    it('handles multiple timer-based operations without leaks', () => {
      const id1 = useToastStore.getState().addToast({
        type: 'info',
        title: 'First',
        duration: 5000,
      });

      act(() => {
        useToastStore.getState().dismissToast(id1);
      });

      const id2 = useToastStore.getState().addToast({
        type: 'info',
        title: 'Second',
        duration: 5000,
      });

      expect(useToastStore.getState().toasts).toHaveLength(1);
      expect(useToastStore.getState().toasts[0].id).toBe(id2);

      // Advance timers
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      // Only the second toast should have been dismissed
      expect(useToastStore.getState().toasts).toHaveLength(0);
    });
  });
});
