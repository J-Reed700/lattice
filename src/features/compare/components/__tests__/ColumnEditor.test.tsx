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
    expect(screen.getByText('2 of 6 columns')).toBeInTheDocument();
  });

  it('refuses more than six columns and says so', async () => {
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

    expect(screen.getByText('Six columns maximum.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Compare' }));
    // The seventh is dropped rather than sent.
    expect(onSubmit).toHaveBeenCalledWith(['a', 'b', 'c', 'd', 'e', 'f']);
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
});
