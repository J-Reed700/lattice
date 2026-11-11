import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { programCopy } from '../copy';
import { ProgramDebrief } from '../ProgramDebrief';
import { programDebrief, programSession } from './fixtures';

function renderDebrief() {
  render(<ProgramDebrief session={programSession()} debrief={programDebrief()} />);
}

describe('ProgramDebrief', () => {
  it('gives the strong answer with the reference folded away', () => {
    renderDebrief();
    expect(screen.getByText(/Remember each value and its position/)).toBeInTheDocument();
    expect(screen.getByText('O(n) time, O(n) space')).toBeInTheDocument();
    const reference = screen.getByText(/seen = \{\}/);
    expect(reference.closest('details')).not.toHaveAttribute('open');
  });

  it('lists every case in full', () => {
    renderDebrief();
    expect(screen.getByRole('heading', { name: 'Every case, in full' })).toBeInTheDocument();
    expect(screen.getAllByText(/an input with duplicates/).length).toBeGreaterThan(0);
  });

  it('keeps the words the debrief frame prints for Program', () => {
    expect(programCopy.debrief).toEqual({
      check: 'run',
      learnerCase: 'Example',
      techniques: 'Problem patterns',
      caught: 'Your examples caught this',
      missed: 'Your examples did not catch this',
    });
    expect((['nudge', 'idea', 'plan', 'point'] as const).map((rung) => programCopy.rung(rung).counted)).toEqual([
      'nudge',
      'idea',
      'plan',
      'line hint',
    ]);
  });
});
