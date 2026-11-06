import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { formatClock, InterviewClock } from './InterviewClock';

describe('formatClock', () => {
  it('shows minutes and two-digit seconds, counting a part of a second as the whole of it', () => {
    expect(formatClock(545_000)).toBe('9:05');
    expect(formatClock(600_000)).toBe('10:00');
    expect(formatClock(999)).toBe('0:01');
    expect(formatClock(0)).toBe('0:00');
    expect(formatClock(-4_000)).toBe('0:00');
  });
});

describe('InterviewClock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-04T12:00:00Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('counts down from when the attempt started, not from when it was opened', () => {
    render(<InterviewClock startedAt={Date.now() - 30_000} limitMs={600_000} />);
    expect(screen.getByRole('timer')).toHaveTextContent('Time left 9:30');
    act(() => {
      vi.advanceTimersByTime(90_000);
    });
    expect(screen.getByRole('timer')).toHaveTextContent('Time left 8:00');
  });

  it('says when time is up, and goes on saying it: it hands nothing in', () => {
    render(<InterviewClock startedAt={Date.now() - 590_000} limitMs={600_000} />);
    expect(screen.getByRole('timer')).toHaveTextContent('Time left 0:10');
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.getByRole('status')).toHaveTextContent('Time is up');
    expect(screen.queryByRole('timer')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(120_000);
    });
    expect(screen.getByRole('status')).toHaveTextContent('Time is up');
  });

  it('is already out of time for an attempt that ran past its limit while the learner was away', () => {
    render(<InterviewClock startedAt={Date.now() - 700_000} limitMs={600_000} />);
    expect(screen.getByRole('status')).toHaveTextContent('Time is up');
  });
});
