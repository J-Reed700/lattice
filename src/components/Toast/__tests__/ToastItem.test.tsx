import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastItem } from '../ToastItem';

import type { Toast } from '../../../stores/toastStore';

const toast = (overrides: Partial<Toast> = {}): Toast => ({
  id: 't1', type: 'success', title: 'Recipe saved', createdAt: Date.now(), dismissible: true, ...overrides,
} as Toast);
const remaining = () => Number(screen.getByRole('progressbar', { name: 'Time remaining' }).getAttribute('aria-valuenow'));
const advance = (ms: number) => act(() => { vi.advanceTimersByTime(ms); });

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('ToastItem', () => {
  it('shows its title, message and type, and counts down to dismissal', () => {
    const onDismiss = vi.fn();
    render(<ToastItem toast={toast({ message: 'Added to Desserts', duration: 1000 })} onDismiss={onDismiss} pauseOnHover index={0} />);
    expect(screen.getByRole('status')).toHaveTextContent('Recipe saved');
    expect(screen.getByText('Added to Desserts')).toBeInTheDocument();
    advance(496);
    expect(remaining()).toBeGreaterThan(45);
    expect(remaining()).toBeLessThan(55);
    advance(600);
    expect(screen.getByRole('status')).toHaveClass('opacity-0');
    advance(300);
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  // Bug: once its time is up the countdown keeps calling dismiss on every tick
  // until the toast unmounts, so onDismiss fires many times (ToastItem.tsx:102-103,
  // updateProgress never clears its interval).
  it.fails('dismisses itself exactly once when its time is up', () => {
    const onDismiss = vi.fn();
    render(<ToastItem toast={toast({ duration: 200 })} onDismiss={onDismiss} pauseOnHover index={0} />);
    advance(800);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('pauses while hovered when asked to', () => {
    const onDismiss = vi.fn();
    render(<ToastItem toast={toast({ duration: 1000 })} onDismiss={onDismiss} pauseOnHover index={0} />);
    advance(496);
    fireEvent.mouseEnter(screen.getByRole('status'));
    const paused = remaining();
    advance(5000);
    expect(remaining()).toBe(paused);
    expect(onDismiss).not.toHaveBeenCalled();
    fireEvent.mouseLeave(screen.getByRole('status'));
    advance(300);
    expect(remaining()).toBeLessThan(paused);
    advance(600);
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  it('keeps counting while hovered when pausing is off', () => {
    const onDismiss = vi.fn();
    render(<ToastItem toast={toast({ duration: 500 })} onDismiss={onDismiss} pauseOnHover={false} index={1} />);
    expect(screen.getByRole('status')).toHaveClass('mt-2');
    fireEvent.mouseEnter(screen.getByRole('status'));
    advance(900);
    expect(onDismiss).toHaveBeenCalled();
    fireEvent.mouseLeave(screen.getByRole('status'));
  });

  it('stays until dismissed when it has no duration', () => {
    const onDismiss = vi.fn();
    render(<ToastItem toast={toast({ type: 'error', duration: 0 })} onDismiss={onDismiss} pauseOnHover index={0} />);
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    advance(10_000);
    expect(onDismiss).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss notification' }));
    advance(299);
    expect(onDismiss).not.toHaveBeenCalled();
    advance(1);
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  it('runs its action and then goes away', () => {
    const onDismiss = vi.fn();
    const onClick = vi.fn();
    render(<ToastItem toast={toast({ type: 'warning', action: { label: 'Undo', onClick }, dismissible: false })} onDismiss={onDismiss} pauseOnHover index={0} />);
    expect(screen.queryByRole('button', { name: 'Dismiss notification' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(onClick).toHaveBeenCalled();
    advance(300);
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  it.each(['success', 'error', 'warning', 'info'] as const)('draws an icon for a %s toast, or the one it was given', (type) => {
    const { container, unmount } = render(<ToastItem toast={toast({ type })} onDismiss={vi.fn()} pauseOnHover index={0} />);
    expect(container.querySelector('svg')).not.toBeNull();
    unmount();
    render(<ToastItem toast={toast({ type, icon: <span>custom icon</span> })} onDismiss={vi.fn()} pauseOnHover index={0} />);
    expect(screen.getByText('custom icon')).toBeInTheDocument();
  });
});
