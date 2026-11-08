import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ElapsedClock, formatClock } from './ElapsedClock';

describe('formatClock', () => {
  it('formats milliseconds into m:ss format, counting a part of a second as the whole of it', () => {
    expect(formatClock(545_000)).toBe('9:05');
    expect(formatClock(600_000)).toBe('10:00');
    expect(formatClock(999)).toBe('0:01');
    expect(formatClock(0)).toBe('0:00');
    expect(formatClock(-4_000)).toBe('0:00');
  });
});

describe('ElapsedClock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-08T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders fixed elapsed time when elapsedMs is provided', () => {
    render(<ElapsedClock elapsedMs={125_000} />);

    expect(screen.getByRole('timer')).toHaveTextContent('2:05');
  });

  it('ticks forward when running is true with startedAt', () => {
    render(<ElapsedClock startedAt={Date.now()} running={true} />);

    expect(screen.getByRole('timer')).toHaveTextContent('0:00');

    act(() => {
      vi.advanceTimersByTime(65_000);
    });

    expect(screen.getByRole('timer')).toHaveTextContent('1:05');
  });

  it('pauses counting when running is false', () => {
    const { rerender } = render(<ElapsedClock startedAt={Date.now()} running={true} />);

    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(screen.getByRole('timer')).toHaveTextContent('0:30');

    rerender(<ElapsedClock startedAt={Date.now()} running={false} elapsedMs={30_000} />);

    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(screen.getByRole('timer')).toHaveTextContent('0:30');
  });

  it('shows soft budget and indicates overtime when budget is exceeded without ending the session', () => {
    // 600_000 ms (10 min) budget, 700_000 ms elapsed (overtime)
    render(<ElapsedClock elapsedMs={700_000} budgetMs={600_000} />);

    // Must still show timer / clock
    expect(screen.getByRole('timer')).toBeInTheDocument();

    // Must indicate overtime status (soft budget exceeded)
    const overtimeIndicator = screen.getByRole('status');
    expect(overtimeIndicator).toHaveTextContent(/overtime|past budget|over budget|time is up/i);
  });

  it('continues ticking when overtime without ending or disabling', () => {
    render(<ElapsedClock startedAt={Date.now() - 595_000} budgetMs={600_000} running={true} />);

    expect(screen.getByRole('timer')).toHaveTextContent('9:55');

    act(() => {
      vi.advanceTimersByTime(15_000); // Now 610_000 ms, 10 seconds past budget
    });

    expect(screen.getByRole('timer')).toHaveTextContent('10:10');
    expect(screen.getByRole('status')).toBeInTheDocument();
  });
});
