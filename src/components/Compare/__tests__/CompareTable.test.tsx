import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { CompareTableDto } from '@/types/api/compare';

import { CompareTable } from '../CompareTable';


function table(overrides: Partial<CompareTableDto> = {}): CompareTableDto {
  return {
    columns: ['method', 'sample size'],
    rows: [
      {
        documentId: 'doc_1',
        title: 'trial.pdf',
        filePath: '/vault/trial.pdf',
        cells: [
          {
            value: 'randomised controlled trial',
            citation: {
              chunkId: 'chunk_2',
              excerpt: 'We conducted a randomised controlled trial.',
            },
          },
          { value: null, citation: null },
        ],
        error: null,
      },
    ],
    modelName: 'llama-3',
    generatedAt: '2026-09-06T12:00:00.000Z',
    ...overrides,
  };
}

describe('CompareTable', () => {
  it('renders one header per column plus Document', () => {
    render(<CompareTable table={table()} onOpenCitation={vi.fn()} />);
    const headers = screen.getAllByRole('columnheader');
    expect(headers.map((h) => h.textContent)).toEqual([
      'Document',
      'method',
      'sample size',
    ]);
  });

  it('renders "not stated" for a null cell rather than an empty cell', () => {
    render(<CompareTable table={table()} onOpenCitation={vi.fn()} />);
    expect(screen.getByText('not stated')).toBeInTheDocument();
    const cells = screen.getAllByRole('cell');
    expect(cells.every((cell) => cell.textContent?.trim() !== '')).toBe(true);
  });

  it('renders a value with no citation button when the citation is null', () => {
    const source = table();
    source.rows[0].cells[0] = { value: 'survey', citation: null };
    render(<CompareTable table={source} onOpenCitation={vi.fn()} />);
    expect(screen.getByText('survey')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('calls onOpenCitation with the row and cell', async () => {
    const user = userEvent.setup();
    const onOpenCitation = vi.fn();
    const source = table();
    render(<CompareTable table={source} onOpenCitation={onOpenCitation} />);

    await user.click(screen.getByRole('button'));
    expect(onOpenCitation).toHaveBeenCalledWith(source.rows[0], source.rows[0].cells[0]);
  });

  it('renders a row error as a muted line and still renders every cell', () => {
    const source = table();
    source.rows[0].error = 'Timed out.';
    render(<CompareTable table={source} onOpenCitation={vi.fn()} />);
    expect(screen.getByText('Timed out.')).toBeInTheDocument();
    expect(screen.getAllByRole('cell')).toHaveLength(3);
  });

  it('renders an empty-string value as a value, not as "not stated"', () => {
    // "not stated" is what `null` means. An empty answer the model returned is
    // still an answer, and the cell must not claim the document is silent.
    const source = table();
    source.rows[0].cells[1] = { value: '', citation: null };
    render(<CompareTable table={source} onOpenCitation={vi.fn()} />);
    expect(screen.queryByText('not stated')).not.toBeInTheDocument();
  });

  it('renders a missing tail cell as "not stated" instead of throwing', () => {
    const source = table({ columns: ['method', 'sample size', 'finding'] });
    render(<CompareTable table={source} onOpenCitation={vi.fn()} />);
    expect(screen.getAllByRole('cell')).toHaveLength(4);
    expect(screen.getAllByText('not stated')).toHaveLength(2);
  });
});
