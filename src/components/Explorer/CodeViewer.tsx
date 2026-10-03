import { useEffect, useLayoutEffect, useRef } from 'react';

import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { python } from '@codemirror/lang-python';
import { rust } from '@codemirror/lang-rust';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorState, RangeSetBuilder, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, lineNumbers, type DecorationSet } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { csharp } from '@replit/codemirror-lang-csharp';

import type { ExplorerLineRange } from '@/stores/explorerStore';

export interface CodeViewerProps {
  path: string;
  text: string;
  /** The backend's language name for the file (`rust`, `tsx`, …). */
  language: string | null;
  /** Lines the reader picked in the gutter. */
  selection: ExplorerLineRange | null;
  /** Lines an answer pointed at; scrolled to whenever `highlightNonce` changes. */
  highlight: ExplorerLineRange | null;
  highlightNonce: number | null;
  onSelectLines: (_range: ExplorerLineRange | null) => void;
}

/**
 * The languages the app already ships a CodeMirror mode for. Everything else
 * reads as plain text, which for a viewer is a fair trade against more
 * packages: line numbers, selection and highlights work the same.
 */
function languageExtension(language: string | null): Extension {
  switch (language) {
    case 'rust': return rust();
    case 'typescript': return javascript({ typescript: true });
    case 'tsx': return javascript({ typescript: true, jsx: true });
    case 'javascript': return javascript();
    case 'jsx': return javascript({ jsx: true });
    case 'python': return python();
    case 'json': return json();
    case 'csharp': return csharp();
    default: return [];
  }
}

const highlighting = syntaxHighlighting(HighlightStyle.define([
  { tag: t.keyword, color: 'var(--code-keyword)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--code-function)' },
  { tag: [t.typeName, t.className, t.namespace], color: 'var(--code-type)' },
  { tag: [t.string, t.special(t.string)], color: 'var(--code-string)' },
  { tag: [t.number, t.bool, t.null], color: 'var(--code-number)' },
  { tag: [t.comment, t.lineComment, t.blockComment], color: 'var(--code-comment)', fontStyle: 'italic' },
  { tag: [t.operator, t.punctuation, t.bracket], color: 'var(--code-punctuation)' },
]));

const theme = EditorView.theme({
  '&': {
    height: '100%',
    color: 'var(--code-fg)',
    backgroundColor: 'var(--code-bg)',
    fontSize: '12.5px',
    fontFamily: 'var(--font-mono, "JetBrains Mono Variable", "JetBrains Mono", ui-monospace, SFMono-Regular, monospace)',
  },
  '.cm-scroller': { lineHeight: '1.65', fontFamily: 'inherit' },
  '.cm-content': { padding: '10px 0 40vh' },
  '.cm-line': { padding: '0 16px 0 12px' },
  '.cm-gutters': {
    color: 'var(--code-gutter-fg)',
    backgroundColor: 'var(--code-bg)',
    border: 'none',
    cursor: 'pointer',
    userSelect: 'none',
  },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 10px 0 14px', minWidth: '44px', textAlign: 'right' },
  '.cm-selectionBackground, ::selection': { backgroundColor: 'var(--code-selection) !important' },
  '&.cm-focused': { outline: 'none' },
});

interface Marks { selection: ExplorerLineRange | null; highlight: ExplorerLineRange | null }
const setMarks = StateEffect.define<Marks>();

const selectedLine = Decoration.line({ class: 'cm-explorer-selected' });
const highlightedLine = Decoration.line({ class: 'cm-explorer-highlight' });

/** Line decorations for the picked lines and the referenced ones. */
const marksField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(decorations, transaction) {
    for (const effect of transaction.effects) {
      if (!effect.is(setMarks)) continue;
      const { doc } = transaction.state;
      const byLine = new Map<number, Decoration>();
      const paint = (range: ExplorerLineRange | null, decoration: Decoration) => {
        if (!range) return;
        const last = Math.min(range.endLine, doc.lines);
        for (let line = Math.max(1, range.startLine); line <= last; line += 1) byLine.set(line, decoration);
      };
      paint(effect.value.selection, selectedLine);
      // An answer's reference wins where the two overlap: it is the newer news.
      paint(effect.value.highlight, highlightedLine);
      const builder = new RangeSetBuilder<Decoration>();
      for (const line of [...byLine.keys()].sort((a, b) => a - b)) {
        const from = doc.line(line).from;
        builder.add(from, from, byLine.get(line)!);
      }
      return builder.finish();
    }
    return decorations;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** Gutter selection: click a number, shift-click to extend, or drag across. */
function gutterSelection(onSelect: (_range: ExplorerLineRange | null) => void, current: () => ExplorerLineRange | null): Extension {
  const lineAt = (view: EditorView, clientY: number) =>
    view.state.doc.lineAt(view.lineBlockAtHeight(clientY - view.documentTop).from).number;

  return lineNumbers({
    domEventHandlers: {
      mousedown(view, block, event) {
        const mouse = event as MouseEvent;
        if (mouse.button !== 0) return false;
        mouse.preventDefault();
        const line = view.state.doc.lineAt(block.from).number;
        const existing = current();
        const anchor = mouse.shiftKey && existing ? existing.startLine : line;
        const pick = (to: number) => onSelect({ startLine: Math.min(anchor, to), endLine: Math.max(anchor, to) });
        // A plain click on the only selected line clears it.
        if (!mouse.shiftKey && existing?.startLine === line && existing.endLine === line) {
          onSelect(null);
          return true;
        }
        pick(line);
        const move = (next: MouseEvent) => pick(lineAt(view, next.clientY));
        const up = () => {
          window.removeEventListener('mousemove', move);
          window.removeEventListener('mouseup', up);
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
        return true;
      },
    },
  });
}

/**
 * The read-only file view: CodeMirror with line numbers, gutter selection and
 * the lines an answer points at.
 */
export function CodeViewer({ path, text, language, selection, highlight, highlightNonce, onSelectLines }: CodeViewerProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Handlers read the latest props without rebuilding the editor.
  const onSelectRef = useRef(onSelectLines);
  const selectionRef = useRef(selection);
  useLayoutEffect(() => {
    onSelectRef.current = onSelectLines;
    selectionRef.current = selection;
  });

  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: text,
        extensions: [
          gutterSelection((range) => onSelectRef.current(range), () => selectionRef.current),
          languageExtension(language),
          highlighting,
          theme,
          marksField,
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          EditorView.contentAttributes.of({ 'aria-label': `Contents of ${path}` }),
        ],
      }),
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [language, path, text]);

  useEffect(() => {
    viewRef.current?.dispatch({ effects: setMarks.of({ selection, highlight }) });
  }, [selection, highlight, path, text]);

  // Scroll to a reference each time one is revealed, even the same one twice.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !highlight || highlightNonce === null) return;
    const line = Math.min(Math.max(1, highlight.startLine), view.state.doc.lines);
    view.dispatch({ effects: EditorView.scrollIntoView(view.state.doc.line(line).from, { y: 'center' }) });
  }, [highlightNonce, highlight, path, text]);

  return <div ref={hostRef} className="explorer-code h-full min-h-0 overflow-hidden" />;
}
