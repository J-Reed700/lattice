import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type ChoiceSlot, StartSentence } from './StartSentence';

describe('StartSentence', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('renders interactive choices inside a fieldset with aria-describedby (P3-20)', () => {
    const slots: ChoiceSlot[] = [
      {
        id: 'difficulty',
        label: 'Difficulty',
        value: 'easy',
        options: [
          { value: 'easy', label: 'easy', description: 'Beginner friendly' },
          { value: 'medium', label: 'medium', description: 'Intermediate level' },
          { value: 'hard', label: 'hard', description: 'Challenging constraints' },
        ],
        ariaDescribedBy: 'diff-help',
      },
      {
        id: 'language',
        label: 'Language',
        value: 'Python',
        options: [
          { value: 'Python', label: 'Python' },
          { value: 'Rust', label: 'Rust' },
        ],
      },
    ];

    render(
      <StartSentence
        programId="course-1"
        cardId="code"
        slots={slots}
        sentencePrefix="An "
        sentenceSuffix=" problem."
      />,
    );

    const fieldset = screen.getByRole('group');
    expect(fieldset).toBeInTheDocument();

    const diffButton = screen.getByRole('button', { name: /difficulty/i });
    expect(diffButton).toHaveAttribute('aria-describedby', expect.stringContaining('diff-help'));
  });

  it('renders options as an interactive sentence where clicking choices triggers updates', async () => {
    const user = userEvent.setup();
    const handleChange = vi.fn();

    const slots: ChoiceSlot[] = [
      {
        id: 'difficulty',
        label: 'Difficulty',
        value: 'easy',
        options: [
          { value: 'easy', label: 'easy' },
          { value: 'medium', label: 'medium' },
        ],
      },
      {
        id: 'setting',
        label: 'Setting',
        value: 'coached',
        options: [
          { value: 'coached', label: 'coached' },
          { value: 'mock', label: 'mock' },
        ],
      },
    ];

    render(
      <StartSentence
        programId="course-1"
        cardId="code"
        slots={slots}
        sentencePrefix="An "
        sentenceSuffix=" problem."
        onChange={handleChange}
      />,
    );

    const diffButton = screen.getByRole('button', { name: /difficulty/i });
    expect(diffButton).toHaveTextContent('easy');

    await user.click(diffButton);

    const mediumOption = screen.getByRole('option', { name: /medium/i });
    await user.click(mediumOption);

    expect(handleChange).toHaveBeenCalledWith('difficulty', 'medium');
  });

  it('remembers choices per course independently', async () => {
    const user = userEvent.setup();

    const slots: ChoiceSlot[] = [
      {
        id: 'setting',
        label: 'Setting',
        value: 'coached',
        options: [
          { value: 'coached', label: 'coached' },
          { value: 'mock', label: 'mock' },
        ],
      },
    ];

    // Setup for Course Alpha
    const { unmount: unmountAlpha } = render(
      <StartSentence
        programId="course-alpha"
        cardId="code"
        slots={slots}
      />,
    );

    const alphaBtn = screen.getByRole('button', { name: /setting/i });
    expect(alphaBtn).toHaveTextContent('coached');

    await user.click(alphaBtn);
    const mockOpt = screen.getByRole('option', { name: /mock/i });
    await user.click(mockOpt);
    unmountAlpha();

    // Re-render Course Alpha: remembers 'mock'
    const { unmount: unmountAlpha2 } = render(
      <StartSentence
        programId="course-alpha"
        cardId="code"
        slots={slots}
      />,
    );
    expect(screen.getByRole('button', { name: /setting/i })).toHaveTextContent('mock');
    unmountAlpha2();

    // Render Course Beta: stays at default 'coached'
    render(
      <StartSentence
        programId="course-beta"
        cardId="code"
        slots={slots}
      />,
    );
    expect(screen.getByRole('button', { name: /setting/i })).toHaveTextContent('coached');
  });
});
