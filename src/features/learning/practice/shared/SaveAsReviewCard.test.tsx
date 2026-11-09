import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SaveAsReviewCard } from './SaveAsReviewCard';

const mutation = {
  mutate: vi.fn(),
  isSuccess: false,
  isPending: false,
  isError: false,
  error: null as Error | null,
};

vi.mock('@/features/learning/memory/useLearningMemory', () => ({
  useSaveLearningCardDraft: () => mutation,
}));
vi.mock('@/features/learning/workspace/useLearningStudio', () => ({
  useLearningProgram: () => ({ data: { summary: { currentLessonId: 'lesson-current' }, modules: [] } }),
}));

describe('SaveAsReviewCard', () => {
  beforeEach(() => {
    mutation.mutate.mockReset();
    mutation.isSuccess = false;
    mutation.isPending = false;
    mutation.isError = false;
    mutation.error = null;
  });

  it('shows the front and a back the learner can edit', () => {
    render(<SaveAsReviewCard programId="p1" lessonId="l1" front="Why sort first?" back="So equal items sit together." />);
    expect(screen.getByText('Why sort first?')).toBeInTheDocument();
    expect(screen.getByRole('textbox')).toHaveValue('So equal items sit together.');
  });

  it('saves the card as a draft with the edited back', () => {
    render(
      <SaveAsReviewCard programId="p1" lessonId="l1" front="Why sort first?" back="x" sourceLabel="A coding session" />,
    );
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Equal items sit together.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Make a recall card' }));
    expect(mutation.mutate).toHaveBeenCalledWith(
      expect.objectContaining({
        draftId: null,
        programId: 'p1',
        lessonId: 'l1',
        question: 'Why sort first?',
        answer: 'Equal items sit together.',
        explanation: 'A coding session',
      }),
      expect.anything(),
    );
  });

  it("files the card under the course's current lesson when none is given", () => {
    render(<SaveAsReviewCard programId="p1" front="Q" back="A" />);
    fireEvent.click(screen.getByRole('button', { name: 'Make a recall card' }));
    expect(mutation.mutate).toHaveBeenCalledWith(
      expect.objectContaining({ lessonId: 'lesson-current' }),
      expect.anything(),
    );
  });

  it('confirms once saved', () => {
    mutation.isSuccess = true;
    render(<SaveAsReviewCard programId="p1" lessonId="l1" front="Q" back="A" />);
    expect(screen.getByRole('status')).toHaveTextContent(/saved/i);
  });

  it('shows a failure with Try again', () => {
    mutation.isError = true;
    mutation.error = new Error('Prepare this lesson first.');
    render(<SaveAsReviewCard programId="p1" lessonId="l1" front="Q" back="A" />);
    expect(screen.getByRole('alert')).toHaveTextContent('Prepare this lesson first.');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mutation.mutate).toHaveBeenCalled();
  });

  it('will not save a blank answer and says why', () => {
    render(<SaveAsReviewCard programId="p1" lessonId="l1" front="Q" back="" />);
    const button = screen.getByRole('button', { name: 'Make a recall card' });
    expect(button).toBeDisabled();
    expect(screen.getByText('Write the answer first.')).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });
    expect(button).toBeDisabled();
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'An answer.' } });
    expect(button).toBeEnabled();
    expect(screen.queryByText('Write the answer first.')).not.toBeInTheDocument();
  });
});
