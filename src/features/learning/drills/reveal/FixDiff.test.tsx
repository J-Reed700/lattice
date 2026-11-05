import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { FixDiff } from '@/features/learning/drills/reveal/FixDiff';
import type { FixDiffDto } from '@/lib/bindings';

const FIX: FixDiffDto = {
  path: 'src/a.py',
  lines: [
    { kind: 'unchanged', text: 'def f():' },
    { kind: 'removed', text: '    return 1' },
    { kind: 'added', text: '    return 2' },
    { kind: 'unchanged', text: '' },
  ],
};

const rows = () => [...screen.getByRole('group', { name: 'The fix as a diff' }).querySelectorAll('[data-kind]')];

describe('FixDiff', () => {
  it('names the file and marks each line removed, added or unchanged', () => {
    render(<FixDiff fix={FIX} />);
    expect(screen.getByText('src/a.py')).toBeInTheDocument();
    expect(rows().map((row) => row.getAttribute('data-kind'))).toEqual(['unchanged', 'removed', 'added', 'unchanged']);
    expect(rows().map((row) => row.querySelector('[aria-hidden="true"]')?.textContent)).toEqual([
      '\u{a0}',
      '-',
      '+',
      '\u{a0}',
    ]);
    expect(rows()[1].querySelector('del')).toHaveTextContent('return 1');
    expect(rows()[2].querySelector('ins')).toHaveTextContent('return 2');
    expect(rows()[0].querySelector('ins, del')).toBeNull();
  });

  it('keeps a line’s indentation', () => {
    render(<FixDiff fix={FIX} />);
    expect(rows()[1].querySelector('del')).toHaveClass('whitespace-pre');
    expect(rows()[1].querySelector('del')?.textContent).toBe('Removed:     return 1');
  });

  it('says in words what each marked line does, for a reader that cannot see the marks', () => {
    render(<FixDiff fix={FIX} />);
    expect(rows()[1].querySelector('.sr-only')).toHaveTextContent('Removed:');
    expect(rows()[2].querySelector('.sr-only')).toHaveTextContent('Added:');
    expect(rows()[0].querySelector('.sr-only')).toHaveTextContent('Unchanged:');
  });

  it('keeps an empty line a line', () => {
    render(<FixDiff fix={FIX} />);
    expect(rows()[3].querySelector('.whitespace-pre')?.textContent).toBe('Unchanged: \u{a0}');
  });

  it('shows no lines for a fix that has none', () => {
    render(<FixDiff fix={{ path: 'src/a.py', lines: [] }} />);
    expect(rows()).toHaveLength(0);
  });
});
