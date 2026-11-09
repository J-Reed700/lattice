import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CriteriaList, type CriterionView } from './CriteriaList';

const criteria: CriterionView[] = [
  {
    id: 'c1',
    title: 'Explains the approach',
    points: 3,
    maxPoints: 4,
    evidence: [
      { kind: 'quote', text: 'I would use a hash map' },
      { kind: 'lines', path: 'solution.py', start: 3, end: 5 },
      { kind: 'run', label: 'Run 2' },
    ],
    feedback: 'Clear and specific.',
    nextStep: 'State the cost out loud.',
  },
  { id: 'c2', title: 'Tests edge cases', points: null, maxPoints: 4, evidence: [] },
];

describe('CriteriaList', () => {
  it('shows points, evidence, feedback and the next step without opening anything', () => {
    render(<CriteriaList criteria={criteria} heading="Against the criteria" />);
    expect(screen.getByRole('heading', { name: 'Against the criteria' })).toBeInTheDocument();
    const first = screen.getByText('Explains the approach').closest('li') as HTMLElement;
    expect(within(first).getByText('3 of 4')).toBeInTheDocument();
    expect(within(first).getByText(/I would use a hash map/)).toBeVisible();
    expect(within(first).getByText(/solution\.py/)).toHaveTextContent(/3.*5/);
    expect(within(first).getByText('Run 2')).toBeInTheDocument();
    expect(within(first).getByText('Clear and specific.')).toBeVisible();
    expect(within(first).getByText(/State the cost out loud/)).toBeVisible();
  });

  it('says "Not enough evidence" in text for a criterion without points', () => {
    render(<CriteriaList criteria={criteria} />);
    const second = screen.getByText('Tests edge cases').closest('li') as HTMLElement;
    expect(within(second).getByText('Not enough evidence')).toBeVisible();
    expect(within(second).queryByText(/of 4/)).not.toBeInTheDocument();
  });
});
