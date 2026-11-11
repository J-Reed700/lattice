import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  analyzeFeynmanExplanation,
  FeynmanTechniquePanel,
  lessonPoints,
} from '@/features/learning/practice/FeynmanTechniquePanel';
import type { LearningLessonDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

describe('analyzeFeynmanExplanation', () => {
  const sampleLesson: LearningLessonDto = {
    id: 'lesson-1',
    title: 'Photosynthesis Fundamentals',
    objective: 'Understand how chloroplasts transform solar radiation into glucose',
    estimatedMinutes: 20,
    preparation: 'ready',
    completed: false,
    blocks: [
      {
        kind: 'explanation',
        title: 'The Chloroplast Engine',
        body: 'Inside plant cells, chloroplasts contain chlorophyll molecules that absorb photons and synthesize adenosine triphosphate to produce carbohydrates.',
        sourceIds: ['s1'],
      },
    ],
  };

  it('rates short explanation (< 15 words) as Needs More Explanation', () => {
    const analysis = analyzeFeynmanExplanation('Plants make food from the sun.', '', sampleLesson);
    expect(analysis.rating).toBe('Needs More Explanation');
    expect(analysis.wordCount).toBeLessThan(15);
  });

  it('flags unexplained domain jargon when no plain-language cues are used', () => {
    const explanation =
      'The chloroplasts take photons and synthesize adenosine and carbohydrates without any special tools in the leafy structure of the plant.';
    const analysis = analyzeFeynmanExplanation(explanation, '', sampleLesson);

    expect(analysis.flaggedJargon.length).toBeGreaterThan(0);
    expect(analysis.flaggedJargon).toContain('chloroplasts');
    expect(analysis.rating).toBe('Has Unexplained Jargon');
    expect(analysis.beginnerQuestions.some((q) => q.includes('chloroplasts'))).toBe(true);
  });

  it('does not flag jargon when explained using plain-language cues', () => {
    const explanation =
      'Plants have tiny solar factories called chloroplasts, which means tiny chambers inside leaves. Sunlight turns into sweet sugar because the cell needs food.';
    const analysis = analyzeFeynmanExplanation(explanation, 'It works like a solar bakery.', sampleLesson);

    expect(analysis.flaggedJargon).not.toContain('chloroplasts');
    expect(analysis.hasAnalogy).toBe(true);
    expect(analysis.denseSentences).toHaveLength(0);
    expect(analysis.rating).toBe('Simple & Clear');
  });

  it('flags overly dense sentences (> 28 words)', () => {
    const longSentence =
      'When the sun rises in the morning and shines its bright rays onto the green leaves of tall trees standing in the open forest, the cellular machinery inside every single plant begins absorbing each individual packet of light energy and converts it through a sequence of chemical reactions into sweet nourishing fuel.';
    const analysis = analyzeFeynmanExplanation(longSentence, '', sampleLesson);

    expect(analysis.denseSentences.length).toBeGreaterThan(0);
    expect(analysis.denseSentences[0]).toBe(longSentence);
    expect(analysis.rating).toBe('Has Unexplained Jargon');
  });

  it('detects textbook verbatim overlap against lesson blocks', () => {
    const copiedText =
      'Inside plant cells chloroplasts contain chlorophyll molecules that absorb photons and synthesize adenosine triphosphate to produce carbohydrates in every single leaf on the branch.';
    const analysis = analyzeFeynmanExplanation(copiedText, '', sampleLesson);

    expect(analysis.verbatimOverlapPercent).toBeGreaterThan(20);
    expect(analysis.verbatimPhrases.length).toBeGreaterThan(0);
    expect(analysis.rating).toBe('Has Unexplained Jargon');
  });

  it('generates follow-up questions for missing analogy and unexplained terms', () => {
    const analysis = analyzeFeynmanExplanation(
      'The leaf uses chloroplasts to trap sunlight and turn it into basic sugars for cell nourishment.',
      '',
      sampleLesson,
      'Photosynthesis Fundamentals'
    );

    expect(analysis.beginnerQuestions.some((q) => q.includes('analogy'))).toBe(true);
  });
});

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

  it('persists explanation to localStorage, handles manual save, and resets save feedback timer', () => {
    vi.useFakeTimers();
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    const explanationInput = screen.getByLabelText(/Your plain-language explanation/i);
    fireEvent.change(explanationInput, {
      target: { value: 'Persisted plain explanation text.' },
    });

    const saveButton = screen.getByRole('button', { name: /Save draft/i });
    fireEvent.click(saveButton);

    expect(screen.getByText('Saved')).toBeInTheDocument();
    expect(localStorage.getItem('feynman_workbench_prog-1_l-1')).toContain('Persisted plain explanation text.');

    act(() => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.queryByText('Saved')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('loads previously stored draft from localStorage on mount', () => {
    localStorage.setItem(
      'feynman_workbench_prog-1_l-1',
      JSON.stringify({
        explanation: 'Stored explanation loaded on mount.',
        analogy: 'Like a recipe card.',
        readAloud: true,
        concept: 'Call Stack and Base Case',
      })
    );

    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} />);

    expect(screen.getByDisplayValue('Stored explanation loaded on mount.')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Like a recipe card.')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /Speak out loud test/i })).toBeChecked();
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
    const onDraftMissedConcept = vi.fn();
    render(<FeynmanTechniquePanel program={mockProgram} lesson={mockLesson} module={mockModule} onDraftMissedConcept={onDraftMissedConcept} />);

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

    fireEvent.click(screen.getByRole('button', { name: /Make a recall card/i }));
    expect(onDraftMissedConcept).toHaveBeenCalledWith(
      'Call Stack and Base Case',
      'A recursive function calls itself until reaching a termination condition known as the base case.',
    );
    expect(screen.getByRole('button', { name: /Added to recall cards/i })).toBeDisabled();

    fireEvent.click(screen.getByRole('checkbox', { name: /Call Stack and Base Case/i }));
    expect(screen.getByRole('status')).toHaveTextContent('1 of 1 covered');
    expect(screen.queryByRole('button', { name: /Added to recall cards|Make a recall card/i })).not.toBeInTheDocument();

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

describe('lessonPoints', () => {
  const block = (kind: LearningLessonDto['blocks'][number]['kind'], title: string, body: string) => ({ kind, title, body, sourceIds: [] });
  const lesson = {
    id: 'l', title: 'Caching', objective: 'Choose an eviction policy', estimatedMinutes: 20, preparation: 'ready', completed: false,
    blocks: [
      block('explanation', 'What eviction is', '- Space is bounded\n- Something must leave when it fills'),
      block('worked_example', 'An LRU cache, step by step', 'Insert A, B, C; touch A; insert D: B leaves.'),
      block('check', 'Your turn', 'Which entry leaves?'),
      block('guided_practice', 'Try it', 'Build one.'),
    ],
  } as LearningLessonDto;

  it('checks a named section against its key points, or the whole section when it is prose', () => {
    expect(lessonPoints('What eviction is', lesson)).toEqual([
      { title: 'Space is bounded', body: '' },
      { title: 'Something must leave when it fills', body: '' },
    ]);
    expect(lessonPoints('An LRU cache, step by step', lesson)).toEqual([
      { title: 'An LRU cache, step by step', body: 'Insert A, B, C; touch A; insert D: B leaves.' },
    ]);
  });

  it('checks the lesson as a whole against its teaching sections only, and a module against its outcomes', () => {
    expect(lessonPoints('Caching', lesson).map((point) => point.title)).toEqual(['What eviction is', 'An LRU cache, step by step']);
    expect(lessonPoints('Anything', null, { outcomes: ['Pick a policy for a workload'] } as LearningModuleDto)).toEqual([
      { title: 'Pick a policy for a workload', body: '' },
    ]);
  });
});
