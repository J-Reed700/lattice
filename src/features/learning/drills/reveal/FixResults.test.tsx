import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { FixResults } from '@/features/learning/drills/reveal/FixResults';
import { rankIssues } from '@/features/learning/drills/reveal/revealPresentation';
import { FIXES, ISSUES } from '@/features/learning/drills/reveal/revealTestData';
import type { IssueFix, SuiteVerdict } from '@/lib/bindings';

const ranked = rankIssues(ISSUES);

const fixed = (suite: SuiteVerdict, patch: Partial<IssueFix> = {}): IssueFix => ({
  issue: 1,
  outcome: 'fixed',
  decidedBy: 'test',
  reason: 'The test that shows this issue passes on your files.',
  output: null,
  fallback: null,
  gradedBeside: [],
  suite,
  ...patch,
});

const show = (fixes: IssueFix[]) => render(<FixResults fixes={fixes} ranked={ranked} />);

describe('FixResults', () => {
  it('shows a fixed issue, and a suite that passed on the learner’s files alone', () => {
    const { container } = show([fixed({ status: 'passed', ranBeside: [] })]);
    const row = screen.getByRole('article', { name: 'Issue 2: Wrong operator or comparison' });
    expect(within(row).getByText('Fixed')).toBeInTheDocument();
    expect(within(row).getByText('Decided by the issue’s test')).toBeInTheDocument();
    expect(within(row).getByText('The test that shows this issue passes on your files.')).toBeInTheDocument();
    expect(screen.getByText('The hidden suite passes on your files: nothing that worked is broken.')).toBeInTheDocument();
    expect(screen.getByText('It ran on your files as you left them.')).toBeInTheDocument();
    expect(container.querySelector('details')).toBeNull();
  });

  it('says when the source passage decided an issue', () => {
    show([fixed({ status: 'not_run', why: 'x' }, { decidedBy: 'source' })]);
    expect(screen.getByText('Decided by the source passage')).toBeInTheDocument();
  });

  it('says what a passing suite ran with', () => {
    show([fixed({ status: 'passed', ranBeside: [0] })]);
    expect(
      screen.getByText('It ran with the fixes of the issues you left: Magic number (issue 3).'),
    ).toBeInTheDocument();
  });

  it('says when the hidden suite did not run, and why', () => {
    show([fixed({ status: 'not_run', why: 'This drill has no hidden suite.' })]);
    expect(screen.getByText('The hidden suite did not run.')).toBeInTheDocument();
    expect(screen.getByText('This drill has no hidden suite.')).toBeInTheDocument();
  });

  it('shows the suite’s output itself when no issue carries it', () => {
    show([fixed({ status: 'failed', output: 'boom', ranBeside: [] })]);
    expect(screen.getByText('The hidden suite fails on your files: your changes broke something that worked.')).toBeInTheDocument();
    const output = screen.getByText('Suite output').closest('details');
    expect(output).not.toHaveAttribute('open');
    expect(within(output as HTMLElement).getByText('boom')).not.toBeVisible();
  });

  it('shows the suite’s output once when an issue it decided carries it', () => {
    show([fixed({ status: 'failed', output: 'boom', ranBeside: [] }, { outcome: 'partly', decidedBy: 'suite', output: 'boom' })]);
    expect(screen.getAllByText('Suite output')).toHaveLength(1);
    expect(within(screen.getByRole('article')).getByText('Suite output')).toBeInTheDocument();
  });

  it('shows every result of a fix in the page’s order', () => {
    show(FIXES);
    expect(screen.getAllByRole('article').map((row) => within(row).getByRole('heading').textContent)).toEqual([
      'Issue 1: Stale state',
      'Issue 2: Wrong operator or comparison',
      'Issue 3: Magic number',
    ]);
  });

  it('names an issue the page does not have by its place in the key', () => {
    show([fixed({ status: 'passed', ranBeside: [] }, { issue: 8 })]);
    expect(screen.getByRole('heading', { name: 'Issue 9' })).toBeInTheDocument();
  });

  it('shows nothing for a fix with no results', () => {
    const { container } = show([]);
    expect(container).toBeEmptyDOMElement();
  });
});
