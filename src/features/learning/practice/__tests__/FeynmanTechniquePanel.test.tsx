import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { FeynmanTechniquePanel } from '@/features/learning/practice/FeynmanTechniquePanel';
import type * as Wire from '@/lib/bindings';
import { setDraft, useDraftStore } from '@/shared/drafts/drafts';

type LearningLessonDto = Wire.LearningLessonDto;
type LearningModuleDto = Wire.LearningModuleDto;
type LearningProgramDto = Wire.LearningProgramDto;

const reviewCard = vi.hoisted(() => vi.fn());
vi.mock('@/features/learning/practice/shared/SaveAsReviewCard', () => ({
  SaveAsReviewCard: (props: Record<string, unknown>) => {
    reviewCard(props);
    return <div data-testid="review-card" />;
  },
}));

describe('FeynmanTechniquePanel UI', () => {
  const mockLesson: LearningLessonDto = {
    id: 'l-1',
    title: 'Recursive Algorithms',
    objective: 'Explain the base case and inductive step',
    estimatedMinutes: 25,
    preparation: 'ready',
    completed: false,
    blocks: [
      {
        kind: 'explanation',
        title: 'Call Stack and Base Case',
        body: 'A recursive function calls itself until reaching a termination condition known as the base case.',
        sourceIds: [],
      },
    ],
  };

  const mockModule: LearningModuleDto = {
    id: 'm-1',
    title: 'Core Computing',
    summary: 'Foundations of algorithms',
    outcomes: ['Master recursion principles'],
    lessons: [mockLesson],
  };

  const mockProgram: LearningProgramDto = {
    summary: {
      id: 'prog-1',
      title: 'Computer Science',
      goal: 'Learn algorithms',
      status: 'active',
      revision: 1,
      moduleCount: 1,
      lessonCount: 1,
      completedLessons: 0,
      currentLessonId: 'l-1',
      createdAt: 1_700_000_000,
    },
    priorKnowledge: '',
    minutesPerSession: 30,
    modelName: 'default',
    modules: [mockModule],
    sources: [],
  };

  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    useDraftStore.setState({ drafts: {} });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders Feynman guidance and defaults to current lesson title', () => {
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    expect(screen.getByText(/The Feynman Technique · Teach a Beginner/i)).toBeInTheDocument();
    expect(screen.getByText(/Teach it from memory, then check it/i)).toBeInTheDocument();
    expect(screen.getByText('Recursive Algorithms')).toBeInTheDocument();
  });

  it('allows switching target concept to block title or custom concept', async () => {
    const user = userEvent.setup();
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const select = screen.getByLabelText(/Choose concept to teach/i);
    await user.selectOptions(select, 'Call Stack and Base Case');

    expect(screen.getByText('Call Stack and Base Case')).toBeInTheDocument();

    await user.selectOptions(select, '__custom__');
    const customInput = screen.getByPlaceholderText(/Enter specific concept or idea to explain/i);
    await user.type(customInput, 'Russian Nesting Dolls');

    expect(screen.getByText('Russian Nesting Dolls')).toBeInTheDocument();
  });

  it('updates analysis real-time and toggles the read out loud checkbox', () => {
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const explanationInput = screen.getByLabelText(/Your plain-language explanation/i);
    const analogyInput = screen.getByLabelText(/Everyday Analogy/i);

    // Initial state
    expect(screen.getByText('Needs More Explanation')).toBeInTheDocument();

    // Type a simple, clear explanation (> 15 words) with plain-language cue and sentences <= 28 words
    fireEvent.change(explanationInput, {
      target: {
        value:
          'A recursive step is like opening nesting dolls, which means each doll holds a smaller doll. It repeats until reaching the base case because you find the smallest toy.',
      },
    });
    fireEvent.change(analogyInput, {
      target: {
        value: 'It works like Russian matryoshka dolls.',
      },
    });

    expect(screen.getByText('Simple & Clear')).toBeInTheDocument();

    // Check the speak out loud toggle
    const checkbox = screen.getByRole('checkbox', { name: /Speak out loud test/i });
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
  });

  it('copies explanation to clipboard with feedback and resets copied timer', async () => {
    vi.useFakeTimers();
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextMock },
      configurable: true,
      writable: true,
    });

    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const explanationInput = screen.getByLabelText(/Your plain-language explanation/i);
    fireEvent.change(explanationInput, {
      target: {
        value: 'This is an explanation that teaches how repetition stops when a target is hit.',
      },
    });

    const copyButton = screen.getByRole('button', { name: /Copy explanation/i });
    await act(async () => {
      fireEvent.click(copyButton);
    });

    expect(writeTextMock).toHaveBeenCalled();
    expect(screen.getByText('Copied')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.queryByText('Copied')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('writes nothing on mount, so a draft kept for later is not wiped', () => {
    setDraft('feynman:prog-1:l-1:explanation', 'Kept for later.');
    const before = localStorage.getItem('lattice.drafts');
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);
    expect(screen.getByDisplayValue('Kept for later.')).toBeInTheDocument();
    expect(localStorage.getItem('lattice.drafts')).toBe(before);
    expect(localStorage.getItem('lattice.drafts')).toContain('Kept for later.');
  });

  it('keeps what is typed in the shared draft store, without a Save draft button', () => {
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);
    expect(localStorage.getItem('lattice.drafts')).toBeNull();
    fireEvent.change(screen.getByLabelText(/Your plain-language explanation/i), { target: { value: 'Persisted plain explanation text.' } });
    fireEvent.change(screen.getByLabelText(/Everyday Analogy/i), { target: { value: 'Like a recipe card.' } });
    fireEvent.click(screen.getByRole('checkbox', { name: /Speak out loud test/i }));
    const stored = JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}') as Record<string, string>;
    expect(stored['feynman:prog-1:l-1:explanation']).toBe('Persisted plain explanation text.');
    expect(stored['feynman:prog-1:l-1:analogy']).toBe('Like a recipe card.');
    expect(screen.queryByRole('button', { name: /Save draft/i })).not.toBeInTheDocument();
    expect(screen.getByText('Draft kept on this device')).toBeVisible();
  });

  it('restores the explanation, analogy, concept and speak-aloud tick on mount', () => {
    setDraft('feynman:prog-1:l-1:explanation', 'Stored explanation loaded on mount.');
    setDraft('feynman:prog-1:l-1:analogy', 'Like a recipe card.');
    setDraft('feynman:prog-1:l-1:concept', 'Call Stack and Base Case');
    setDraft('feynman:prog-1:l-1:readAloud', 'yes');

    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    expect(screen.getByDisplayValue('Stored explanation loaded on mount.')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Like a recipe card.')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /Speak out loud test/i })).toBeChecked();
    expect(screen.getByLabelText(/Choose concept to teach/i)).toHaveValue('Call Stack and Base Case');
  });

  it('displays jargon, dense sentences, textbook verbatim alerts, and beginner questions in the UI', () => {
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const explanationInput = screen.getByLabelText(/Your plain-language explanation/i);

    // Type text containing textbook wording, dense phrasing, and technical terms
    const longVerbatim =
      'A recursive function calls itself until reaching a termination condition known as the base case in order to demonstrate cellular bioenergetics and chloroplast organelles with dense descriptive sentences that exceed twenty-eight words without pauses.';
    fireEvent.change(explanationInput, {
      target: { value: longVerbatim },
    });

    expect(screen.getByText(/Unexplained domain jargon detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Overly dense sentences/i)).toBeInTheDocument();
    expect(screen.getByText(/Textbook phrasing detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Beginner Follow-up Questions/i)).toBeInTheDocument();
  });

  it('renders gracefully when lesson and module are omitted', () => {
    render(<FeynmanTechniquePanel program={mockProgram} />);

    expect(screen.getByText('Computer Science')).toBeInTheDocument();
    expect(screen.getByText(/The Feynman Technique · Teach a Beginner/i)).toBeInTheDocument();
  });

  it('keeps the lesson closed until the learner has explained it, then checks the explanation against it', () => {
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const compare = screen.getByRole('button', { name: /Compare with the lesson/i });
    expect(compare).toBeDisabled();
    expect(screen.queryByText(/termination condition known as the base case/)).not.toBeInTheDocument();

    const explanationInput = screen.getByLabelText(/Your plain-language explanation/i);
    fireEvent.change(explanationInput, { target: { value: 'It keeps calling itself on a smaller problem.' } });
    fireEvent.click(compare);

    // The lesson's point opens beside the explanation, which can no longer be edited from it.
    expect(screen.getByText(/termination condition known as the base case/)).toBeInTheDocument();
    expect(explanationInput).toHaveAttribute('readonly');
    expect(screen.getByRole('status')).toHaveTextContent('0 of 1 covered');

    expect(screen.queryByTestId('review-card')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Make a recall card/i }));
    expect(screen.getByTestId('review-card')).toBeInTheDocument();
    expect(reviewCard).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'prog-1',
      lessonId: 'l-1',
      front: 'Call Stack and Base Case',
      back: 'A recursive function calls itself until reaching a termination condition known as the base case.',
    }));

    fireEvent.click(screen.getByRole('checkbox', { name: /Call Stack and Base Case/i }));
    expect(screen.getByRole('status')).toHaveTextContent('1 of 1 covered');
    expect(screen.queryByTestId('review-card')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Make a recall card/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Explain it again from memory/i }));
    expect(screen.queryByText(/termination condition known as the base case/)).not.toBeInTheDocument();
    expect(explanationInput).not.toHaveAttribute('readonly');
  });

  it('closes the comparison when another concept is chosen', async () => {
    const user = userEvent.setup();
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);
    fireEvent.change(screen.getByLabelText(/Your plain-language explanation/i), { target: { value: 'It calls itself.' } });
    fireEvent.click(screen.getByRole('button', { name: /Compare with the lesson/i }));
    expect(screen.getByRole('status')).toHaveTextContent('0 of 1 covered');

    await user.selectOptions(screen.getByLabelText(/Choose concept to teach/i), 'Call Stack and Base Case');
    expect(screen.getByRole('button', { name: /Compare with the lesson/i })).toBeEnabled();
  });
});
