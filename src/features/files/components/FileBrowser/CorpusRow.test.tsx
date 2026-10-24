import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { CorpusRow } from './CorpusRow';

const doc = { id: 'd1', fileName: 'Herbal.pdf', filePath: '/lib/Herbal.pdf', fileType: 'pdf', category: 'document', modifiedAt: '2026-01-01', indexedAt: '2026-01-01' } as DocumentMetadata;

const renderRow = (overrides: Partial<Parameters<typeof CorpusRow>[0]> = {}) => {
  const props = {
    doc,
    isSelected: false,
    selectionActive: false,
    onClick: vi.fn(),
    onDoubleClick: vi.fn(),
    onToggleSelect: vi.fn(),
    onContextMenu: vi.fn(),
    onRename: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  };
  render(<CorpusRow {...props} />);
  return props;
};

const row = () => screen.getByRole('checkbox').closest('[aria-selected]') as HTMLElement;

describe('CorpusRow', () => {
  it('shows the selection box on hover, selection mode, or when selected', () => {
    const { rerender } = render(<CorpusRow doc={doc} isSelected={false} selectionActive={false} onClick={vi.fn()} onDoubleClick={vi.fn()} onToggleSelect={vi.fn()} onContextMenu={vi.fn()} />);
    const box = screen.getByRole('checkbox', { name: 'Select Herbal.pdf' });
    expect(box.className).toContain('opacity-0');
    fireEvent.mouseEnter(row());
    expect(box.className).toContain('opacity-100');
    fireEvent.mouseLeave(row());
    expect(box.className).toContain('opacity-0');
    rerender(<CorpusRow doc={doc} isSelected selectionActive={false} onClick={vi.fn()} onDoubleClick={vi.fn()} onToggleSelect={vi.fn()} onContextMenu={vi.fn()} />);
    expect(screen.getByRole('checkbox')).toHaveAttribute('aria-checked', 'true');
    expect(screen.queryByRole('button', { name: 'Rename' })).not.toBeInTheDocument();
  });

  it('opens on double-click or Enter, and selects on Space or the box', () => {
    const props = renderRow({ selectionActive: true, indent: 24 });
    fireEvent.click(row());
    expect(props.onClick).toHaveBeenCalled();
    fireEvent.doubleClick(row());
    fireEvent.keyDown(row(), { key: 'Enter' });
    expect(props.onDoubleClick).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(row(), { key: ' ' });
    fireEvent.click(screen.getByRole('checkbox'));
    expect(props.onToggleSelect).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(row(), { key: 'x' });
    expect(row()).toHaveStyle({ paddingLeft: '36px' });
  });

  it('ignores keys meant for the buttons inside it', () => {
    const props = renderRow();
    fireEvent.keyDown(screen.getByRole('button', { name: 'Rename' }), { key: 'Enter' });
    expect(props.onDoubleClick).not.toHaveBeenCalled();
  });

  it('opens the menu from a right-click or the actions button', () => {
    const props = renderRow();
    fireEvent.contextMenu(row());
    fireEvent.click(screen.getByRole('button', { name: 'Actions for Herbal.pdf' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Actions for Herbal.pdf' }));
    expect(props.onContextMenu).toHaveBeenCalledTimes(2);
    expect(props.onClick).not.toHaveBeenCalled();
    expect(props.onDoubleClick).not.toHaveBeenCalled();
  });

  it('renames and deletes from the row without opening it', () => {
    const props = renderRow();
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(props.onRename).toHaveBeenCalled();
    expect(props.onDelete).toHaveBeenCalled();
    expect(props.onClick).not.toHaveBeenCalled();
  });
});
