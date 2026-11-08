import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ReviewCheckbox, SelectionBar } from '@/features/learning/lessons/review/SelectionBar';

const NOUN: [string, string] = ['statement', 'statements'];

describe('SelectionBar', () => {
  it('shows nothing when nothing is selected', () => {
    const { container } = render(<SelectionBar count={0} noun={NOUN} actions={[{ label: 'Accept', onClick: vi.fn() }]} onClear={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('says how many are selected, in the singular and the plural', () => {
    const { rerender } = render(<SelectionBar count={1} noun={NOUN} actions={[]} onClear={vi.fn()} />);
    expect(screen.getByRole('toolbar', { name: '1 statement selected' })).toHaveTextContent('1 statement selected');
    rerender(<SelectionBar count={3} noun={NOUN} actions={[]} onClear={vi.fn()} />);
    expect(screen.getByRole('toolbar', { name: '3 statements selected' })).toHaveTextContent('3 statements selected');
  });

  it('runs an action and clears the selection', () => {
    const accept = vi.fn();
    const clear = vi.fn();
    render(<SelectionBar count={2} noun={NOUN} actions={[{ label: 'Accept', onClick: accept }]} onClear={clear} />);
    fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(accept).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(clear).toHaveBeenCalledTimes(1);
  });

  it('disables an action that is not available and says why', () => {
    render(
      <SelectionBar
        count={2}
        noun={NOUN}
        actions={[
          { label: 'Accept', onClick: vi.fn(), disabled: true, hint: 'Some of these need a note. Accept them one at a time.' },
          { label: 'Remove', onClick: vi.fn(), tone: 'danger' },
        ]}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Remove' })).toBeEnabled();
    expect(screen.getByText('Some of these need a note. Accept them one at a time.')).toBeInTheDocument();
  });

  it('shows no hint for an action that is available', () => {
    render(<SelectionBar count={2} noun={NOUN} actions={[{ label: 'Accept', onClick: vi.fn(), hint: 'Not shown while it can run.' }]} onClear={vi.fn()} />);
    expect(screen.queryByText('Not shown while it can run.')).not.toBeInTheDocument();
  });

  it('disables every button while busy', () => {
    render(<SelectionBar count={2} noun={NOUN} actions={[{ label: 'Accept', onClick: vi.fn() }]} onClear={vi.fn()} busy />);
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled();
  });
});

describe('ReviewCheckbox', () => {
  it('is a labelled checkbox that reports whether shift was held', () => {
    const toggle = vi.fn();
    const { rerender } = render(<ReviewCheckbox checked={false} label="Select this statement" onToggle={toggle} />);
    const box = screen.getByRole('checkbox', { name: 'Select this statement' });
    expect(box).not.toBeChecked();
    fireEvent.click(box);
    expect(toggle).toHaveBeenLastCalledWith(false);
    fireEvent.click(box, { shiftKey: true });
    expect(toggle).toHaveBeenLastCalledWith(true);
    rerender(<ReviewCheckbox checked label="Select this statement" onToggle={toggle} />);
    expect(screen.getByRole('checkbox', { name: 'Select this statement' })).toBeChecked();
  });

  it('can be disabled', () => {
    const toggle = vi.fn();
    render(<ReviewCheckbox checked={false} label="Select this statement" onToggle={toggle} disabled />);
    expect(screen.getByRole('checkbox', { name: 'Select this statement' })).toBeDisabled();
  });
});
