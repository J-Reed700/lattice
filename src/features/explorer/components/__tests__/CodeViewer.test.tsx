import { foldable, foldedRanges, foldEffect, language } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { CodeViewer, type CodeViewerProps } from '../CodeViewer';

const SOURCE = [
  '#include <vector>',
  '',
  'int sum(const std::vector<int>& values) {',
  '  int total = 0;',
  '  for (int value : values) total += value;',
  '  return total;',
  '}',
].join('\n');

function showViewer(props: Partial<CodeViewerProps> = {}) {
  const onSelectLines = vi.fn();
  const base: CodeViewerProps = {
    path: 'src/sum.cpp',
    text: SOURCE,
    language: 'cpp',
    selection: null,
    highlight: null,
    highlightNonce: null,
    onSelectLines,
    ...props,
  };
  const rendered = render(<CodeViewer {...base} />);
  const rerender = (next: Partial<CodeViewerProps>) => rendered.rerender(<CodeViewer {...base} {...next} />);
  return { onSelectLines, rerender };
}

const editor = () => document.querySelector<HTMLElement>('.cm-editor')!;
const view = () => EditorView.findFromDOM(editor())!;
const lineClass = (line: number) => document.querySelectorAll('.cm-line')[line - 1]!.className;

/** Presses a line number. jsdom has no layout, so the line under the pointer is given. */
function pressNumber(line: number, init: MouseEventInit = {}) {
  const current = view();
  const block = current.lineBlockAt(current.state.doc.line(line).from);
  const spy = vi.spyOn(current, 'lineBlockAtHeight').mockReturnValue(block);
  const numbers = document.querySelectorAll('.cm-lineNumbers .cm-gutterElement');
  fireEvent.mouseDown(numbers[numbers.length - 1]!, { button: 0, ...init });
  fireEvent.mouseUp(window);
  spy.mockRestore();
}

let rangeGeometry: { getClientRects?: PropertyDescriptor; getBoundingClientRect?: PropertyDescriptor } = {};

beforeAll(() => {
  // jsdom has no layout engine; CodeMirror measures ranges when it scrolls.
  rangeGeometry = {
    getClientRects: Object.getOwnPropertyDescriptor(Range.prototype, 'getClientRects'),
    getBoundingClientRect: Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect'),
  };
  Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] });
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', { configurable: true, value: () => new DOMRect(0, 0, 0, 16) });
});

afterAll(() => {
  if (rangeGeometry.getClientRects) Object.defineProperty(Range.prototype, 'getClientRects', rangeGeometry.getClientRects);
  else Reflect.deleteProperty(Range.prototype, 'getClientRects');
  if (rangeGeometry.getBoundingClientRect) Object.defineProperty(Range.prototype, 'getBoundingClientRect', rangeGeometry.getBoundingClientRect);
  else Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect');
});

describe('CodeViewer', () => {
  it('shows the text at once and colors it in place once the grammar loads', async () => {
    showViewer({ selection: { startLine: 4, endLine: 4 } });
    const mounted = editor();
    expect(document.querySelector('.cm-content')).toHaveTextContent('#include <vector>');

    await waitFor(() => expect(view().state.facet(language)?.name).toBe('cpp'));
    expect(editor()).toBe(mounted);
    expect(lineClass(4)).toContain('cm-explorer-selected');
  });

  it("uses the backend's language when the path does not name one", async () => {
    showViewer({ path: 'scripts/bootstrap', text: 'echo "$HOME"\n', language: 'shell' });
    await waitFor(() => expect(view().state.facet(language)?.name).toBe('shell'));
  });

  it('marks picked and referenced lines, the reference winning where they overlap', () => {
    showViewer({ selection: { startLine: 3, endLine: 5 }, highlight: { startLine: 5, endLine: 6 }, highlightNonce: 1 });
    expect(lineClass(3)).toContain('cm-explorer-selected');
    expect(lineClass(4)).toContain('cm-explorer-selected');
    expect(lineClass(5)).toContain('cm-explorer-highlight');
    expect(lineClass(5)).not.toContain('cm-explorer-selected');
    expect(lineClass(6)).toContain('cm-explorer-highlight');
    expect(lineClass(7)).toBe('cm-line');
  });

  it('picks lines in the gutter: click, shift-click to extend, click the lone line to clear', () => {
    const { onSelectLines, rerender } = showViewer();
    pressNumber(3);
    expect(onSelectLines).toHaveBeenLastCalledWith({ startLine: 3, endLine: 3 });

    rerender({ selection: { startLine: 3, endLine: 3 } });
    pressNumber(6, { shiftKey: true });
    expect(onSelectLines).toHaveBeenLastCalledWith({ startLine: 3, endLine: 6 });

    pressNumber(3);
    expect(onSelectLines).toHaveBeenLastCalledWith(null);
  });

  it('opens a fold that hides the lines an answer points at', async () => {
    const { rerender } = showViewer();
    await waitFor(() => expect(view().state.facet(language)?.name).toBe('cpp'));
    const header = view().state.doc.line(3);
    const body = foldable(view().state, header.from, header.to)!;
    view().dispatch({ effects: foldEffect.of(body) });
    expect(foldedRanges(view().state).size).toBe(1);

    rerender({ highlight: { startLine: 5, endLine: 5 }, highlightNonce: 1 });
    expect(foldedRanges(view().state).size).toBe(0);
    expect(lineClass(5)).toContain('cm-explorer-highlight');
  });

  it('finds in the file with ⌘F from the page, stepping through matches', async () => {
    const user = userEvent.setup();
    showViewer();
    const composer = document.body.appendChild(document.createElement('textarea'));

    // Typing elsewhere, ⌘F is left to that field.
    fireEvent.keyDown(composer, { key: 'f', metaKey: true });
    expect(screen.queryByRole('search')).toBeNull();

    fireEvent.keyDown(window, { key: 'f', metaKey: true });
    const field = screen.getByRole('textbox', { name: 'Find in file' });
    expect(field).toHaveFocus();

    await user.type(field, 'total');
    expect(screen.getByRole('search')).toHaveTextContent('1 of 3');
    await user.keyboard('{Enter}');
    expect(screen.getByRole('search')).toHaveTextContent('2 of 3');
    await user.keyboard('{Shift>}{Enter}{/Shift}');
    expect(screen.getByRole('search')).toHaveTextContent('1 of 3');

    await user.keyboard('{Backspace}');
    await user.click(screen.getByRole('button', { name: 'Match whole word' }));
    expect(screen.getByRole('button', { name: 'Match whole word' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('search')).toHaveTextContent('No results');

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('search')).toBeNull();
    composer.remove();
  });
});
