import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ElapsedClock } from './ElapsedClock';

describe('ElapsedClock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('counts on from the elapsed time while running', () => {
    render(<ElapsedClock elapsedMs={65_000} running label="Session time" />);
    expect(screen.getByRole('timer', { name: 'Session time' })).toHaveTextContent('1:05');
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(screen.getByRole('timer')).toHaveTextContent('1:10');
  });

  it('keeps what it reached when it stops running', () => {
    const { rerender } = render(<ElapsedClock elapsedMs={0} running />);
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    rerender(<ElapsedClock elapsedMs={0} running={false} />);
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(screen.getByRole('timer')).toHaveTextContent('0:30');
  });

  it('says so past the soft limit and carries on counting', () => {
    render(<ElapsedClock elapsedMs={0} running softLimitMs={60_000} />);
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(61_000);
    });
    expect(screen.getByRole('status')).toHaveTextContent(/^Past 1:00\. You can keep going\.$/);
    expect(screen.getByRole('timer')).toHaveTextContent('1:01');
  });

  it('uses the given wording past the limit', () => {
    render(<ElapsedClock elapsedMs={61_000} running={false} softLimitMs={60_000} pastLimit="Past the aim: stop." />);
    expect(screen.getByRole('status')).toHaveTextContent('Past the aim: stop.');
  });
});
