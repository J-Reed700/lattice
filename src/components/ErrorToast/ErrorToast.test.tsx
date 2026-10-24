import { act, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ErrorToast, ErrorToastContainer, useErrorToast, type ToastError } from './ErrorToast';

const toast = (overrides: Partial<ToastError> = {}): ToastError => ({ id: 't1', message: 'The jam did not set', severity: 'error', ...overrides });
const bar = (container: HTMLElement) => container.querySelector<HTMLElement>('[style]');

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('ErrorToast', () => {
  it('fades in, counts down and dismisses itself when its time is up', () => {
    const onDismiss = vi.fn();
    const { container } = render(<ErrorToast error={toast({ duration: 1000 })} onDismiss={onDismiss} />);
    expect(container.firstElementChild).toHaveClass('opacity-0');
    act(() => { vi.advanceTimersByTime(10); });
    expect(container.firstElementChild).toHaveClass('opacity-100');
    act(() => { vi.advanceTimersByTime(500); });
    expect(bar(container)).toHaveStyle({ width: '50%' });
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(600); });
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  it('stays until dismissed when the duration is zero', () => {
    const onDismiss = vi.fn();
    const { container } = render(<ErrorToast error={toast({ duration: 0, severity: 'warning' })} onDismiss={onDismiss} />);
    act(() => { vi.advanceTimersByTime(60_000); });
    expect(onDismiss).not.toHaveBeenCalled();
    expect(bar(container)).toBeNull();
    fireEvent.click(screen.getByRole('button'));
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });

  it('runs its action and then dismisses', () => {
    const onDismiss = vi.fn();
    const onClick = vi.fn();
    render(<ErrorToast error={toast({ severity: 'info', action: { label: 'Retry', onClick } })} onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onClick).toHaveBeenCalled();
    expect(onDismiss).toHaveBeenCalledWith('t1');
  });
});

describe('ErrorToastContainer', () => {
  it('stacks every toast in the chosen corner', () => {
    const { container } = render(
      <ErrorToastContainer errors={[toast(), toast({ id: 't2', message: 'Oven offline', severity: 'warning' })]} onDismiss={vi.fn()} position="bottom-left" />,
    );
    expect(container.firstElementChild).toHaveClass('bottom-4', 'left-4');
    expect(screen.getByText('The jam did not set')).toBeInTheDocument();
    expect(screen.getByText('Oven offline')).toBeInTheDocument();
  });

  it('defaults to the top right', () => {
    const { container } = render(<ErrorToastContainer errors={[]} onDismiss={vi.fn()} />);
    expect(container.firstElementChild).toHaveClass('top-4', 'right-4');
  });
});

describe('useErrorToast', () => {
  it('adds errors with defaults and removes them', () => {
    const { result } = renderHook(() => useErrorToast());
    let id = '';
    act(() => { id = result.current.showError('Burnt toast'); });
    expect(result.current.errors).toEqual([expect.objectContaining({ id, message: 'Burnt toast', severity: 'error' })]);
    act(() => result.current.dismissError(id));
    expect(result.current.errors).toEqual([]);
  });
});
