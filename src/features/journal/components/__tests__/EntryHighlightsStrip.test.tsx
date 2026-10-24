import { createRef, type ComponentProps, type RefObject } from 'react';

import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { EntryHighlightsStrip, HIGHLIGHT_CHAR_LIMIT } from '@/features/journal/components/EntryHighlightsStrip';
import type { NoteHighlight } from '@/types/api/dailyNotes';

const highlight = (id: string, text: string, createdAt: string): NoteHighlight => ({ id, text, createdAt } as NoteHighlight);

const highlights = [
  highlight('old', 'Bread rises as yeast ferments sugar.', '2026-10-01T09:00:00Z'),
  highlight('new', 'Salt slows fermentation.', '2026-10-05T09:00:00Z'),
  highlight('pinned', 'Steam keeps the crust soft early on.', '2026-09-01T09:00:00Z'),
  highlight('undated', 'Scoring guides the oven spring.', 'not a date'),
];

let editor: HTMLDivElement;
let outside: HTMLDivElement;

function renderStrip(overrides: Partial<ComponentProps<typeof EntryHighlightsStrip>> = {}) {
  const editorContainerRef = { current: editor } as RefObject<HTMLElement | null>;
  const props: ComponentProps<typeof EntryHighlightsStrip> = {
    highlights, pinnedIds: new Set(['pinned']), onAddHighlight: vi.fn(), onRemoveHighlight: vi.fn(), onTogglePinned: vi.fn(),
    editorContainerRef, ...overrides,
  };
  render(<EntryHighlightsStrip {...props} />);
  return props;
}

function select(node: Node, start = 0, end = node.textContent?.length ?? 0) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  act(() => { document.dispatchEvent(new Event('selectionchange')); });
}

beforeEach(() => {
  editor = document.createElement('div');
  editor.textContent = 'Knead the dough until it is smooth and elastic.';
  outside = document.createElement('div');
  outside.textContent = 'A sidebar label';
  document.body.append(editor, outside);
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => { callback(0); return 0; });
});
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  editor.remove();
  outside.remove();
  vi.restoreAllMocks();
});

describe('EntryHighlightsStrip', () => {
  it('lists highlights pinned first, then newest, once expanded', async () => {
    const user = userEvent.setup();
    renderStrip();
    const toggle = screen.getByRole('button', { name: '4 highlights from this entry' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Salt slows fermentation.')).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    const quotes = screen.getAllByRole('listitem').map((item) => item.querySelector('blockquote')?.textContent);
    expect(quotes).toEqual([
      'Steam keeps the crust soft early on.',
      'Salt slows fermentation.',
      'Bread rises as yeast ferments sugar.',
      'Scoring guides the oven spring.',
    ]);
  });

  it('pins, unpins and removes a highlight', async () => {
    const user = userEvent.setup();
    const props = renderStrip({ embedded: true });
    const [pinnedItem, newItem] = screen.getAllByRole('listitem');
    await user.click(within(pinnedItem).getByRole('button', { name: 'Unpin' }));
    expect(props.onTogglePinned).toHaveBeenCalledWith('pinned');
    await user.click(within(newItem).getByRole('button', { name: 'Pin' }));
    expect(props.onTogglePinned).toHaveBeenCalledWith('new');
    await user.click(within(newItem).getByRole('button', { name: 'Remove' }));
    expect(props.onRemoveHighlight).toHaveBeenCalledWith('new');
  });

  it('counts a single highlight in the singular', () => {
    renderStrip({ highlights: [highlights[0]], pinnedIds: new Set() });
    expect(screen.getByRole('button', { name: '1 highlight from this entry' })).toBeInTheDocument();
  });

  it('explains how to add one when the rail has none, and renders nothing inline', () => {
    const { unmount } = render(<EntryHighlightsStrip highlights={[]} pinnedIds={new Set()} onAddHighlight={vi.fn()} onRemoveHighlight={vi.fn()} onTogglePinned={vi.fn()} editorContainerRef={createRef()} />);
    expect(screen.queryByText('No highlights yet')).not.toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    unmount();
    renderStrip({ highlights: [], embedded: true });
    expect(screen.getByText('No highlights yet')).toBeInTheDocument();
  });

  it('offers to highlight a selection inside the page and saves its text', async () => {
    const user = userEvent.setup();
    const props = renderStrip({ highlights: [] });
    select(editor.firstChild!, 10, 15);
    await user.click(await screen.findByRole('button', { name: 'Highlight' }));
    expect(props.onAddHighlight).toHaveBeenCalledWith('dough');
    expect(window.getSelection()?.rangeCount).toBe(0);
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument());
  });

  it('caps a saved highlight at the character limit', async () => {
    const user = userEvent.setup();
    editor.textContent = 'a'.repeat(HIGHLIGHT_CHAR_LIMIT + 50);
    const props = renderStrip({ embedded: true });
    select(editor.firstChild!);
    await user.click(await screen.findByRole('button', { name: 'Highlight' }));
    expect(props.onAddHighlight).toHaveBeenCalledWith('a'.repeat(HIGHLIGHT_CHAR_LIMIT));
  });

  it('ignores selections that are empty, blank, outside the page or not laid out', () => {
    editor.textContent = 'Proof   overnight';
    renderStrip();
    select(editor.firstChild!, 2, 2);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
    select(editor.firstChild!, 5, 8);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
    select(outside.firstChild!);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
    vi.spyOn(Range.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 0, 0));
    select(editor.firstChild!, 0, 5);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
    window.getSelection()?.removeAllRanges();
    act(() => { document.dispatchEvent(new Event('selectionchange')); });
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
  });

  it('leaves selections alone when the editor toolbar owns highlighting or there is no page', () => {
    const { unmount } = render(<EntryHighlightsStrip highlights={[]} pinnedIds={new Set()} onAddHighlight={vi.fn()} onRemoveHighlight={vi.fn()} onTogglePinned={vi.fn()} editorContainerRef={{ current: editor }} showFloatingToolbar={false} />);
    select(editor.firstChild!, 0, 5);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
    unmount();
    renderStrip({ editorContainerRef: createRef() });
    select(editor.firstChild!, 0, 5);
    expect(screen.queryByRole('button', { name: 'Highlight' })).not.toBeInTheDocument();
  });
});
