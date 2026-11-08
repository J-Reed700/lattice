import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CriteriaList, type CriterionItem } from './CriteriaList';

describe('CriteriaList', () => {
  it('renders points when a score is awarded', () => {
    const items: CriterionItem[] = [
      {
        id: 'c1',
        title: 'Problem solving',
        description: 'Understood and clarified the problem, chose a sound approach.',
        state: 'met',
        points: 3,
        maxPoints: 4,
      },
    ];

    render(<CriteriaList items={items} />);

    expect(screen.getByText('Problem solving')).toBeInTheDocument();
    expect(screen.getByText(/3\s*\/\s*4/)).toBeInTheDocument();
  });

  it('renders "Not enough evidence" when state is insufficient_evidence', () => {
    const items: CriterionItem[] = [
      {
        id: 'c2',
        title: 'Testing',
        description: 'Wrote examples before coding and tested edge cases.',
        state: 'insufficient_evidence',
        points: null,
      },
    ];

    render(<CriteriaList items={items} />);

    expect(screen.getByText('Testing')).toBeInTheDocument();
    expect(screen.getByText('Not enough evidence')).toBeInTheDocument();
  });

  it('renders observation, quote, and next step when provided', () => {
    const items: CriterionItem[] = [
      {
        id: 'c3',
        title: 'Communication',
        state: 'partial',
        points: 2,
        maxPoints: 4,
        observation: 'Explained time complexity clearly but skipped trade-offs.',
        quote: 'We could use a hash map to reduce lookups to O(1).',
        nextStep: 'Compare space complexity trade-offs before settling on an approach.',
      },
    ];

    render(<CriteriaList items={items} />);

    expect(screen.getByText('Communication')).toBeInTheDocument();
    expect(screen.getByText('Explained time complexity clearly but skipped trade-offs.')).toBeInTheDocument();
    expect(screen.getByText(/We could use a hash map to reduce lookups to O\(1\)\./)).toBeInTheDocument();
    expect(screen.getByText(/Compare space complexity trade-offs before settling on an approach\./)).toBeInTheDocument();
  });

  it('renders icon and text labels for every criterion state', () => {
    const items: CriterionItem[] = [
      { id: '1', title: 'Criterion 1', state: 'met', points: 4, maxPoints: 4 },
      { id: '2', title: 'Criterion 2', state: 'partial', points: 2, maxPoints: 4 },
      { id: '3', title: 'Criterion 3', state: 'missed', points: 0, maxPoints: 4 },
      { id: '4', title: 'Criterion 4', state: 'insufficient_evidence', points: null },
      { id: '5', title: 'Criterion 5', state: 'pending' },
    ];

    render(<CriteriaList items={items} />);

    expect(screen.getByText('Met')).toBeInTheDocument();
    expect(screen.getByText('Partial')).toBeInTheDocument();
    expect(screen.getByText('Missed')).toBeInTheDocument();
    expect(screen.getByText('Not enough evidence')).toBeInTheDocument();
    expect(screen.getByText('Pending')).toBeInTheDocument();
  });

  it('renders open by default with title, subtitle, and footer', () => {
    const items: CriterionItem[] = [
      { id: '1', title: 'Code Quality', state: 'met', points: 4, maxPoints: 4 },
    ];

    const { container } = render(
      <CriteriaList
        title="Debrief Criteria"
        subtitle="Evaluated from your coding session"
        items={items}
        footer={<button type="button">Next Step</button>}
      />,
    );

    expect(screen.getByRole('heading', { name: 'Debrief Criteria' })).toBeInTheDocument();
    expect(screen.getByText('Evaluated from your coding session')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next Step' })).toBeInTheDocument();

    const details = container.querySelector('details');
    if (details) {
      expect(details).toHaveAttribute('open');
    }
  });
});
