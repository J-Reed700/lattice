import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AnswerTimer } from '@/features/learning/practice/practical/AnswerTimer';

describe('AnswerTimer', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('counts the current answer against two minutes and restarts for the next question', () => {
    const { rerender } = render(<AnswerTimer since="question-1" />);
    expect(screen.getByText(/aim for about two minutes/)).toHaveTextContent('Answering for 0:00');
    act(() => { vi.advanceTimersByTime(125_000); });
    expect(screen.getByText(/past two minutes/)).toHaveTextContent('Answering for 2:05');
    rerender(<AnswerTimer since="question-2" />);
    expect(screen.getByText(/aim for about two minutes/)).toHaveTextContent('Answering for 0:00');
  });
});
