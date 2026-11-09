import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodeReviewDrillDto } from '@/lib/bindings';

import { DrillReview } from './DrillReview';
import type { Session } from './drillState';
import type { CodeReviewDrill } from './useCodeReviewDrill';

const clock = vi.hoisted(() => ({ props: [] as Array<Record<string, unknown>> }));
vi.mock('@/features/learning/practice/shared/ElapsedClock', () => ({
  ElapsedClock: (props: Record<string, unknown>) => {
    clock.props.push(props);
    return <p role="timer">clock</p>;
  },
}));

vi.mock('@/features/explorer/components/CodeViewer', () => ({
  CodeViewer: (props: { path: string }) => <div data-testid="code-viewer" data-path={props.path} />,
}));

const CODE_DRILL: CodeReviewDrillDto = {
  title: 'Rate limiter implementation',
  material: { kind: 'code', language: 'python' },
  files: [{ path: 'limiter.py', text: 'class RateLimiter:\n    def allow(self):\n        return True\n' }],
  lesson: null,
  issues: 4,
};

const PASSAGE_DRILL: CodeReviewDrillDto = {
  title: 'Spacing and recall',
  material: { kind: 'passage', sourceTitle: 'Spacing effect' },
  files: [{ path: 'explanation.md', text: 'Spaced practice improves recall.' }],
  lesson: null,
  issues: 3,
};

function sessionOf(patch: Partial<Session> = {}): Session {
  return {
    attemptId: 'attempt-1',
    mode: 'practice',
    startedAt: Date.now(),
    drill: CODE_DRILL,
    limitMs: null,
    ...patch,
  };
}

function drillOf(session: Session | null): CodeReviewDrill {
  const drill = {
    session,
    review: {
      findings: [],
      changedPaths: [],
      submitting: false,
      fixEdits: {},
      add: vi.fn(),
      update: vi.fn(),
      remove: vi.fn(),
      submit: vi.fn(),
      move: vi.fn(),
      editFix: vi.fn(),
      discardFix: vi.fn(),
    },
    failure: null,
    retry: vi.fn(),
    toChoices: vi.fn(),
  };
  return drill as unknown as CodeReviewDrill;
}

const back = <button type="button">Back to drill setup</button>;

beforeEach(() => {
  clock.props = [];
});

describe('DrillReview', () => {
  it('renders nothing before there is an attempt, without reading a missing session', () => {
    const { container } = render(<DrillReview drill={drillOf(null)} back={back} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('puts the back control the panel hands it in the header', () => {
    render(<DrillReview drill={drillOf(sessionOf())} back={back} />);
    expect(screen.getByRole('button', { name: 'Back to drill setup' })).toBeVisible();
  });

  it('names how many issues to find and what to do about each, above the panes', () => {
    render(<DrillReview drill={drillOf(sessionOf({ drill: { ...CODE_DRILL, issues: 4 } }))} back={back} />);
    expect(screen.getByText('Find the 4 issues in this code, and say what is wrong and how to fix each.')).toBeVisible();
  });

  it('says the issue, with no number, when there is one', () => {
    render(<DrillReview drill={drillOf(sessionOf({ drill: { ...CODE_DRILL, issues: 1 } }))} back={back} />);
    expect(screen.getByText('Find the issue in this code, and say what is wrong and how to fix it.')).toBeVisible();
  });

  it('asks for errors in an explanation', () => {
    render(<DrillReview drill={drillOf(sessionOf({ drill: PASSAGE_DRILL }))} back={back} />);
    expect(screen.getByText('Find the 3 errors in this explanation, and say what is wrong and how to fix each.')).toBeVisible();
  });

  it('says the error, with no number, of an explanation with one', () => {
    render(<DrillReview drill={drillOf(sessionOf({ drill: { ...PASSAGE_DRILL, issues: 1 } }))} back={back} />);
    expect(screen.getByText('Find the error in this explanation, and say what is wrong and how to fix it.')).toBeVisible();
  });

  it('shows the clock with its time limit as the soft limit when timed, and none when practising', () => {
    const startedAt = Date.now() - 30_000;
    const { unmount } = render(
      <DrillReview drill={drillOf(sessionOf({ mode: 'interview', startedAt, limitMs: 600_000 }))} back={back} />,
    );
    expect(screen.getByRole('timer')).toBeVisible();
    const last = clock.props[clock.props.length - 1];
    expect(last?.softLimitMs).toBe(600_000);
    expect(last?.running).toBe(true);
    expect(last?.elapsedMs).toBeGreaterThanOrEqual(30_000);
    expect(last?.elapsedMs).toBeLessThan(40_000);
    unmount();

    render(<DrillReview drill={drillOf(sessionOf())} back={back} />);
    expect(screen.queryByRole('timer')).not.toBeInTheDocument();
  });
});
