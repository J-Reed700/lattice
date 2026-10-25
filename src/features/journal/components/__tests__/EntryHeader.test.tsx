import type { ComponentProps } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { EntryHeader } from '@/features/journal/components/EntryHeader';

const NOW = new Date(2026, 9, 24, 12, 0);
const ago = (minutes: number) => new Date(NOW.getTime() - minutes * 60_000).toISOString();

function props(overrides: Partial<ComponentProps<typeof EntryHeader>> = {}): ComponentProps<typeof EntryHeader> {
  return {
    date: new Date(2026, 9, 21), journalName: 'Field notes', pageTitle: 'Tide pools', pageId: 'p1', onRename: vi.fn(),
    wordCount: 1234, lastEditedAt: ago(5), hasPendingChanges: false, isSaving: false, saveError: null, onRetrySave: vi.fn(),
    ...overrides,
  };
}

const title = () => screen.getByRole('textbox', { name: 'Page title' });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe('EntryHeader', () => {
  it('shows the date, word count and save state', () => {
    render(<EntryHeader {...props()} actions={<button type="button">Side panel</button>} />);
    expect(screen.getByText('Wednesday, Oct 21, 2026')).toHaveAttribute('title', 'Field notes');
    expect(screen.getByText('1,234 words')).toBeInTheDocument();
    expect(screen.getByText('Saved')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Side panel' })).toBeInTheDocument();
    expect(title()).toHaveValue('Tide pools');
  });

  it.each([
    [ago(0), 'edited just now'],
    [ago(12), 'edited 12m ago'],
    [ago(180), 'edited 3h ago'],
    [ago(3 * 24 * 60), 'edited 3d ago'],
    [new Date(2026, 8, 2, 9).toISOString(), 'edited Sep 2'],
    ['garbled', 'edited just now'],
  ])('describes the last edit at %s as "%s"', (lastEditedAt, label) => {
    render(<EntryHeader {...props({ lastEditedAt, wordCount: 1 })} />);
    expect(screen.getByText('1 word').parentElement).toHaveAttribute('title', label);
  });

  it('shows saving and a retry for a failed save', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const onRetrySave = vi.fn();
    const { rerender } = render(<EntryHeader {...props({ isSaving: true })} />);
    expect(screen.getByText('Saving…')).toBeInTheDocument();
    rerender(<EntryHeader {...props({ saveError: 'Disk full', onRetrySave })} />);
    await user.click(screen.getByRole('button', { name: /retry/i }));
    expect(onRetrySave).toHaveBeenCalled();
  });

  it('renames on Enter with the trimmed title', async () => {
    const user = userEvent.setup();
    const p = props();
    render(<EntryHeader {...p} />);
    await user.clear(title());
    await user.type(title(), '  Rock pools  {Enter}');
    expect(p.onRename).toHaveBeenCalledWith('Rock pools');
    expect(title()).not.toHaveFocus();
  });

  it('does not rename when the title is unchanged, and restores a cleared title', async () => {
    const user = userEvent.setup();
    const p = props();
    render(<EntryHeader {...p} />);
    await user.click(title());
    await user.tab();
    await user.clear(title());
    await user.tab();
    expect(p.onRename).not.toHaveBeenCalled();
    expect(title()).toHaveValue('Tide pools');
  });

  it('keeps a pasted multi-line title on one line', async () => {
    const user = userEvent.setup();
    const p = props({ pageTitle: null });
    render(<EntryHeader {...p} />);
    expect(title()).toHaveAttribute('placeholder', 'Untitled page');
    await user.click(title());
    await user.paste('Low tide\nhigh tide');
    expect(title()).toHaveValue('Low tide high tide');
  });

  it('abandons an edit on Escape', async () => {
    const user = userEvent.setup();
    const p = props();
    render(<EntryHeader {...p} />);
    await user.type(title(), ' at dusk{Escape}');
    expect(p.onRename).not.toHaveBeenCalled();
    expect(title()).toHaveValue('Tide pools');
  });

  it('takes the new title when the page or its name changes elsewhere', async () => {
    const user = userEvent.setup();
    const p = props();
    const { rerender } = render(<EntryHeader {...p} />);
    await user.type(title(), ' draft');
    rerender(<EntryHeader {...p} pageId="p2" pageTitle="Kelp forests" />);
    expect(title()).toHaveValue('Kelp forests');
    rerender(<EntryHeader {...p} pageId="p2" pageTitle="Kelp forests of the north" />);
    expect(title()).toHaveValue('Kelp forests of the north');
  });

  it('focuses and selects the title of a page that was just made', () => {
    render(<EntryHeader {...props({ pageTitle: '', autoFocusTitle: true })} />);
    expect(title()).toHaveFocus();
  });
});
