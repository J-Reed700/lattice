import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { LearningCodeEditor } from './LearningCodeEditor';

function editorElement() {
  return document.querySelector<HTMLElement>('.cm-content[contenteditable="true"]');
}

let rangeGeometry: { getClientRects?: PropertyDescriptor; getBoundingClientRect?: PropertyDescriptor } = {};

beforeAll(() => {
  // jsdom has no layout engine. These two geometry methods are needed only
  // for CodeMirror's caret/selection measurement; browser geometry stays real.
  rangeGeometry = {
    getClientRects: Object.getOwnPropertyDescriptor(Range.prototype, 'getClientRects'),
    getBoundingClientRect: Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect'),
  };
  Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] });
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect(0, 0, 0, 16),
  });
});

afterAll(() => {
  if (rangeGeometry.getClientRects) Object.defineProperty(Range.prototype, 'getClientRects', rangeGeometry.getClientRects);
  else Reflect.deleteProperty(Range.prototype, 'getClientRects');
  if (rangeGeometry.getBoundingClientRect) Object.defineProperty(Range.prototype, 'getBoundingClientRect', rangeGeometry.getBoundingClientRect);
  else Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect');
});

describe('LearningCodeEditor', () => {
  it('accepts real keyboard input, reports it, and keeps the editor mounted across controlled renders', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const rendered = render(<LearningCodeEditor path="src/main.ts" value="" onChange={onChange} />);
    const editor = editorElement();

    expect(editor).toBeInTheDocument();
    editor?.focus();
    await user.keyboard('const answer = 42;');

    expect(onChange).toHaveBeenLastCalledWith('const answer = 42;');
    expect(editor?.textContent).toBe('const answer = 42;');
    expect(document.querySelector('.cm-gutters .cm-gutter.cm-lineNumbers')).toBeInTheDocument();
    expect(screen.getByText('TypeScript')).toBeInTheDocument();

    const mountedEditor = editorElement();
    rendered.rerender(<LearningCodeEditor path="src/main.ts" value="const answer = 42;" onChange={onChange} />);
    expect(editorElement()).toBe(mountedEditor);
    expect(editorElement()).toHaveTextContent('const answer = 42;');
  });

  it('syncs an external value without reporting it as a learner edit', async () => {
    const onChange = vi.fn();
    const { rerender } = render(<LearningCodeEditor path="lesson.py" value="print(1)" onChange={onChange} />);

    rerender(<LearningCodeEditor path="lesson.py" value="print(2)" onChange={onChange} />);

    expect(editorElement()).toHaveTextContent('print(2)');
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByText('Python')).toBeInTheDocument();
  });

  it('changes language and starts a clean undo history when the active file changes', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const { rerender } = render(<LearningCodeEditor path="a.rs" value="fn main() {}" onChange={onChange} />);
    const editor = editorElement();
    editor?.focus();
    await user.keyboard('!');
    expect(onChange).toHaveBeenLastCalledWith('!fn main() {}');

    rerender(<LearningCodeEditor path="b.json" value='{"ready": true}' onChange={onChange} />);

    expect(screen.getByText('JSON')).toBeInTheDocument();
    expect(editorElement()).toHaveTextContent('{"ready": true}');
    onChange.mockClear();
    editorElement()?.focus();
    await user.keyboard('{Control>}z{/Control}');
    expect(editorElement()).toHaveTextContent('{"ready": true}');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('selects JavaScript, JSX, TSX, and plain text modes from the active file path', () => {
    const onChange = vi.fn();
    const { rerender } = render(<LearningCodeEditor path="app.js" value="const value = 1;" onChange={onChange} />);
    expect(screen.getByText('JavaScript')).toBeInTheDocument();

    for (const [path, language] of [['view.jsx', 'JSX'], ['view.tsx', 'TSX'], ['notes.md', 'Plain text']] as const) {
      rerender(<LearningCodeEditor path={path} value="const value = 1;" onChange={onChange} />);
      expect(screen.getByText(language)).toBeInTheDocument();
    }
  });

  it('indents with Tab and documents the Escape then Tab focus exit', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<><LearningCodeEditor path="main.js" value="run();" onChange={onChange} /><button type="button">Continue</button></>);
    const editor = editorElement();
    editor?.focus();

    await user.keyboard('{Tab}');
    expect(onChange).toHaveBeenLastCalledWith('  run();');
    expect(screen.getByText('Press Esc, then Tab to move focus')).toBeInTheDocument();
  });

  it('keeps read-only contents non-editable and labels the editing surface accessibly', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<LearningCodeEditor path="README.md" value="lesson notes" onChange={onChange} readOnly ariaLabel="Reference file contents" />);
    const editor = document.querySelector<HTMLElement>('.cm-content');

    expect(editor).toHaveAttribute('contenteditable', 'false');
    expect(editor).toHaveAttribute('aria-label', 'Reference file contents');
    expect(screen.getByText('Read only')).toBeInTheDocument();
    editor?.focus();
    await user.keyboard('changed');
    expect(editor).toHaveTextContent('lesson notes');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('opens the built-in find panel and supports a run keyboard shortcut', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onRunShortcut = vi.fn();
    render(<LearningCodeEditor path="lesson.cs" value='Console.WriteLine("hello");' onChange={onChange} onRunShortcut={onRunShortcut} />);

    await user.click(screen.getByRole('button', { name: 'Find in code' }));
    expect(screen.getByRole('textbox', { name: 'Find' })).toBeInTheDocument();
    expect(screen.getByText('C#')).toBeInTheDocument();

    editorElement()?.focus();
    await user.keyboard('{Control>}{Enter}{/Control}');
    expect(onRunShortcut).toHaveBeenCalledTimes(1);
  });
});
