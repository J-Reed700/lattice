import type { ReactNode } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BubbleMenuBar } from './BubbleMenuBar';

import type { Editor } from '@tiptap/react';

type ShouldShow = (props: { editor: FakeEditor; from: number; to: number }) => boolean;
const bubble = vi.hoisted(() => ({ shouldShow: null as null | ((props: never) => boolean) }));
vi.mock('@tiptap/react/menus', () => ({
  BubbleMenu: ({ children, shouldShow }: { children: ReactNode; shouldShow: (props: never) => boolean }) => {
    bubble.shouldShow = shouldShow;
    return <div role="toolbar" aria-label="Selection">{children}</div>;
  },
}));

interface FakeEditor {
  isEditable: boolean;
  active: Set<string>;
  calls: string[][];
  listeners: Map<string, () => void>;
  isActive: (name: string) => boolean;
  chain: () => Record<string, (...args: unknown[]) => unknown>;
  on: (event: string, listener: () => void) => void;
  off: (event: string, listener: () => void) => void;
  state: { selection: { from: number; to: number }; doc: { textBetween: (from: number, to: number, sep: string) => string } };
}

function fakeEditor(selected = '  The Nile floods each summer.  '): FakeEditor {
  const editor: FakeEditor = {
    isEditable: true,
    active: new Set(),
    calls: [],
    listeners: new Map(),
    isActive: (name) => editor.active.has(name),
    chain: () => {
      const steps: string[] = [];
      const chain = new Proxy({} as Record<string, (...args: unknown[]) => unknown>, {
        get: (_target, prop: string) => (...args: unknown[]) => {
          if (prop === 'run') {
            editor.calls.push(steps);
            return true;
          }
          steps.push(args.length ? `${prop}:${JSON.stringify(args[0])}` : prop);
          return chain;
        },
      });
      return chain;
    },
    on: (event, listener) => { editor.listeners.set(event, listener); },
    off: (event) => { editor.listeners.delete(event); },
    state: { selection: { from: 1, to: 9 }, doc: { textBetween: () => selected } },
  };
  return editor;
}

function Location() {
  const location = useLocation();
  return <output aria-label="location">{location.pathname + location.search}</output>;
}

function renderBar(editor: FakeEditor, selectionActions?: { id: string; label: string; onSelect: (text: string) => void }[]) {
  return render(
    <MemoryRouter initialEntries={['/journals']}>
      <BubbleMenuBar editor={editor as unknown as Editor} selectionActions={selectionActions} />
      <Location />
    </MemoryRouter>,
  );
}

const press = (name: string) => fireEvent.mouseDown(screen.getByRole('button', { name }));

let editor: FakeEditor;
beforeEach(() => { editor = fakeEditor(); });

describe('BubbleMenuBar', () => {
  it('shows only for an editable, non-empty selection outside code and images', () => {
    renderBar(editor);
    const shouldShow = bubble.shouldShow as unknown as ShouldShow;
    expect(shouldShow({ editor, from: 1, to: 5 })).toBe(true);
    expect(shouldShow({ editor, from: 3, to: 3 })).toBe(false);
    editor.active.add('codeBlock');
    expect(shouldShow({ editor, from: 1, to: 5 })).toBe(false);
    editor.active = new Set(['image']);
    expect(shouldShow({ editor, from: 1, to: 5 })).toBe(false);
    editor.active = new Set();
    editor.isEditable = false;
    expect(shouldShow({ editor, from: 1, to: 5 })).toBe(false);
  });

  it('toggles the marks and shows which are on', () => {
    editor.active.add('italic');
    renderBar(editor);
    expect(screen.getByRole('button', { name: 'Italic' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'false');
    press('Bold');
    press('Italic');
    press('Highlight');
    expect(editor.calls).toEqual([['focus', 'toggleBold'], ['focus', 'toggleItalic'], ['focus', 'toggleHighlight']]);
  });

  it('removes an existing link in one step', () => {
    editor.active.add('link');
    renderBar(editor);
    press('Remove link');
    expect(editor.calls).toEqual([['focus', 'unsetLink']]);
    expect(screen.queryByRole('textbox', { name: 'Paste a link' })).not.toBeInTheDocument();
  });

  it('links the selection to a pasted web address', () => {
    renderBar(editor);
    press('Link');
    const input = screen.getByRole('textbox', { name: 'Paste a link' });
    fireEvent.change(input, { target: { value: '  https://example.org/nile  ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(editor.calls).toEqual([['focus', 'extendMarkRange:"link"', 'setLink:{"href":"https://example.org/nile"}']]);
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('drops a link that is not a web address, or abandoned with Escape', () => {
    renderBar(editor);
    press('Link');
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'javascript:alert(1)' } });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
    expect(editor.calls).toEqual([]);
    press('Link');
    expect(screen.getByRole('textbox')).toHaveValue('');
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'a' });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });
    expect(screen.getByRole('button', { name: 'Bold' })).toBeInTheDocument();
  });

  it('closes the link field when the selection moves, and stops listening on unmount', () => {
    const { unmount } = renderBar(editor);
    press('Link');
    act(() => editor.listeners.get('selectionUpdate')!());
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    unmount();
    expect(editor.listeners.has('selectionUpdate')).toBe(false);
  });

  it('asks about the selection in a new chat', () => {
    renderBar(editor);
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Ask about this' }));
    expect(screen.getByRole('status', { name: 'location' })).toHaveTextContent('/chat?new=1&quote=The+Nile+floods+each+summer.');
  });

  it('runs a surface’s own verbs with the selected text, and nothing on blank text', () => {
    const onSelect = vi.fn();
    renderBar(editor, [{ id: 'highlight', label: 'Save highlight', onSelect }]);
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Save highlight' }));
    expect(onSelect).toHaveBeenCalledWith('The Nile floods each summer.');

    const blank = fakeEditor('   ');
    renderBar(blank, [{ id: 'other', label: 'Keep', onSelect }]);
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Keep' }));
    fireEvent.mouseDown(screen.getAllByRole('button', { name: 'Ask about this' })[1]);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(screen.getAllByRole('status', { name: 'location' })[1]).toHaveTextContent('/journals');
  });
});
