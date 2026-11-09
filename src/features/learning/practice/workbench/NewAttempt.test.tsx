import type { ComponentProps } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { NewAttempt } from './NewAttempt';

function setup(overrides: Partial<ComponentProps<typeof NewAttempt>> = {}) {
  const props = { choice: 'explore' as const, onChoose: vi.fn(), onStart: vi.fn(), starting: false, busy: false, ...overrides };
  render(<NewAttempt {...props} />);
  return props;
}

describe('NewAttempt', () => {
  it('names the three help levels Full help, Hints only and No help', () => {
    setup();
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: /Explore.*Full help/s })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Practice.*Hints only/s })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Demonstrate.*No help/s })).toBeInTheDocument();
    for (const old of ['Open aid', 'Guided aid', 'Independent']) expect(screen.queryByText(old)).not.toBeInTheDocument();
  });

  it('offers every mode, with the chosen one checked, and reports a change', async () => {
    const props = setup({ choice: 'practice' });
    expect(screen.getByRole('radio', { name: /Practice/ })).toBeChecked();
    await userEvent.setup().click(screen.getByRole('radio', { name: /Demonstrate/ }));
    expect(props.onChoose).toHaveBeenCalledWith('demonstrate');
  });

  it('starts an attempt', async () => {
    const props = setup();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Start an attempt' }));
    expect(props.onStart).toHaveBeenCalledTimes(1);
  });

  it('disables Start with its reason beneath while another action runs', () => {
    setup({ busy: true });
    expect(screen.getByRole('button', { name: 'Start an attempt' })).toBeDisabled();
    expect(screen.getByText('Another action is in progress.')).toBeVisible();
  });

  it('shows Starting… while the attempt is being made', () => {
    setup({ busy: true, starting: true });
    expect(screen.getByRole('button', { name: 'Starting…' })).toBeDisabled();
  });
});
