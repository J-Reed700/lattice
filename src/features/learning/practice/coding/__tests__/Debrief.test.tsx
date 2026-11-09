import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { Debrief } from '../Debrief';
import { codingDebrief } from './fixtures';

function renderDebrief() {
  const onTryInterview = vi.fn();
  const onExit = vi.fn();
  render(<Debrief title="Pair with a sum" debrief={codingDebrief()} onExit={onExit} onTryInterview={onTryInterview} />);
  return { onTryInterview, onExit };
}

describe('Debrief', () => {
  it('shows the scorecard with its evidence, and not enough evidence as such', () => {
    renderDebrief();
    expect(screen.getByText('Problem solving')).toBeInTheDocument();
    expect(screen.getByText(/a dict remembers what I have seen/)).toBeInTheDocument();
    expect(screen.getByText('Testing')).toBeInTheDocument();
    expect(screen.getByText(/Not enough evidence/)).toBeInTheDocument();
  });

  it('shows the facts beside the scores', () => {
    renderDebrief();
    const facts = screen.getByRole('region', { name: 'What happened' });
    expect(within(facts).getByText(/1:30/)).toBeInTheDocument();
    expect(within(facts).getByText(/5:00/)).toBeInTheDocument();
    expect(within(facts).getByText(/4 runs/)).toBeInTheDocument();
    expect(within(facts).getByText(/Coached/)).toBeInTheDocument();
    expect(within(facts).getByText(/Mock/)).toBeInTheDocument();
  });

  it('gives the strong answer with the reference folded away', () => {
    renderDebrief();
    expect(screen.getByText(/Remember each value and its position/)).toBeInTheDocument();
    expect(screen.getByText('O(n) time, O(n) space')).toBeInTheDocument();
    const reference = screen.getByText(/seen = \{\}/);
    expect(reference.closest('details')).not.toHaveAttribute('open');
  });

  it('lists the cases and which misreadings the examples caught', () => {
    renderDebrief();
    expect(screen.getAllByText(/an input with duplicates/).length).toBeGreaterThan(0);
    expect(screen.getByText('Uses the same element twice')).toBeInTheDocument();
    const missed = screen.getByText('Returns values instead of positions').closest('li');
    expect(missed).toHaveTextContent(/did not catch/i);
  });

  it('names the pattern with its rating and when it comes back', () => {
    renderDebrief();
    const patterns = screen.getByRole('region', { name: 'Problem patterns' });
    expect(within(patterns).getByText('Hash map lookup')).toBeInTheDocument();
    expect(within(patterns).getByText('Good')).toBeInTheDocument();
    expect(within(patterns).getByText(/Comes back/)).toBeInTheDocument();
    expect(within(patterns).getByText(/pairs that sum to a target/)).toBeInTheDocument();
  });

  it('offers to try it under interview conditions', async () => {
    const user = userEvent.setup();
    const { onTryInterview } = renderDebrief();
    await user.click(screen.getByRole('button', { name: 'Try it under interview conditions' }));
    expect(onTryInterview).toHaveBeenCalledTimes(1);
  });

  it('names each setting used once', () => {
    renderDebrief();
    expect(screen.getAllByText(/Coached/)).toHaveLength(1);
    expect(screen.getAllByText(/Mock/)).toHaveLength(1);
  });
});
