import type { ComponentProps } from 'react';

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { ExamplesTable } from '../ExamplesTable';
import { programWorkDto } from './fixtures';

const problem = programWorkDto().problem;

function example(overrides: Partial<Wire.CodingExampleDto> = {}): Wire.CodingExampleDto {
  return {
    ordinal: 0,
    args: ['[3, 3]', '6'],
    expected: '[0, 1]',
    verdict: 'right',
    expects: null,
    whyInvalid: null,
    ...overrides,
  };
}

function renderTable(props: Partial<ComponentProps<typeof ExamplesTable>> = {}) {
  const onCheck = vi.fn().mockResolvedValue(undefined);
  const onRemove = vi.fn().mockResolvedValue(undefined);
  render(
    <ExamplesTable
      signature={problem.signature}
      examples={[]}
      coverage={{ caught: 0, total: 3, uncaught: null }}
      onCheck={onCheck}
      onRemove={onRemove}
      {...props}
    />,
  );
  return { onCheck, onRemove };
}

describe('ExamplesTable', () => {
  it('has one field per parameter of the signature and one for the expected result', () => {
    renderTable();
    expect(screen.getByLabelText('numbers')).toBeInTheDocument();
    expect(screen.getByLabelText('target')).toBeInTheDocument();
    expect(screen.getByLabelText('expected')).toBeInTheDocument();
  });

  it('sends the arguments in signature order and the expected result', async () => {
    const user = userEvent.setup();
    const { onCheck } = renderTable();
    await user.type(screen.getByLabelText('target'), '6');
    await user.type(screen.getByLabelText('numbers'), '[[3, 3]');
    await user.type(screen.getByLabelText('expected'), '[[0, 1]');
    await user.click(screen.getByRole('button', { name: 'Check' }));
    expect(onCheck).toHaveBeenCalledWith({ ordinal: null, args: ['[3, 3]', '6'], expected: '[0, 1]' });
  });

  it('keeps Check off until every field is filled and says why', () => {
    renderTable();
    expect(screen.getByRole('button', { name: 'Check' })).toBeDisabled();
    expect(screen.getByText(/Fill in every argument and the expected result/)).toBeInTheDocument();
  });

  it('shows each verdict with an icon and text', () => {
    renderTable({
      examples: [
        example({ ordinal: 0, verdict: 'right' }),
        example({ ordinal: 1, verdict: 'wrong', expects: '[0, 2]' }),
        example({ ordinal: 2, verdict: 'invalid', whyInvalid: 'That is not a Python value.' }),
      ],
    });
    const rows = screen.getAllByRole('row').slice(1);
    expect(rows).toHaveLength(3);
    const expected: [HTMLElement, string][] = [
      [rows[0], 'Right'],
      [rows[1], 'Not right'],
      [rows[2], 'Invalid'],
    ];
    for (const [row, text] of expected) {
      const badge = within(row).getByText(text);
      expect(badge.parentElement?.querySelector('svg')).not.toBeNull();
    }
    expect(within(rows[1]).getByText(/The problem expects \[0, 2\]/)).toBeInTheDocument();
    expect(within(rows[2]).getByText('That is not a Python value.')).toBeInTheDocument();
  });

  it('points at the statement when the setting keeps back what the problem expects', () => {
    renderTable({ examples: [example({ verdict: 'wrong', expects: null })] });
    expect(screen.getByText(/That's not what the problem expects\. Check the statement/)).toBeInTheDocument();
  });

  it('announces the coverage and describes the missed mistake only when given', () => {
    renderTable({ coverage: { caught: 2, total: 3, uncaught: 'an input where the same number appears twice' } });
    expect(screen.getByRole('status')).toHaveTextContent('Your examples catch 2 of 3 common mistakes');
    expect(screen.getByText(/an input where the same number appears twice/)).toBeInTheDocument();
  });

  it('says nothing about coverage when the session holds none', () => {
    renderTable({ coverage: null });
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.queryByText(/common mistakes/)).not.toBeInTheDocument();
  });

  it('removes a row and loads a row for editing under its ordinal', async () => {
    const user = userEvent.setup();
    const { onCheck, onRemove } = renderTable({ examples: [example({ ordinal: 4 })] });
    await user.click(screen.getByRole('button', { name: /Remove example 1/ }));
    expect(onRemove).toHaveBeenCalledWith(4);

    await user.click(screen.getByRole('button', { name: /Edit example 1/ }));
    expect(screen.getByLabelText('numbers')).toHaveValue('[3, 3]');
    await user.clear(screen.getByLabelText('expected'));
    await user.type(screen.getByLabelText('expected'), '[[1, 0]');
    await user.click(screen.getByRole('button', { name: 'Check' }));
    expect(onCheck).toHaveBeenCalledWith({ ordinal: 4, args: ['[3, 3]', '6'], expected: '[1, 0]' });
  });

  it('clears the form when the row being edited is removed', async () => {
    const user = userEvent.setup();
    const { onCheck } = renderTable({ examples: [example({ ordinal: 4 })] });
    await user.click(screen.getByRole('button', { name: /Edit example 1/ }));
    expect(screen.getByLabelText('numbers')).toHaveValue('[3, 3]');
    await user.click(screen.getByRole('button', { name: /Remove example 1/ }));
    expect(screen.getByLabelText('numbers')).toHaveValue('');

    await user.type(screen.getByLabelText('numbers'), '[[1]');
    await user.type(screen.getByLabelText('target'), '2');
    await user.type(screen.getByLabelText('expected'), '[[0]');
    await user.click(screen.getByRole('button', { name: 'Check' }));
    expect(onCheck).toHaveBeenCalledWith({ ordinal: null, args: ['[1]', '2'], expected: '[0]' });
  });
});
