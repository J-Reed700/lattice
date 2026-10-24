import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { PageList } from '@/features/journal/components/PageList';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

const page = (id: string, title: string) => ({ id, title, updatedAt: new Date().toISOString() } as WorkspaceNote);
const pages = [page('p1', 'Sourdough log'), page('p2', 'Spice blends')];

function renderList(props: Partial<Parameters<typeof PageList>[0]> = {}) {
  const handlers = { onSelectPage: vi.fn(), onNewPage: vi.fn(), onRenamePage: vi.fn(), onDeletePage: vi.fn() };
  render(<TooltipProvider><PageList pages={pages} activePageId="p1" {...handlers} {...props} /></TooltipProvider>);
  return handlers;
}

const row = (title: string) => screen.getByText(title).closest('li')!;

describe('PageList row actions', () => {
  it('renames a page from its button with Enter, trimmed', () => {
    const { onRenamePage } = renderList();
    fireEvent.click(within(row('Spice blends')).getByRole('button', { name: 'Rename page' }));
    const field = screen.getByRole('textbox', { name: 'Rename Spice blends' });
    expect(field).toHaveValue('Spice blends');
    fireEvent.change(field, { target: { value: '  Spice mixes  ' } });
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(onRenamePage).toHaveBeenCalledWith('p2', 'Spice mixes');
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('renames on double-click and commits on blur, skipping an unchanged or blank title', () => {
    const { onRenamePage } = renderList();
    fireEvent.doubleClick(screen.getByRole('button', { name: /Sourdough log/ }));
    fireEvent.blur(screen.getByRole('textbox', { name: 'Rename Sourdough log' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: /Sourdough log/ }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
    expect(onRenamePage).not.toHaveBeenCalled();
  });

  it('abandons a rename on Escape and ignores other keys', () => {
    const { onRenamePage } = renderList();
    fireEvent.click(within(row('Sourdough log')).getByRole('button', { name: 'Rename page' }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Rye log' } });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'a' });
    expect(screen.getByRole('textbox')).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(onRenamePage).not.toHaveBeenCalled();
  });

  it('does not rename on double-click without a rename handler', () => {
    renderList({ onRenamePage: undefined });
    fireEvent.doubleClick(screen.getByRole('button', { name: /Sourdough log/ }));
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Rename page' })).not.toBeInTheDocument();
  });

  it('deletes a page, but always keeps the last one', () => {
    const { onDeletePage } = renderList();
    fireEvent.click(within(row('Spice blends')).getByRole('button', { name: 'Delete page' }));
    expect(onDeletePage).toHaveBeenCalledWith(pages[1]);
  });

  it('cannot delete the only page', () => {
    renderList({ pages: [pages[0]] });
    expect(screen.getByRole('button', { name: 'A journal keeps at least one page' })).toBeDisabled();
  });

  it('offers no row actions without handlers', () => {
    renderList({ onRenamePage: undefined, onDeletePage: undefined });
    expect(screen.queryByRole('button', { name: /Delete page|Rename page/ })).not.toBeInTheDocument();
  });
});
