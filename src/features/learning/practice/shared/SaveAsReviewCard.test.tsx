import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SaveAsReviewCard } from './SaveAsReviewCard';

const mockMutation = {
  mutate: vi.fn(),
  isSuccess: false,
  isPending: false,
  isError: false,
  error: null as Error | null,
};

vi.mock('@/features/learning/memory/useLearningMemory', () => ({
  useSaveLearningCardDraft: () => mockMutation,
}));

describe('SaveAsReviewCard', () => {
  beforeEach(() => {
    mockMutation.mutate.mockReset();
    mockMutation.isSuccess = false;
    mockMutation.isPending = false;
    mockMutation.isError = false;
    mockMutation.error = null;
  });

  it('renders prompt text and textarea without arbitrary maxLength', () => {
    render(
      <SaveAsReviewCard
        programId="prog-1"
        lessonId="lesson-1"
        promptText="Binary search requires a sorted array."
        promptLabel="What is right instead?"
      />,
    );

    expect(screen.getByText('Binary search requires a sorted array.')).toBeInTheDocument();
    expect(screen.getByText('What is right instead?')).toBeInTheDocument();

    const textarea = screen.getByRole('textbox');
    // Rule: no arbitrary limits / no maxLength on learner answers
    expect(textarea).not.toHaveAttribute('maxLength');
  });

  it('saves a recall card draft on button click', () => {
    render(
      <SaveAsReviewCard
        programId="prog-1"
        lessonId="lesson-1"
        promptText="Hash tables provide O(1) worst-case lookups."
        explanation="Drafted from misconception in coding practice"
        initialAnswer="Hash tables provide O(1) average-case lookups, but O(n) worst-case."
      />,
    );

    const button = screen.getByRole('button', { name: /recall card/i });
    fireEvent.click(button);

    expect(mockMutation.mutate).toHaveBeenCalledWith({
      draftId: null,
      programId: 'prog-1',
      lessonId: 'lesson-1',
      question: 'Hash tables provide O(1) worst-case lookups.',
      answer: 'Hash tables provide O(1) average-case lookups, but O(n) worst-case.',
      explanation: 'Drafted from misconception in coding practice',
      sourceIds: [],
    });
  });

  it('announces confirmation via role="status" when saved', () => {
    mockMutation.isSuccess = true;

    render(
      <SaveAsReviewCard
        programId="prog-1"
        lessonId="lesson-1"
        promptText="Misconception text"
      />,
    );

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent(/saved as a recall card draft/i);
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('announces error via role="alert" when saving fails', () => {
    mockMutation.isError = true;
    mockMutation.error = new Error('Database write failed');

    render(
      <SaveAsReviewCard
        programId="prog-1"
        lessonId="lesson-1"
        promptText="Misconception text"
      />,
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Database write failed');
  });
});
