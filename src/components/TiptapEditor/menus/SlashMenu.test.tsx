import { createRef, type MutableRefObject } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SlashMenu, type SlashHandlers } from './SlashMenu';
import { SLASH_MENU_PLUGIN_KEY } from '../extensions/slashMenu';


import type { Editor } from '@tiptap/react';

const items = vi.hoisted(() => {
  const Icon = () => <svg />;
  const make = (id: string, label: string, keywords: string[]) => ({ id, label, keywords, icon: Icon, run: vi.fn() });
  return [make('heading', 'Heading', ['title']), make('quote', 'Quote', ['cite']), make('list', 'Bullet list', ['ul'])];
});
vi.mock('./slashItems', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./slashItems')>()),
  SLASH_ITEMS: items,
}));

const tr = { setMeta: vi.fn() };
let coords = { left: 100, top: 200, bottom: 220 };
const editor = {
  view: {
    state: { tr },
    dispatch: vi.fn(),
    coordsAtPos: vi.fn(() => coords),
  },
} as unknown as Editor;

function setup() {
  const handlersRef = createRef<SlashHandlers>() as MutableRefObject<SlashHandlers>;
  render(<SlashMenu editor={editor} handlersRef={handlersRef} />);
  const open = (query: string, from = 5, to = 5 + query.length + 1) => act(() => handlersRef.current.onStateChange({ query, from, to }));
  const key = (name: Parameters<SlashHandlers['onKeyDown']>[0]) => {
    let handled = false;
    act(() => { handled = handlersRef.current.onKeyDown(name); });
    return handled;
  };
  return { handlersRef, open, key };
}

const options = () => screen.getAllByRole('option');
const active = () => screen.getByRole('option', { selected: true });

beforeEach(() => {
  coords = { left: 100, top: 200, bottom: 220 };
  tr.setMeta.mockReset().mockReturnValue('dismiss-transaction');
  vi.mocked(editor.view.dispatch).mockReset();
  for (const item of items) item.run.mockReset();
});
afterEach(() => vi.restoreAllMocks());

describe('SlashMenu', () => {
  it('stays hidden until the editor reports a slash, and for a query that matches nothing', () => {
    const { open, key } = setup();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(key('Enter')).toBe(false);
    open('zzz');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(key('ArrowDown')).toBe(false);
  });

  it('lists matching blocks below the caret', () => {
    const { open } = setup();
    open('');
    expect(screen.getByRole('listbox', { name: 'Insert block' })).toHaveStyle({ left: '100px', top: '226px' });
    expect(options().map((o) => o.textContent)).toEqual(['Heading', 'Quote', 'Bullet list']);
    open('cit');
    expect(options().map((o) => o.textContent)).toEqual(['Quote']);
  });

  it('moves the highlight with the arrows, wrapping both ways, and resets it on a new query', () => {
    const { open, key } = setup();
    open('');
    expect(active()).toHaveTextContent('Heading');
    expect(key('ArrowUp')).toBe(true);
    expect(active()).toHaveTextContent('Bullet list');
    key('ArrowDown');
    key('ArrowDown');
    expect(active()).toHaveTextContent('Quote');
    open('b');
    expect(active()).toHaveTextContent('Bullet list');
    fireEvent.mouseEnter(options()[0]);
    expect(active()).toHaveTextContent('Bullet list');
  });

  it('inserts the highlighted block on Enter or Tab and closes', () => {
    const { open, key } = setup();
    open('', 3, 4);
    key('ArrowDown');
    expect(key('Tab')).toBe(true);
    expect(items[1].run).toHaveBeenCalledWith(editor, { from: 3, to: 4 });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    open('', 3, 4);
    key('Enter');
    expect(items[0].run).toHaveBeenCalledWith(editor, { from: 3, to: 4 });
  });

  it('inserts a block picked with the mouse', () => {
    const { open } = setup();
    open('', 7, 8);
    fireEvent.mouseEnter(options()[2]);
    expect(active()).toHaveTextContent('Bullet list');
    fireEvent.mouseDown(options()[2]);
    expect(items[2].run).toHaveBeenCalledWith(editor, { from: 7, to: 8 });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('dismisses on Escape through the editor plugin', () => {
    const { open, key } = setup();
    open('');
    expect(key('Escape')).toBe(true);
    expect(tr.setMeta).toHaveBeenCalledWith(SLASH_MENU_PLUGIN_KEY, 'dismiss');
    expect(editor.view.dispatch).toHaveBeenCalledWith('dismiss-transaction');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('keeps the menu inside the window near its edges', () => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 240, 300));
    coords = { left: window.innerWidth - 50, top: window.innerHeight - 20, bottom: window.innerHeight - 4 };
    const { open } = setup();
    open('');
    const menu = screen.getByRole('listbox');
    expect(menu.style.left).toBe(`${window.innerWidth - 240 - 8}px`);
    expect(menu.style.top).toBe(`${window.innerHeight - 20 - 300 - 6}px`);
  });
});
