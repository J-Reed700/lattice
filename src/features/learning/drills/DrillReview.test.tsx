import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { DrillReview } from './DrillReview';
import type { CodeReviewDrill } from './useCodeReviewDrill';

function makeMockDrill(overrides: Partial<CodeReviewDrill> = {}): CodeReviewDrill {
  return {
    session: {
      attemptId: 'attempt-1',
      mode: 'practice' as const,
      startedAt: Date.now(),
      limitMs: null,
      drill: {
        title: 'Rate limiter implementation',
        material: { kind: 'code' as const, language: 'python' },
        files: [
          {
            path: 'limiter.py',
            text: 'class RateLimiter:\n    def allow(self):\n        return True\n',
          },
        ],
        lesson: null,
      },
    },
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
    ...overrides,
  } as unknown as CodeReviewDrill;
}

describe('DrillReview', () => {
  it('displays the back button with accessible name "Back to drill setup"', async () => {
    const drill = makeMockDrill();
    render(<DrillReview drill={drill} />);

    const backButton = screen.getByRole('button', { name: 'Back to drill setup' });
    expect(backButton).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(backButton);
    expect(drill.toChoices).toHaveBeenCalledTimes(1);
  });

  it('displays the one-line task statement above the panes for code drills', () => {
    const drill = makeMockDrill();
    render(<DrillReview drill={drill} />);

    expect(
      screen.getByText('Find the issues in this snippet and explain the fix for each'),
    ).toBeInTheDocument();
  });

  it('displays the one-line task statement above the panes for passage drills', () => {
    const drill = makeMockDrill();
    drill.session!.drill.material = {
      kind: 'passage' as const,
      sourceTitle: 'Spacing effect',
    } as unknown as typeof drill.session.drill.material;
    drill.session!.drill.files = [{ path: 'explanation.md', text: 'Spaced practice improves recall.' }];
    render(<DrillReview drill={drill} />);

    expect(
      screen.getByText('Find the issues in this explanation and explain the fix for each'),
    ).toBeInTheDocument();
  });
});
