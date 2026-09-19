import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { describeFocus, FocusChips } from '../FocusChips';

import type { SpaceDocument } from '../../../../types';

const document = (documentId: string, fileName: string): SpaceDocument => ({
  documentId,
  fileName,
  category: 'Research Paper',
  modifiedAt: '2026-09-18T00:00:00Z',
});

describe('the focus chips', () => {
  it('shows nothing when the chat is asking its whole space', () => {
    const { container } = render(
      <FocusChips documents={[]} onRemove={vi.fn()} onClear={vi.fn()} />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('names each document the chat is pinned to', () => {
    render(
      <FocusChips
        documents={[document('doc-1', 'Halvorsen 2024.pdf')]}
        onRemove={vi.fn()}
        onClear={vi.fn()}
      />
    );

    expect(screen.getByText('Only')).toBeInTheDocument();
    expect(screen.getByText('Halvorsen 2024.pdf')).toBeInTheDocument();
  });

  it('unpins one document by id', async () => {
    const onRemove = vi.fn();
    render(
      <FocusChips
        documents={[document('doc-1', 'Halvorsen 2024.pdf')]}
        onRemove={onRemove}
        onClear={vi.fn()}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: /Stop asking only Halvorsen/ }));
    expect(onRemove).toHaveBeenCalledWith('doc-1');
  });

  it('offers the whole space back once more than one document is pinned', async () => {
    const onClear = vi.fn();
    render(
      <FocusChips
        documents={[document('doc-1', 'Halvorsen 2024.pdf'), document('doc-2', 'Transect.md')]}
        onRemove={vi.fn()}
        onClear={onClear}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Ask the whole space' }));
    expect(onClear).toHaveBeenCalled();
  });
});

describe('what a focused chat calls itself', () => {
  it('counts the documents and names the space', () => {
    expect(describeFocus(2, 'Heat Island Thesis')).toBe(
      'Asking 2 documents in Heat Island Thesis'
    );
    expect(describeFocus(1, 'Heat Island Thesis')).toBe('Asking 1 document in Heat Island Thesis');
  });

  it('says only what it knows when there is no space to name', () => {
    expect(describeFocus(3, null)).toBe('Asking 3 documents');
  });
});
