import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AnswerTimer } from '@/features/learning/practice/practical/AnswerTimer';

describe('AnswerTimer', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('counts the current answer against two minutes and restarts for the next question', () => {
    const { rerender } = render(<AnswerTimer since="question-1" />);
    expect(screen.getByRole('timer', { name: 'Answer time' })).toHaveTextContent('0:00');
    expect(screen.getByText(/aim for about two minutes/i)).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(125_000);
    });
    expect(screen.getByRole('timer', { name: 'Answer time' })).toHaveTextContent('2:05');
    expect(screen.getByRole('status')).toHaveTextContent('Past two minutes: land your result and stop.');
    rerender(<AnswerTimer since="question-2" />);
    expect(screen.getByRole('timer', { name: 'Answer time' })).toHaveTextContent('0:00');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('holds while the answer is with the interviewer and resumes if the send fails', () => {
    const { rerender } = render(<AnswerTimer since="question-1" />);
    act(() => {
      vi.advanceTimersByTime(90_000);
    });
    rerender(<AnswerTimer since="question-1" running={false} />);
    act(() => {
      vi.advanceTimersByTime(75_000);
    });
    expect(screen.getByRole('timer', { name: 'Answer time' })).toHaveTextContent('1:30');
    expect(screen.getByText('Answer sent.')).toBeInTheDocument();
    rerender(<AnswerTimer since="question-1" />);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.getByRole('timer', { name: 'Answer time' })).toHaveTextContent('1:40');
  });
});
