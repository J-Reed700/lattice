import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DrillReveal, type DrillRevealProps } from '@/features/learning/drills/reveal/DrillReveal';
import {
  FIXES,
  ISSUES,
  LITERALS_AFTER,
  LITERALS_BEFORE,
  MODEL,
  OPERATOR_AFTER,
  OPERATOR_BEFORE,
  RECORD,
  STALE_AFTER,
  STALE_BEFORE,
  STALE_STATE,
  gradedResult,
  revealOf,
  ungradedResult,
} from '@/features/learning/drills/reveal/revealTestData';
import { severityLabel } from '@/features/learning/drills/severity';
import type { AttemptMode, CodeReviewResultDto } from '@/lib/bindings';

function setup(result: CodeReviewResultDto = gradedResult(), props: Partial<DrillRevealProps> = {}) {
  const onNextDrill = vi.fn();
  const onRegrade = vi.fn();
  const view = render(
    <DrillReveal result={result} onNextDrill={onNextDrill} onRegrade={onRegrade} regrading={false} {...props} />,
  );
  return { onNextDrill, onRegrade, ...view };
}

const TITLE = 'A sliding-window rate limiter';
const CONTRAST = 'The code you reviewed · The code without the issues';

const region = (name: string) => screen.getByRole('region', { name });
const card = (name: string) => screen.getByRole('article', { name });
const pane = (name: string) => screen.getByRole('group', { name });
const highlighted = (element: HTMLElement) =>
  [...element.querySelectorAll('.cm-line.cm-explorer-highlight')].map((line) => line.textContent);

afterEach(() => {
  Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
});

describe('DrillReveal, a graded review', () => {
  describe('the summary', () => {
    const figure = (label: string) => within(region(TITLE)).getByText(label, { selector: 'dt' }).nextElementSibling;

    it('counts the issues found, half found and missed, and by severity', () => {
      setup();
      expect(within(region(TITLE)).getByRole('heading', { name: 'Of the 3 issues in the code' })).toBeInTheDocument();
      expect(figure('Found')).toHaveTextContent(/^1$/);
      expect(figure('Half found')).toHaveTextContent(/^1$/);
      expect(figure('Missed')).toHaveTextContent(/^1$/);
      const tallies = within(within(region(TITLE)).getByRole('list', { name: 'By severity' }))
        .getAllByRole('listitem')
        .map((item) => item.textContent);
      expect(tallies).toEqual([
        `${severityLabel('critical')}0 of 1 found, 1 half found`,
        `${severityLabel('major')}1 of 1 found`,
        `${severityLabel('nit')}0 of 1 found`,
      ]);
    });

    it('gives the weighted recall and the precision as percents, and the extras and false positives', () => {
      setup();
      expect(figure('Weighted recall')).toHaveTextContent(/^63%/);
      expect(figure('Precision')).toHaveTextContent(/^75%/);
      expect(figure('Valid extras')).toHaveTextContent(/^1/);
      expect(figure('False positives')).toHaveTextContent(/^1$/);
    });

    it('leaves the precision out when there were no comments to measure', () => {
      setup(gradedResult({ reveal: revealOf({ score: { ...revealOf().score, precision: null } }) }));
      expect(within(region(TITLE)).queryByText('Precision', { selector: 'dt' })).toBeNull();
      expect(figure('Weighted recall')).toBeInTheDocument();
    });

    it('names the model that graded the review, when it did, and how long the learner took', () => {
      setup();
      const summary = region(TITLE);
      expect(within(summary).getByText(MODEL)).toBeInTheDocument();
      expect(summary.querySelector('time')).toHaveAttribute('datetime', '2026-11-04T15:30:00.000Z');
      expect(within(summary).getByText('1 min 30 s')).toBeInTheDocument();
    });

    it('says so when no model was recorded', () => {
      setup(gradedResult({ graderModel: null, gradedAt: null, timeSpentMs: null }));
      const summary = region(TITLE);
      expect(within(summary).getByText('Grader model not recorded')).toBeInTheDocument();
      expect(summary.querySelector('time')).toBeNull();
      expect(within(summary).queryByText(/Time spent/)).toBeNull();
    });

    it('says the review did not lead with the most severe issue, and why', () => {
      setup();
      expect(
        within(region(TITLE)).getByText(
          'Did not lead with the most severe issue: Stale state was only half found: a comment pointed at it but gave no cause, or the wrong one.',
        ),
      ).toBeInTheDocument();
    });

    it('says the review led with the most severe issue', () => {
      const issues = ISSUES.map((issue) =>
        issue.id === STALE_STATE.id ? { ...issue, outcome: 'found' as const, finding: 0 } : issue,
      );
      setup(
        gradedResult({
          reveal: revealOf({ issues, score: { ...revealOf().score, ledWithTheMostSevere: true } }),
        }),
      );
      expect(
        within(region(TITLE)).getByText('Led with the most severe issue: your first comment found Stale state.'),
      ).toBeInTheDocument();
    });

    it.each<[AttemptMode, string]>([
      ['practice', 'Result · Practice · python'],
      ['interview', 'Result · Interview · python'],
      ['rapid', 'Result · Rapid · python'],
    ])('says the mode it was practised in: %s', (mode, eyebrow) => {
      setup(gradedResult({ mode }));
      expect(within(region(TITLE)).getByText(eyebrow)).toBeInTheDocument();
    });

    it('moves focus to the result’s heading', () => {
      setup();
      expect(screen.getByRole('heading', { name: TITLE })).toHaveFocus();
    });
  });

  describe('the issues', () => {
    it('lists them most severe first, each numbered', () => {
      setup();
      const names = within(region('Issues, most severe first'))
        .getAllByRole('article')
        .map((article) => within(article).getByRole('heading', { level: 3 }).textContent);
      expect(names).toEqual(['Stale state', 'Wrong operator or comparison', 'Magic number']);
      expect(within(card('Stale state')).getByText('Issue 1')).toBeInTheDocument();
      expect(within(card('Wrong operator or comparison')).getByText('Issue 2')).toBeInTheDocument();
      expect(within(card('Magic number')).getByText('Issue 3')).toBeInTheDocument();
    });

    it.each([
      ['Wrong operator or comparison', 'Found', 'Your comment names the operator.'],
      [
        'Stale state',
        'Half found',
        'Your comment is on the right line but is about speed, not about hits that never expire.',
      ],
      ['Magic number', 'Missed', 'No comment mentions the literals.'],
    ])('shows %s as %s with the grader’s reason', (name, label, reason) => {
      setup();
      expect(within(card(name)).getByText(label)).toBeInTheDocument();
      expect(within(card(name)).getByText(reason)).toBeInTheDocument();
    });

    it('shows each issue’s severity, its lines, what happens and how to say it', () => {
      setup();
      const operator = within(card('Wrong operator or comparison'));
      expect(operator.getByText(severityLabel('major'))).toBeInTheDocument();
      expect(
        operator.getByRole('button', { name: 'Show src/limiter.py · lines 13–14 in the code' }),
      ).toHaveTextContent('src/limiter.py · lines 13–14');
      expect(operator.getByText('What happens')).toBeInTheDocument();
      expect(operator.getByText('A caller gets one call more than the limit.')).toBeInTheDocument();
      expect(operator.getByText('How to say it')).toBeInTheDocument();
      expect(
        operator.getByText('The comparison lets one call too many through. It should refuse at the limit.'),
      ).toBeInTheDocument();
      expect(
        within(card('Stale state')).getByRole('button', { name: 'Show src/limiter.py · line 12 in the code' }),
      ).toHaveTextContent('src/limiter.py · line 12');
    });

    it('shows the fix as a diff that marks the lines removed and added and leaves the rest alone', () => {
      setup();
      const diff = within(card('Wrong operator or comparison')).getByRole('group', { name: 'The fix as a diff' });
      const removed = diff.querySelectorAll('del');
      const added = diff.querySelectorAll('ins');
      expect(removed).toHaveLength(1);
      expect(removed[0]).toHaveTextContent('if len(recent) > self.limit:');
      expect(added).toHaveLength(1);
      expect(added[0]).toHaveTextContent('if len(recent) >= self.limit:');
      const kinds = [...diff.querySelectorAll('[data-kind]')].map((row) => [
        row.getAttribute('data-kind'),
        row.querySelector('[aria-hidden="true"]')?.textContent,
      ]);
      expect(kinds).toEqual([
        ['removed', '-'],
        ['added', '+'],
        ['unchanged', '\u{a0}'],
      ]);
      const unchanged = diff.querySelector('[data-kind="unchanged"]');
      expect(unchanged).toHaveTextContent('self.hits[key] = recent');
      expect(unchanged?.querySelector('ins, del')).toBeNull();
    });

    it('names the comment an issue was credited to, and none for a missed one', () => {
      setup();
      expect(
        within(card('Wrong operator or comparison')).getByRole('button', {
          name: 'Credited to your comment 1: “This lets one call more than the limit through.”',
        }),
      ).toBeInTheDocument();
      expect(
        within(card('Stale state')).getByRole('button', {
          name: 'Credited to your comment 2: “This line looks slow.”',
        }),
      ).toBeInTheDocument();
      expect(within(card('Magic number')).queryByText(/Credited to/)).toBeNull();
    });
  });

  describe('the comments', () => {
    it('lists them in the learner’s order with their lines, severity and what came of each', () => {
      setup();
      const cards = within(region('Your comments')).getAllByRole('article');
      expect(cards.map((comment) => within(comment).getByRole('heading').textContent)).toEqual([
        'Comment 1',
        'Comment 2',
        'Comment 3',
        'Comment 4',
      ]);
      const [first, second, third, fourth] = cards;
      expect(first).toHaveTextContent('src/limiter.py · line 13');
      expect(first).toHaveTextContent('This lets one call more than the limit through.');
      expect(first).toHaveTextContent(`Your severity${severityLabel('major')}`);
      expect(first).toHaveTextContent('Matches issue 2, Wrong operator or comparison');
      expect(first).toHaveTextContent('It is the changed operator.');
      expect(second).toHaveTextContent('Matches issue 1, Stale state');
      expect(second).toHaveTextContent('Right line, wrong reason.');
      expect(third).toHaveTextContent('No lines selected');
      expect(third).toHaveTextContent('Valid extra');
      expect(third).toHaveTextContent('The dictionary really does grow.');
      expect(third).not.toHaveTextContent('Your severity');
      expect(fourth).toHaveTextContent('False positive');
      expect(fourth).toHaveTextContent('A single dictionary operation needs none here.');
    });

    it('says so when the learner left none', () => {
      setup(gradedResult({ findings: [], reveal: revealOf({ findings: [] }) }));
      expect(within(region('Your comments')).getByText('You left no comments.')).toBeInTheDocument();
    });
  });

  describe('your fix', () => {
    const withFix = () => setup(gradedResult({ reveal: revealOf({ fix: FIXES }) }));

    it('is left out when the learner wrote none', () => {
      setup();
      expect(screen.queryByRole('region', { name: 'Your fix' })).toBeNull();
    });

    it('is left out when it holds no results', () => {
      setup(gradedResult({ reveal: revealOf({ fix: [] }) }));
      expect(screen.queryByRole('region', { name: 'Your fix' })).toBeNull();
    });

    it('shows each issue’s result in the page’s order, with what decided it and why', () => {
      withFix();
      const rows = within(region('Your fix')).getAllByRole('article');
      expect(rows.map((row) => within(row).getByRole('heading').textContent)).toEqual([
        'Issue 1: Stale state',
        'Issue 2: Wrong operator or comparison',
        'Issue 3: Magic number',
      ]);
      const [stale, operator, literals] = rows;
      expect(stale).toHaveTextContent('Not fixed');
      expect(stale).toHaveTextContent('Decided by the issue’s test');
      expect(stale).toHaveTextContent('The test that shows this issue still fails on your files.');
      expect(stale).toHaveTextContent(
        'Its test ran with the fixes of the issues you left: Wrong operator or comparison (issue 2) and Magic number (issue 3).',
      );
      expect(operator).toHaveTextContent('Partly');
      expect(operator).toHaveTextContent('Decided by the hidden suite');
      expect(literals).toHaveTextContent('Not fixed');
      expect(literals).toHaveTextContent('Decided by a judge');
      expect(literals).toHaveTextContent(
        'The issue’s test could not decide it, so a judge did: There is no runtime here to run the test.',
      );
    });

    it('says what the hidden suite made of the learner’s files and what it ran with', () => {
      withFix();
      const fix = within(region('Your fix'));
      expect(
        fix.getByText('The hidden suite fails on your files: your changes broke something that worked.'),
      ).toBeInTheDocument();
      expect(
        fix.getByText(
          'It ran with the fixes of the issues you left: Stale state (issue 1) and Magic number (issue 3).',
        ),
      ).toBeInTheDocument();
    });

    it('folds each issue’s output away, and shows the suite’s once', () => {
      withFix();
      const fix = within(region('Your fix'));
      const [stale, operator, literals] = fix.getAllByRole('article').map((row) => within(row));
      expect(stale.getByText('Test output').closest('details')).not.toHaveAttribute('open');
      expect(stale.getByText('AssertionError: an expired hit must not count')).not.toBeVisible();
      expect(operator.getByText('Suite output').closest('details')).not.toHaveAttribute('open');
      expect(operator.getByText('AssertionError: a call after the window must be allowed')).not.toBeVisible();
      expect(literals.queryByText(/Test output|Suite output/)).toBeNull();
      expect(fix.getAllByText('Suite output')).toHaveLength(1);
    });
  });

  describe('the contrast', () => {
    it('shows the code reviewed beside the same code without the issues', () => {
      setup();
      const reviewed = within(region(CONTRAST)).getByRole('group', { name: 'The code you reviewed' });
      const clean = within(region(CONTRAST)).getByRole('group', { name: 'The code without the issues' });
      expect(reviewed.querySelector('.cm-content')).toHaveTextContent(STALE_BEFORE.trim());
      expect(reviewed.querySelector('.cm-content')).not.toHaveTextContent(STALE_AFTER.trim());
      expect(clean.querySelector('.cm-content')).toHaveTextContent(STALE_AFTER.trim());
      expect(clean.querySelector('.cm-content')).not.toHaveTextContent(STALE_BEFORE.trim());
    });

    it('offers a tab for each file and shows the file chosen in both versions', async () => {
      const user = userEvent.setup();
      setup();
      const tabs = within(screen.getByRole('tablist', { name: 'Files' })).getAllByRole('tab');
      expect(tabs.map((tab) => tab.textContent)).toEqual(['src/limiter.py', 'src/config.py']);
      expect(tabs[0]).toHaveAttribute('aria-selected', 'true');

      await user.click(tabs[1]);
      expect(tabs[1]).toHaveAttribute('aria-selected', 'true');
      expect(pane('The code you reviewed').querySelector('.cm-content')).toHaveTextContent(LITERALS_BEFORE.trim());
      expect(pane('The code without the issues').querySelector('.cm-content')).toHaveTextContent(
        LITERALS_AFTER.trim(),
      );
    });

    it('moves between the file tabs with the arrow keys, Home and End', async () => {
      const user = userEvent.setup();
      setup();
      const [first, second] = screen.getAllByRole('tab');
      first.focus();
      await user.keyboard('{ArrowRight}');
      expect(second).toHaveAttribute('aria-selected', 'true');
      expect(second).toHaveFocus();
      await user.keyboard('{ArrowRight}');
      expect(first).toHaveAttribute('aria-selected', 'true');
      await user.keyboard('{ArrowLeft}');
      expect(second).toHaveAttribute('aria-selected', 'true');
      await user.keyboard('{Home}');
      expect(first).toHaveAttribute('aria-selected', 'true');
      await user.keyboard('{End}');
      expect(second).toHaveAttribute('aria-selected', 'true');
    });

    it('moves the highlight to an issue’s lines, in both versions, when the learner asks for them', async () => {
      const user = userEvent.setup();
      setup();
      expect(highlighted(pane('The code you reviewed'))).toEqual([]);

      await user.click(
        within(card('Wrong operator or comparison')).getByRole('button', {
          name: 'Show src/limiter.py · lines 13–14 in the code',
        }),
      );
      expect(highlighted(pane('The code you reviewed'))).toEqual([OPERATOR_BEFORE, RECORD]);
      expect(highlighted(pane('The code without the issues'))).toEqual([OPERATOR_AFTER, RECORD]);

      await user.click(
        within(card('Stale state')).getByRole('button', { name: 'Show src/limiter.py · line 12 in the code' }),
      );
      expect(highlighted(pane('The code you reviewed'))).toEqual([STALE_BEFORE]);
      expect(highlighted(pane('The code without the issues'))).toEqual([STALE_AFTER]);
    });

    it('switches to the file an issue is in', async () => {
      const user = userEvent.setup();
      setup();
      await user.click(
        within(card('Magic number')).getByRole('button', { name: 'Show src/config.py · line 8 in the code' }),
      );
      expect(screen.getByRole('tab', { name: 'src/config.py' })).toHaveAttribute('aria-selected', 'true');
      expect(highlighted(pane('The code you reviewed'))).toEqual([LITERALS_BEFORE]);
      expect(highlighted(pane('The code without the issues'))).toEqual([LITERALS_AFTER]);
      expect(screen.getByRole('status')).toHaveTextContent('Showing src/config.py · line 8');
    });

    it('does the same from the comment an issue was credited to', async () => {
      const user = userEvent.setup();
      setup();
      await user.click(screen.getByRole('button', { name: /^Credited to your comment 1:/ }));
      expect(highlighted(pane('The code you reviewed'))).toEqual([OPERATOR_BEFORE, RECORD]);
    });

    it('scrolls the contrast into view', async () => {
      const user = userEvent.setup();
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      setup();
      await user.click(
        within(card('Stale state')).getByRole('button', { name: 'Show src/limiter.py · line 12 in the code' }),
      );
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'start', behavior: 'smooth' });
      expect(scrollIntoView.mock.contexts[0]).toContainElement(region(CONTRAST));
    });

    it('scrolls without animation for a reader who prefers less motion', async () => {
      const user = userEvent.setup();
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      const original = window.matchMedia;
      window.matchMedia = ((query: string) => ({ matches: query.includes('prefers-reduced-motion'), media: query })) as typeof window.matchMedia;
      try {
        setup();
        await user.click(
          within(card('Stale state')).getByRole('button', { name: 'Show src/limiter.py · line 12 in the code' }),
        );
        expect(scrollIntoView).toHaveBeenCalledWith({ block: 'start', behavior: 'auto' });
      } finally {
        window.matchMedia = original;
      }
    });

    it('starts again from the first file for another attempt', async () => {
      const user = userEvent.setup();
      const { rerender } = setup();
      await user.click(
        within(card('Magic number')).getByRole('button', { name: 'Show src/config.py · line 8 in the code' }),
      );
      expect(screen.getByRole('tab', { name: 'src/config.py' })).toHaveAttribute('aria-selected', 'true');

      rerender(
        <DrillReveal
          result={gradedResult({ attemptId: 'attempt-2' })}
          onNextDrill={vi.fn()}
          onRegrade={vi.fn()}
          regrading={false}
        />,
      );
      expect(screen.getByRole('tab', { name: 'src/limiter.py' })).toHaveAttribute('aria-selected', 'true');
      expect(highlighted(pane('The code you reviewed'))).toEqual([]);
    });
  });

  describe('the patterns', () => {
    it('lists each pattern with how this attempt rated it', () => {
      setup();
      const items = within(region('Patterns this drill practised'))
        .getAllByRole('listitem')
        .map((item) => item.textContent);
      expect(items).toEqual(['Stale stateHard', 'Wrong operator or comparisonEasy', 'Magic numberAgain']);
    });

    it('is left out when there are no ratings', () => {
      setup(gradedResult({ reveal: revealOf({ ratings: [] }) }));
      expect(screen.queryByRole('region', { name: 'Patterns this drill practised' })).toBeNull();
    });
  });

  it('moves on when the learner asks for the next drill', async () => {
    const { onNextDrill } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Next drill' }));
    expect(onNextDrill).toHaveBeenCalledTimes(1);
  });
});

describe('DrillReveal, a review that could not be graded', () => {
  it('says why, names the model that tried, and shows nothing of the key', () => {
    setup(ungradedResult());
    expect(screen.getByRole('heading', { name: 'This review could not be graded' })).toHaveFocus();
    expect(screen.getByRole('alert')).toHaveTextContent('The grader did not answer: the model is down.');
    expect(screen.getByText(MODEL)).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Issues, most severe first' })).toBeNull();
    expect(screen.queryByRole('region', { name: CONTRAST })).toBeNull();
  });

  it('says it plainly when the grader said nothing', () => {
    setup(ungradedResult({ error: null, graderModel: null }));
    expect(screen.getByRole('alert')).toHaveTextContent('The grader gave no assessment.');
    expect(screen.queryByText(/Tried with/)).toBeNull();
  });

  it('lets the learner read their own comments', () => {
    setup(ungradedResult());
    const cards = within(region('Your comments')).getAllByRole('article');
    expect(cards).toHaveLength(4);
    expect(cards[0]).toHaveTextContent('This lets one call more than the limit through.');
    expect(cards[0]).toHaveTextContent('src/limiter.py · line 13');
    expect(cards[0]).toHaveTextContent(`Your severity${severityLabel('major')}`);
    expect(cards[2]).toHaveTextContent('No lines selected');
    expect(cards[0]).not.toHaveTextContent('Matches');
  });

  it('says so when there were no comments', () => {
    setup(ungradedResult({ findings: [] }));
    expect(screen.getByText('You left no comments.')).toBeInTheDocument();
  });

  it('grades again when asked', async () => {
    const { onRegrade } = setup(ungradedResult());
    const button = screen.getByRole('button', { name: 'Grade again' });
    expect(button).toBeEnabled();
    await userEvent.click(button);
    expect(onRegrade).toHaveBeenCalledTimes(1);
  });

  it('is disabled, with a spinner, while it grades again', async () => {
    const { onRegrade } = setup(ungradedResult(), { regrading: true });
    const button = screen.getByRole('button', { name: 'Grade again' });
    expect(button).toBeDisabled();
    expect(button.querySelector('svg.animate-spin')).not.toBeNull();
    await userEvent.click(button);
    expect(onRegrade).not.toHaveBeenCalled();
    expect(screen.getByRole('status')).toHaveTextContent('Grading again…');
  });

  it('moves on when the learner asks for the next drill', async () => {
    const { onNextDrill } = setup(ungradedResult());
    await userEvent.click(screen.getByRole('button', { name: 'Next drill' }));
    expect(onNextDrill).toHaveBeenCalledTimes(1);
  });
});

describe('DrillReveal, a review with nothing to reveal yet', () => {
  it('says a handed-in review is being graded', () => {
    setup(gradedResult({ status: 'submitted', gradedAt: null, reveal: null }));
    expect(screen.getByRole('status')).toHaveTextContent('Your review is being graded.');
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('says there is nothing to show for a review still open, or graded without its reveal', () => {
    const { unmount } = setup(gradedResult({ status: 'open', gradedAt: null, reveal: null }));
    expect(screen.getByRole('status')).toHaveTextContent('There is nothing to show for this review yet.');
    unmount();
    setup(gradedResult({ reveal: null }));
    expect(screen.getByRole('status')).toHaveTextContent('There is nothing to show for this review yet.');
    expect(screen.queryByRole('button')).toBeNull();
  });
});
