import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { lastChosen, StartSentence, type ChoiceSlot } from './StartSentence';

const slots: ChoiceSlot[] = [
  {
    id: 'difficulty',
    label: 'Difficulty',
    options: [
      { value: 'easy', label: 'easy' },
      { value: 'hard', label: 'hard' },
    ],
    defaultValue: 'easy',
  },
  { id: 'setting', label: 'Setting', options: [{ value: 'coached', label: 'coached' }], defaultValue: 'coached' },
];
const parts = ['A ', { slot: 'difficulty' }, ' problem, ', { slot: 'setting' }, '.'];

function show(programId = 'course-1', onChange = vi.fn()) {
  return render(<StartSentence programId={programId} cardId="code" legend="Code options" slots={slots} parts={parts} describedBy="promise" onChange={onChange} />);
}

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('StartSentence', () => {
  it('puts its menus in a fieldset named by the legend and described by the card', () => {
    show();
    const group = screen.getByRole('group', { name: 'Code options' });
    expect(group.tagName).toBe('FIELDSET');
    expect(group).toHaveAttribute('aria-describedby', 'promise');
    expect(screen.getByRole('combobox', { name: 'Difficulty' })).toHaveValue('easy');
  });

  it('shows a slot with one option as plain text, not a menu', () => {
    show();
    expect(screen.queryByRole('combobox', { name: 'Setting' })).not.toBeInTheDocument();
    expect(screen.getByText('coached')).toBeVisible();
  });

  it('remembers a choice for the course and not for another', async () => {
    const user = userEvent.setup();
    const first = show('course-1');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Difficulty' }), 'hard');
    expect(lastChosen('course-1', 'code', 'difficulty')).toBe('hard');
    first.unmount();

    const second = show('course-1');
    expect(screen.getByRole('combobox', { name: 'Difficulty' })).toHaveValue('hard');
    second.unmount();
    show('course-2');
    expect(screen.getByRole('combobox', { name: 'Difficulty' })).toHaveValue('easy');
  });

  it('falls back to the default when the remembered value is no longer offered', () => {
    localStorage.setItem('lattice:practice:sentence:course-1:code', JSON.stringify({ difficulty: 'impossible' }));
    show();
    expect(screen.getByRole('combobox', { name: 'Difficulty' })).toHaveValue('easy');
  });

  it('still works when storage throws', async () => {
    const user = userEvent.setup();
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    const onChange = vi.fn();
    show('course-1', onChange);
    await user.selectOptions(screen.getByRole('combobox', { name: 'Difficulty' }), 'hard');
    expect(onChange).toHaveBeenLastCalledWith({ difficulty: 'hard', setting: 'coached' });
  });
});
