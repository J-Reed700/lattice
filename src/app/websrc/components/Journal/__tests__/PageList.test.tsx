import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { formatPageTime, PageList } from '../PageList';

function page(overrides: Partial<WorkspaceNote> = {}): WorkspaceNote {
  return {
    id: 'note_1',
    title: 'Week of Sep 1',
    content: '',
    linkedDocumentIds: [],
    linkedConversationIds: [],
    highlights: [],
    stickyNotes: [],
    conversationSnapshots: [],
    createdAt: '2026-09-01T09:00:00.000Z',
    updatedAt: '2026-09-06T09:00:00.000Z',
    ...overrides,
  } as WorkspaceNote;
}

function renderList(props: Partial<Parameters<typeof PageList>[0]> = {}) {
  return render(
    <TooltipProvider>
      <PageList
        pages={[page(), page({ id: 'note_2', title: 'Journal · Journal 1' })]}
        activePageId="note_2"
        onSelectPage={vi.fn()}
        onNewPage={vi.fn()}
        {...props}
      />
    </TooltipProvider>,
  );
}

describe('formatPageTime', () => {
  it('counts minutes, hours and days before falling back to a date', () => {
    const now = Date.now();
    expect(formatPageTime(new Date(now - 10_000).toISOString())).toBe('just now');
    expect(formatPageTime(new Date(now - 12 * 60_000).toISOString())).toBe('12m');
    expect(formatPageTime(new Date(now - 3 * 3_600_000).toISOString())).toBe('3h');
    expect(formatPageTime(new Date(now - 2 * 86_400_000).toISOString())).toBe('2d');
    expect(formatPageTime(new Date(now - 30 * 86_400_000).toISOString())).toMatch(/\w{3} \d+/);
  });

  it('says nothing for an unparseable timestamp rather than "Invalid Date"', () => {
    expect(formatPageTime('not a date')).toBe('');
  });
});

describe('PageList', () => {
  it('lists every page in the order it is given', () => {
    renderList();
    const rows = screen.getAllByRole('button').filter((b) => b.textContent?.trim());
    expect(rows[0]).toHaveTextContent('Week of Sep 1');
    expect(rows[1]).toHaveTextContent('Journal · Journal 1');
  });

  it('marks the current page and no other', () => {
    renderList();
    const marked = screen
      .getAllByRole('button')
      .filter((b) => b.getAttribute('aria-current') === 'page');
    expect(marked).toHaveLength(1);
    expect(marked[0]).toHaveTextContent('Journal · Journal 1');
  });

  it('names an untitled page rather than showing an empty row', () => {
    renderList({ pages: [page({ id: 'note_3', title: '   ' })], activePageId: null });
    expect(screen.getByText('Untitled page')).toBeInTheDocument();
  });

  it('opens the page that was clicked', async () => {
    const user = userEvent.setup();
    const onSelectPage = vi.fn();
    renderList({ onSelectPage });

    await user.click(screen.getByText('Week of Sep 1'));
    expect(onSelectPage).toHaveBeenCalledWith('note_1');
  });

  it('offers one verb for a new page', async () => {
    const user = userEvent.setup();
    const onNewPage = vi.fn();
    renderList({ onNewPage });

    await user.click(screen.getByRole('button', { name: 'New page' }));
    expect(onNewPage).toHaveBeenCalledTimes(1);
  });

  it('says so when there are no pages', () => {
    renderList({ pages: [], activePageId: null });
    expect(screen.getByText('No pages yet.')).toBeInTheDocument();
  });
});
