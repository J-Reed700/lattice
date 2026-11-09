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

  it('shows the weight of a criterion that is not graded yet, with no icon and no score', () => {
    render(
      <CriteriaList
        criteria={[
          { id: 'u1', title: 'Covers the cases', description: 'Name what you tried.', maxPoints: 3, evidence: [] },
          { id: 'u2', title: 'Reads clearly', maxPoints: 1, evidence: [] },
        ]}
      />,
    );
    const first = screen.getByText('Covers the cases').closest('li') as HTMLElement;
    expect(within(first).getByText('Worth 3 points')).toBeInTheDocument();
    expect(within(first).getByText('Name what you tried.')).toBeInTheDocument();
    expect(within(first).queryByText('Not enough evidence')).not.toBeInTheDocument();
    expect(first.querySelector('[data-score] svg')).toBeNull();
    const second = screen.getByText('Reads clearly').closest('li') as HTMLElement;
    expect(within(second).getByText('Worth 1 point')).toBeInTheDocument();
    expect(screen.queryByText('Not enough evidence')).not.toBeInTheDocument();
  });

  it('shows an outcome by its label and level, with an icon and no points', () => {
    render(
      <CriteriaList
        criteria={[
          { id: 'o1', title: 'Names the cause', outcome: { label: 'Met', level: 'met' }, evidence: [] },
          { id: 'o2', title: 'Tests it', outcome: { label: 'Partly met', level: 'partly' }, evidence: [] },
          { id: 'o3', title: 'Explains why', outcome: { label: 'Not met', level: 'not_met' }, evidence: [] },
        ]}
      />,
    );
    for (const [title, label, level] of [
      ['Names the cause', 'Met', 'met'],
      ['Tests it', 'Partly met', 'partly'],
      ['Explains why', 'Not met', 'not_met'],
    ]) {
      const row = screen.getByText(title).closest('li') as HTMLElement;
      const score = row.querySelector('[data-score]') as HTMLElement;
      expect(score).toHaveTextContent(label);
      expect(score).toHaveAttribute('data-level', level);
      expect(score.querySelector('svg')).not.toBeNull();
      expect(within(row).queryByText(/ of /)).not.toBeInTheDocument();
    }
  });
});
