import { createRef, useState } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ErrorBoundary, withErrorBoundary } from './ErrorBoundary';
import { diagnostics } from '../../utils/diagnostics';


let shouldThrow = true;
function Recipe() {
  if (shouldThrow) throw new Error('Recipe card broke');
  return <p>Recipe card</p>;
}

beforeEach(() => {
  shouldThrow = true;
  diagnostics.clear();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('ErrorBoundary recovery', () => {
  it('shows an isolated error with a retry that recovers the section', () => {
    const onReset = vi.fn();
    render(<ErrorBoundary isolate name="Recipe card" onReset={onReset}><Recipe /></ErrorBoundary>);
    expect(screen.getByText('Error in Recipe card')).toBeInTheDocument();
    expect(screen.getByText('Recipe card broke')).toBeInTheDocument();
    expect(diagnostics.getSnapshot()[0]).toMatchObject({ source: 'Recipe card', message: 'Recipe card broke' });
    shouldThrow = false;
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onReset).toHaveBeenCalled();
    expect(screen.getByText('Recipe card')).toBeInTheDocument();
  });

  it('names an unnamed component generically, and renders nothing without isolation', () => {
    const { unmount } = render(<ErrorBoundary isolate><Recipe /></ErrorBoundary>);
    expect(screen.getByText('Error in component')).toBeInTheDocument();
    unmount();
    const { container } = render(<ErrorBoundary><Recipe /></ErrorBoundary>);
    expect(container).toBeEmptyDOMElement();
  });

  it('survives callbacks that throw', () => {
    const onError = vi.fn(() => { throw new Error('reporter down'); });
    const onReset = vi.fn(() => { throw new Error('reset hook broke'); });
    render(<ErrorBoundary isolate onError={onError} onReset={onReset}><Recipe /></ErrorBoundary>);
    expect(console.error).toHaveBeenCalledWith('Error in onError handler:', expect.any(Error));
    shouldThrow = false;
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(console.error).toHaveBeenCalledWith('Error in onReset handler:', expect.any(Error));
    expect(screen.getByText('Recipe card')).toBeInTheDocument();
  });

  it('resets on any prop change when asked to', () => {
    function Host() {
      const [label, setLabel] = useState('first');
      return (
        <>
          <button type="button" onClick={() => { shouldThrow = false; setLabel('second'); }}>Change props</button>
          <ErrorBoundary resetOnPropsChange isolate name={label}><Recipe /></ErrorBoundary>
        </>
      );
    }
    render(<Host />);
    expect(screen.getByText('Error in first')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Change props' }));
    expect(screen.getByText('Recipe card')).toBeInTheDocument();
  });

  it('stops resetting after repeated failures', () => {
    render(<ErrorBoundary isolate><Recipe /></ErrorBoundary>);
    for (let attempt = 0; attempt < 6; attempt += 1) fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(console.warn).toHaveBeenCalledWith('ErrorBoundary: Maximum error count reached. Not resetting.');
    shouldThrow = false;
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(screen.getByText('Error in component')).toBeInTheDocument();
  });

  it('retries on a schedule, replacing an earlier schedule, and cancels it on unmount', () => {
    vi.useFakeTimers();
    const ref = createRef<ErrorBoundary>();
    const onReset = vi.fn();
    const { unmount } = render(<ErrorBoundary ref={ref} isolate onReset={onReset}><Recipe /></ErrorBoundary>);
    shouldThrow = false;
    act(() => { ref.current!.scheduleReset(5000); ref.current!.scheduleReset(); });
    act(() => { vi.advanceTimersByTime(2999); });
    expect(screen.getByText('Error in component')).toBeInTheDocument();
    act(() => { vi.advanceTimersByTime(1); });
    expect(screen.getByText('Recipe card')).toBeInTheDocument();

    shouldThrow = true;
    expect(onReset).toHaveBeenCalledTimes(1);
    act(() => { ref.current!.scheduleReset(1000); });
    unmount();
    act(() => { vi.advanceTimersByTime(1000); });
    expect(onReset).toHaveBeenCalledTimes(1);
  });

  it('wraps a component in a boundary with a readable name', () => {
    const Safe = withErrorBoundary(Recipe, { isolate: true, name: 'Card' });
    expect(Safe.displayName).toBe('withErrorBoundary(Recipe)');
    render(<Safe />);
    expect(screen.getByText('Error in Card')).toBeInTheDocument();
    const Anonymous = withErrorBoundary(() => null);
    expect(Anonymous.displayName).toBe('withErrorBoundary(Component)');
  });
});
