import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { FileTabs, type FileTabItem } from './FileTabs';

describe('FileTabs', () => {
  it('renders file tabs with labels or fallback paths', () => {
    const files: FileTabItem[] = [
      { path: 'solution.py' },
      { path: 'tests/test_solution.py', label: 'Test Suite' },
    ];

    render(<FileTabs files={files} activePath="solution.py" onSelect={vi.fn()} />);

    expect(screen.getByRole('tablist', { name: /files/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /solution\.py/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Test Suite/ })).toBeInTheDocument();
  });

  it('marks active tab aria-selected="true" and others aria-selected="false"', () => {
    const files: FileTabItem[] = [
      { path: 'solution.py' },
      { path: 'tests.py' },
    ];

    render(<FileTabs files={files} activePath="solution.py" onSelect={vi.fn()} />);

    const activeTab = screen.getByRole('tab', { name: /solution\.py/ });
    const inactiveTab = screen.getByRole('tab', { name: /tests\.py/ });

    expect(activeTab).toHaveAttribute('aria-selected', 'true');
    expect(activeTab).toHaveAttribute('tabIndex', '0');
    expect(inactiveTab).toHaveAttribute('aria-selected', 'false');
    expect(inactiveTab).toHaveAttribute('tabIndex', '-1');
  });

  it('calls onSelect when another tab is clicked', async () => {
    const onSelect = vi.fn();
    const files: FileTabItem[] = [
      { path: 'solution.py' },
      { path: 'tests.py' },
    ];

    render(<FileTabs files={files} activePath="solution.py" onSelect={onSelect} />);

    const inactiveTab = screen.getByRole('tab', { name: /tests\.py/ });
    await userEvent.click(inactiveTab);

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith('tests.py');
  });

  it('supports arrow key navigation across tabs', () => {
    const onSelect = vi.fn();
    const files: FileTabItem[] = [
      { path: 'file1.py' },
      { path: 'file2.py' },
      { path: 'file3.py' },
    ];

    render(<FileTabs files={files} activePath="file1.py" onSelect={onSelect} />);

    const firstTab = screen.getByRole('tab', { name: /file1\.py/ });
    fireEvent.keyDown(firstTab, { key: 'ArrowRight' });

    expect(onSelect).toHaveBeenCalledWith('file2.py');

    fireEvent.keyDown(firstTab, { key: 'End' });
    expect(onSelect).toHaveBeenCalledWith('file3.py');
  });

  it('renders read-only indicator and badges', () => {
    const files: FileTabItem[] = [
      { path: 'reference.py', readOnly: true },
      { path: 'draft.py', badge: 'Draft' },
    ];

    render(<FileTabs files={files} activePath="draft.py" onSelect={vi.fn()} />);

    expect(screen.getByText(/read-only|locked/i)).toBeInTheDocument();
    expect(screen.getByText('Draft')).toBeInTheDocument();
  });

  it('renders optional actions node', () => {
    const files: FileTabItem[] = [{ path: 'solution.py' }];

    render(
      <FileTabs
        files={files}
        activePath="solution.py"
        onSelect={vi.fn()}
        actions={<button type="button">Reset Files</button>}
      />,
    );

    expect(screen.getByRole('button', { name: 'Reset Files' })).toBeInTheDocument();
  });
});
