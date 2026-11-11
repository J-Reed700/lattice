import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { ProposalList } from './ProposalList';

const card = vi.hoisted(() => vi.fn());
vi.mock('./MisconceptionCard', () => ({
  MisconceptionCard: (props: Record<string, unknown>) => {
    card(props);
    return <div data-testid="misconception-card" />;
  },
}));

const proposal = (overrides: Record<string, unknown>) => ({
  id: 'p1', kind: 'follow_up', status: 'pending', text: 'Compare the periods.', evidenceQuote: null, ...overrides,
}) as Wire.LearningPracticeSessionDto['proposals'][number];

function setup(proposals: Wire.LearningPracticeSessionDto['proposals']) {
  const onDecide = vi.fn();
  render(<ProposalList programId="program-1" lessonId="lesson-1" proposals={proposals} active busy={false} onDecide={onDecide} />);
  return onDecide;
}

describe('ProposalList', () => {
  it('shows each status in words', () => {
    setup([
      proposal({ id: 'a', status: 'pending' }),
      proposal({ id: 'b', status: 'accepted', text: 'Kept idea.' }),
      proposal({ id: 'c', status: 'rejected', text: 'Dropped idea.' }),
    ]);
    expect(screen.getByText('Waiting for you')).toBeVisible();
    expect(screen.getByText('Accepted')).toBeVisible();
    expect(screen.getByText('Not used')).toBeVisible();
    for (const raw of ['pending', 'accepted', 'rejected']) expect(screen.queryByText(raw)).not.toBeInTheDocument();
  });

  it('decides a pending idea', async () => {
    const onDecide = setup([proposal({})]);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Accept idea' }));
    expect(onDecide).toHaveBeenCalledWith('p1', true);
  });

  it('files an accepted misconception under the attempt lesson', () => {
    setup([proposal({ kind: 'misconception', status: 'accepted', text: 'An observation proves it.' })]);
    expect(card).toHaveBeenCalledWith(expect.objectContaining({ programId: 'program-1', lessonId: 'lesson-1', idea: 'An observation proves it.' }));
  });
});
