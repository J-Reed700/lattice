import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ColumnEditor, parseColumns } from '../ColumnEditor';

describe('parseColumns', () => {
  it('trims, drops blanks and dedupes case-insensitively, keeping first casing', () => {
    expect(parseColumns('  method , Sample size, , method , SAMPLE SIZE ')).toEqual([
      'method',
      'Sample size',
    ]);
  });

  it('returns nothing for an empty or comma-only string', () => {
    expect(parseColumns('')).toEqual([]);
    expect(parseColumns(' , , ')).toEqual([]);
  });
});

describe('ColumnEditor', () => {
  it('submits the parsed columns on Enter', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <ColumnEditor
        value="method, sample size"
        onChange={vi.fn()}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    await user.type(screen.getByLabelText('Columns'), '{Enter}');
    expect(onSubmit).toHaveBeenCalledWith(['method', 'sample size']);
  });

  it('says how many columns are named', () => {
    render(
      <ColumnEditor value="a, b" onChange={vi.fn()} onSubmit={vi.fn()} isRunning={false} />,
    );
    expect(screen.getByText('2 columns')).toBeInTheDocument();
  });

  it('sends every column named, with no six-column limit', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <ColumnEditor
        value="a, b, c, d, e, f, g"
        onChange={vi.fn()}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    expect(screen.getByText('7 columns')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Compare' }));
    // The seventh is sent too; the backend splits a long row across calls.
    expect(onSubmit).toHaveBeenCalledWith(['a', 'b', 'c', 'd', 'e', 'f', 'g']);
  });

  it('asks for a column instead of failing silently on an empty field', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <ColumnEditor value="" onChange={vi.fn()} onSubmit={onSubmit} isRunning={false} />,
    );

    await user.type(screen.getByLabelText('Columns'), '{Enter}');
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText('Name at least one column.')).toBeInTheDocument();
  });

  it('fills the field from a preset', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ColumnEditor value="" onChange={onChange} onSubmit={vi.fn()} isRunning={false} />,
    );

    await user.click(screen.getByRole('button', { name: 'parties, term, termination' }));
    expect(onChange).toHaveBeenCalledWith('parties, term, termination');
  });

  it('disables Compare while a run is in flight', () => {
    render(
      <ColumnEditor value="a" onChange={vi.fn()} onSubmit={vi.fn()} isRunning />,
    );
    expect(screen.getByRole('button', { name: 'Compare' })).toBeDisabled();
  });

  it('renders interactive removable column chips and adds columns via Enter, comma, or Backspace', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onSubmit = vi.fn();

    const { rerender } = render(
      <ColumnEditor
        value="method, sample size"
        onChange={onChange}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    expect(screen.getByText('method')).toBeInTheDocument();
    expect(screen.getByText('sample size')).toBeInTheDocument();

    // Remove a chip via its close button.
    await user.click(screen.getByRole('button', { name: 'Remove column method' }));
    expect(onChange).toHaveBeenLastCalledWith('sample size');

    rerender(
      <ColumnEditor
        value="sample size"
        onChange={onChange}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    // Add a column by typing + Enter.
    const input = screen.getByLabelText('Columns');
    await user.type(input, 'finding{Enter}');
    expect(onChange).toHaveBeenLastCalledWith('sample size, finding');

    rerender(
      <ColumnEditor
        value="sample size, finding"
        onChange={onChange}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    // Comma commits immediately.
    await user.type(input, 'limitations,');
    expect(onChange).toHaveBeenLastCalledWith('sample size, finding, limitations');

    rerender(
      <ColumnEditor
        value="sample size, finding, limitations"
        onChange={onChange}
        onSubmit={onSubmit}
        isRunning={false}
      />,
    );

    // Backspace on empty input removes the last chip.
    await user.type(input, '{Backspace}');
    expect(onChange).toHaveBeenLastCalledWith('sample size, finding');
  });

  it('commits uncommitted draft text when Compare is clicked', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    render(
      <ColumnEditor value="method" onChange={onChange} onSubmit={onSubmit} isRunning={false} />,
    );

    await user.type(screen.getByLabelText('Columns'), 'outcome');
    await user.click(screen.getByRole('button', { name: 'Compare' }));
    expect(onChange).toHaveBeenCalledWith('method, outcome');
    expect(onSubmit).toHaveBeenCalledWith(['method', 'outcome']);
  });
});
