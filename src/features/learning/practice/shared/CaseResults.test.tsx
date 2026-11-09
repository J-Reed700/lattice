import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CaseResults, caseSummary, type CaseResultView } from './CaseResults';

const cases: CaseResultView[] = [
  { id: 'a', category: 'a typical input', status: 'passed' },
  {
    id: 'b',
    category: 'an input with duplicates',
    status: 'failed',
    args: ['[1, 1]'],
    expected: '[1]',
    actual: '[1, 1, 1]',
  },
  { id: 'c', category: 'an empty input', status: 'passed' },
  { id: 'd', category: 'a very large input', status: 'timed_out' },
  { id: 'e', category: 'a hidden edge', status: 'failed', hidden: true },
];

describe('caseSummary', () => {
  it('counts the passes and names the failing categories', () => {
    expect(caseSummary(cases)).toBe(
      '2 of 5 passed. Failing: an input with duplicates, a very large input, a hidden edge',
    );
  });

  it('has no Failing part when everything passed', () => {
    expect(caseSummary(cases.slice(0, 1))).toBe('1 of 1 passed');
  });
});

describe('CaseResults', () => {
  it('announces the summary in a status region', () => {
    render(<CaseResults cases={cases} />);
    expect(screen.getByRole('status')).toHaveTextContent(caseSummary(cases));
  });

  it('lists failures before passes', () => {
    render(<CaseResults cases={cases} />);
    const order = screen.getAllByRole('listitem').map((row) => row.getAttribute('data-case-id'));
    expect(order.slice(0, 3)).toEqual(['b', 'd', 'e']);
    expect(order.slice(3)).toEqual(['a', 'c']);
  });

  it('shows input, expected and actual for a failing case, and only the category for a hidden one', () => {
    render(<CaseResults cases={cases} />);
    const failing = screen.getByText('an input with duplicates').closest('li') as HTMLElement;
    expect(within(failing).getByText('[1, 1]', { selector: 'code' })).toBeInTheDocument();
    expect(within(failing).getByText('[1]', { selector: 'code' })).toBeInTheDocument();
    expect(within(failing).getByText('[1, 1, 1]', { selector: 'code' })).toBeInTheDocument();
    const hidden = screen.getByText('a hidden edge').closest('li') as HTMLElement;
    expect(within(hidden).getByText(/kept back/i)).toBeInTheDocument();
    expect(within(hidden).queryByText(/Expected/)).not.toBeInTheDocument();
  });

  it('shows the empty text when there are no cases', () => {
    render(<CaseResults cases={[]} emptyText="Nothing has run yet." />);
    expect(screen.getByText('Nothing has run yet.')).toBeInTheDocument();
  });
});
