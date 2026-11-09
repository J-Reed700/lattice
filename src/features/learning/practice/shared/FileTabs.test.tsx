import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FileTabs } from './FileTabs';

const files = [
  { path: 'solution.py' },
  { path: 'notes.md', label: 'Notes', modified: true },
  { path: 'helpers.py' },
];

describe('FileTabs', () => {
  it('is a labelled tablist with the active tab selected and the others out of the tab order', () => {
    render(<FileTabs files={files} active="notes.md" onSelect={vi.fn()} label="Files" />);
    expect(screen.getByRole('tablist', { name: 'Files' })).toBeInTheDocument();
    const active = screen.getByRole('tab', { name: /Notes/ });
    expect(active).toHaveAttribute('aria-selected', 'true');
    expect(active).toHaveAttribute('tabindex', '0');
    expect(screen.getByRole('tab', { name: 'solution.py' })).toHaveAttribute('tabindex', '-1');
  });

  it('says in text that a file was edited', () => {
    render(<FileTabs files={files} active="solution.py" onSelect={vi.fn()} label="Files" />);
    expect(screen.getByRole('tab', { name: /Notes/ })).toHaveTextContent('Edited');
  });

  it('moves with the arrow keys, wrapping, and with Home and End', () => {
    const onSelect = vi.fn();
    render(<FileTabs files={files} active="solution.py" onSelect={onSelect} label="Files" />);
    const first = screen.getByRole('tab', { name: 'solution.py' });
    fireEvent.keyDown(first, { key: 'ArrowRight' });
    expect(onSelect).toHaveBeenLastCalledWith('notes.md');
    fireEvent.keyDown(first, { key: 'ArrowLeft' });
    expect(onSelect).toHaveBeenLastCalledWith('helpers.py');
    fireEvent.keyDown(first, { key: 'End' });
    expect(onSelect).toHaveBeenLastCalledWith('helpers.py');
    fireEvent.keyDown(screen.getByRole('tab', { name: 'helpers.py' }), { key: 'Home' });
    expect(onSelect).toHaveBeenLastCalledWith('solution.py');
  });

  it('selects on click', () => {
    const onSelect = vi.fn();
    render(<FileTabs files={files} active="solution.py" onSelect={onSelect} label="Files" />);
    fireEvent.click(screen.getByRole('tab', { name: 'helpers.py' }));
    expect(onSelect).toHaveBeenCalledWith('helpers.py');
  });
});
