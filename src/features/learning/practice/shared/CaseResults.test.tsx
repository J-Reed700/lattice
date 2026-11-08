import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CaseResults, type CaseRowItem } from './CaseResults';

describe('CaseResults', () => {
  it('renders failures and errors before passed cases', () => {
    const cases: CaseRowItem[] = [
      { id: 'c1', name: 'Example 1', status: 'passed' },
      { id: 'c2', name: 'an input with duplicates', status: 'failed' },
      { id: 'c3', name: 'an empty input', status: 'error' },
      { id: 'c4', name: 'Example 2', status: 'passed' },
      { id: 'c5', name: 'large scale test', status: 'timed_out' },
    ];

    const { container } = render(<CaseResults cases={cases} />);

    // Get all elements containing the case names in document order
    const caseElements = container.querySelectorAll('[data-case-name]');
    const renderedNames = Array.from(caseElements).map((el) => el.getAttribute('data-case-name') || el.textContent);

    // Failing, error, and timed_out cases must come before passed cases
    const duplicatesIndex = renderedNames.findIndex((name) => name?.includes('an input with duplicates'));
    const emptyIndex = renderedNames.findIndex((name) => name?.includes('an empty input'));
    const timedOutIndex = renderedNames.findIndex((name) => name?.includes('large scale test'));
    const example1Index = renderedNames.findIndex((name) => name?.includes('Example 1'));
    const example2Index = renderedNames.findIndex((name) => name?.includes('Example 2'));

    expect(duplicatesIndex).toBeGreaterThanOrEqual(0);
    expect(emptyIndex).toBeGreaterThanOrEqual(0);
    expect(timedOutIndex).toBeGreaterThanOrEqual(0);
    expect(example1Index).toBeGreaterThanOrEqual(0);
    expect(example2Index).toBeGreaterThanOrEqual(0);

    expect(duplicatesIndex).toBeLessThan(example1Index);
    expect(duplicatesIndex).toBeLessThan(example2Index);
    expect(emptyIndex).toBeLessThan(example1Index);
    expect(emptyIndex).toBeLessThan(example2Index);
    expect(timedOutIndex).toBeLessThan(example1Index);
    expect(timedOutIndex).toBeLessThan(example2Index);
  });

  it('announces the summary in a role="status" live region', () => {
    const cases: CaseRowItem[] = [
      { id: 'c1', name: 'Example 1', status: 'passed' },
      { id: 'c2', name: 'an input with duplicates', status: 'failed' },
    ];

    render(
      <CaseResults
        cases={cases}
        summary="1 of 2 passed. Failing: an input with duplicates"
      />,
    );

    const liveRegion = screen.getByRole('status');
    expect(liveRegion).toHaveTextContent('1 of 2 passed. Failing: an input with duplicates');
  });

  it('displays case details including args, expected, actual, printed, and error', () => {
    const cases: CaseRowItem[] = [
      {
        id: 'c1',
        name: 'duplicate element removal',
        status: 'failed',
        args: ['[1, 2, 2, 3]', '2'],
        expected: '[1, 3]',
        actual: '[1, 2, 3]',
        printed: 'found duplicate item at index 1',
        error: 'AssertionError: expected [1, 3] but got [1, 2, 3]',
      },
    ];

    render(<CaseResults cases={cases} />);

    expect(screen.getByText('duplicate element removal')).toBeInTheDocument();
    expect(screen.getByText(/\[1, 2, 2, 3\]/)).toBeInTheDocument();
    expect(screen.getByText(/\[1, 3\]/)).toBeInTheDocument();
    expect(screen.getByText(/\[1, 2, 3\]/)).toBeInTheDocument();
    expect(screen.getByText(/found duplicate item at index 1/)).toBeInTheDocument();
    expect(screen.getByText(/AssertionError: expected \[1, 3\] but got \[1, 2, 3\]/)).toBeInTheDocument();
  });

  it('uses theme token classes and does not hardcode hex palette colors', () => {
    const cases: CaseRowItem[] = [
      { id: 'c1', name: 'Example test', status: 'passed' },
    ];

    const { container } = render(<CaseResults cases={cases} />);

    // Must not use raw hex colors like #211f1b (from old RunOutput)
    expect(container.innerHTML).not.toContain('#211f1b');
    expect(container.innerHTML).not.toMatch(/#[0-9a-fA-F]{3,6}/);

    // Must use theme token classes
    const section = container.querySelector('section');
    expect(section).toBeInTheDocument();
    expect(section?.className).toMatch(/bg-(?:surface|surface-sunken|surface-raised|bg)/);
  });

  it('renders stdout and stderr when provided', () => {
    const cases: CaseRowItem[] = [
      { id: 'c1', name: 'Example test', status: 'passed' },
    ];

    render(
      <CaseResults
        cases={cases}
        stdout="Running test cases...\nAll checks completed."
        stderr="DeprecationWarning: builtin call"
      />,
    );

    expect(screen.getByText(/Running test cases\.\.\./)).toBeInTheDocument();
    expect(screen.getByText(/DeprecationWarning: builtin call/)).toBeInTheDocument();
  });
});
