import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { DebriefFrame } from '../DebriefFrame';
import { createFakeKind } from './fakeKind';
import { practiceDebrief, practiceSession } from './fixtures';

function renderDebrief() {
  const onTryInterview = vi.fn();
  const onExit = vi.fn();
  render(
    <DebriefFrame
      session={practiceSession({ title: 'Pair with a sum' })}
      debrief={practiceDebrief()}
      kind={createFakeKind(vi.fn())}
      onExit={onExit}
      onTryInterview={onTryInterview}
    />,
  );
  return { onTryInterview, onExit };
}

describe('DebriefFrame', () => {
  it('titles the debrief with the session\'s title', () => {
    renderDebrief();
    expect(screen.getByRole('heading', { name: 'Debrief: Pair with a sum' })).toBeInTheDocument();
  });

  it('shows the scorecard with its evidence, and not enough evidence as such', () => {
    renderDebrief();
    expect(screen.getByText('Problem solving')).toBeInTheDocument();
    expect(screen.getByText(/a stack remembers what I have seen/)).toBeInTheDocument();
    expect(screen.getByText('Testing')).toBeInTheDocument();
    expect(screen.getByText(/Not enough evidence/)).toBeInTheDocument();
  });

  it('shows the facts beside the scores, each stage under the label the wire sent', () => {
    renderDebrief();
    const facts = screen.getByRole('region', { name: 'What happened' });
    expect(within(facts).getByText('Understand')).toBeInTheDocument();
    expect(within(facts).getByText('Make it')).toBeInTheDocument();
    expect(within(facts).getByText(/1:30/)).toBeInTheDocument();
    expect(within(facts).getByText(/5:00/)).toBeInTheDocument();
    expect(within(facts).getByText('4 trials')).toBeInTheDocument();
    expect(within(facts).getByText('1 small push')).toBeInTheDocument();
    expect(within(facts).getByText(/Coached/)).toBeInTheDocument();
    expect(within(facts).getByText(/Mock/)).toBeInTheDocument();
  });

  it('gives the kind\'s part of the debrief', () => {
    renderDebrief();
    expect(screen.getByText('Kind debrief text.')).toBeInTheDocument();
  });

  it('lists which misreadings the examples caught', () => {
    renderDebrief();
    expect(screen.getByText('Reverses the words instead of the letters')).toBeInTheDocument();
    const missed = screen.getByText('Drops the last letter').closest('li');
    expect(missed).toHaveTextContent('Missed');
    expect(screen.getByText('Reverses the words instead of the letters').closest('li')).toHaveTextContent('Caught');
  });

  it('names the technique with its rating and when it comes back', () => {
    renderDebrief();
    const techniques = screen.getByRole('region', { name: 'Techniques' });
    expect(within(techniques).getByText('Two pointers')).toBeInTheDocument();
    expect(within(techniques).getByText('Good')).toBeInTheDocument();
    expect(within(techniques).getByText(/Comes back/)).toBeInTheDocument();
    expect(within(techniques).getByText(/walk two positions inward/)).toBeInTheDocument();
  });

  it('labels run and example evidence in the words of the kind', () => {
    const base = practiceDebrief();
    render(
      <DebriefFrame
        session={practiceSession()}
        debrief={{
          ...base,
          scorecard: {
            rows: [
              {
                ...base.scorecard.rows[0],
                evidence: [
                  { kind: 'run', runId: 'run-1' },
                  { kind: 'example', ordinal: 1 },
                ],
              },
            ],
          },
        }}
        kind={createFakeKind(vi.fn())}
        onExit={vi.fn()}
        onTryInterview={vi.fn()}
      />,
    );
    expect(screen.getByText('A trial')).toBeInTheDocument();
    expect(screen.getByText('Case 2')).toBeInTheDocument();
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
