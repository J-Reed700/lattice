import { useEffect, useLayoutEffect, useRef } from 'react';

import { bracketMatching, codeFolding, foldedRanges, foldGutter, foldKeymap, unfoldEffect } from '@codemirror/language';
import { openSearchPanel } from '@codemirror/search';
import { EditorState, RangeSetBuilder, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, keymap, lineNumbers, type DecorationSet } from '@codemirror/view';

import { findPanel } from '@/lib/code/findPanel';
import { codeHighlighting } from '@/lib/code/highlight';
import { icon, icons } from '@/lib/code/icons';
import { resolveLanguage } from '@/lib/code/languages';
import { fillLanguageSlot, languageSlot } from '@/lib/code/languageSlot';
import type { ExplorerLineRange } from '@/stores/explorerStore';

export interface CodeViewerProps {
  path: string;
  text: string;
  /** The backend's language name for the file (`rust`, `tsx`, …), used when the path alone does not say. */
  language: string | null;
  /** Lines the reader picked in the gutter. */
  selection: ExplorerLineRange | null;
  /** Lines an answer pointed at; scrolled to whenever `highlightNonce` changes. */
  highlight: ExplorerLineRange | null;
  highlightNonce: number | null;
  onSelectLines: (_range: ExplorerLineRange | null) => void;
}

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
  '.cm-line': { padding: '0 16px 0 10px' },
  '.cm-gutters': {
    color: 'var(--code-gutter-fg)',
    backgroundColor: 'var(--code-bg)',
    border: 'none',
    cursor: 'pointer',
    userSelect: 'none',
  },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 4px 0 14px', minWidth: '40px', textAlign: 'right' },
  '.cm-selectionBackground, ::selection': { backgroundColor: 'var(--code-selection) !important' },
  '&.cm-focused': { outline: 'none' },

  // Fold chevrons sit just right of the numbers: shown on hover, or while folded.
  '.cm-foldGutter .cm-gutterElement': { display: 'flex', alignItems: 'center', justifyContent: 'center', width: '16px' },
  '.cm-fold-marker': {
    display: 'flex',
    color: 'var(--code-gutter-fg)',
    opacity: '0',
    transition: 'opacity var(--duration-fast) var(--ease-out)',
  },
  '.cm-gutters:hover .cm-fold-marker, .cm-fold-marker-closed': { opacity: '1' },
  '.cm-fold-marker:hover': { color: 'var(--code-fg)' },
  '.cm-foldPlaceholder': {
    margin: '0 4px',
    padding: '0 6px',
    border: 'none',
    borderRadius: '4px',
    color: 'var(--code-gutter-fg)',
    backgroundColor: 'hsl(var(--text-primary) / 0.07)',
    fontFamily: 'var(--font-sans)',
    cursor: 'pointer',
  },
  '.cm-foldPlaceholder:hover': { color: 'var(--code-fg)', backgroundColor: 'hsl(var(--text-primary) / 0.12)' },

  // Bracket pairs light only while the reader is in the code, as in CodeMirror.
  '&.cm-focused .cm-matchingBracket': {
    backgroundColor: 'var(--code-match-bg)',
    outline: '1px solid hsl(var(--accent) / 0.45)',
    borderRadius: '2px',
  },
  '&.cm-focused .cm-nonmatchingBracket': { color: 'var(--code-invalid)' },

  '.cm-panels': { color: 'var(--code-fg)', backgroundColor: 'var(--code-bg)' },
  '.cm-panels-top': { borderBottom: '1px solid var(--code-divider)' },
  '.cm-searchMatch': { backgroundColor: 'hsl(var(--highlight))', borderRadius: '2px' },
  '.cm-searchMatch-selected': {
    backgroundColor: 'hsl(var(--highlight))',
    outline: '1.5px solid hsl(var(--warning))',
  },
});

function foldMarker(open: boolean): HTMLElement {
  const marker = document.createElement('span');
  marker.className = open ? 'cm-fold-marker' : 'cm-fold-marker cm-fold-marker-closed';
  marker.title = open ? 'Fold' : 'Unfold';
  marker.append(icon(open ? icons.chevronDown : icons.chevronRight, 12, 2.25));
  return marker;
}

/** Folding by the language's structure, with chevrons beside the numbers. */
const folding: Extension = [
  codeFolding(),
  foldGutter({ markerDOM: foldMarker }),
  keymap.of(foldKeymap),
];

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

/** Whether a key press belongs to something else: a field being typed in, or a dialog. */
const ownedElsewhere = (target: EventTarget | null) =>
  target instanceof Element && Boolean(target.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]'));

/**
 * The read-only file view: CodeMirror with line numbers, gutter selection,
 * the lines an answer points at, folding, bracket matching and ⌘F search.
 * The grammar loads on first use and colors the text in place.
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
    const codeLanguage = resolveLanguage(path, language);
    const view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: text,
        extensions: [
          gutterSelection((range) => onSelectRef.current(range), () => selectionRef.current),
          folding,
          languageSlot(codeLanguage),
          codeHighlighting,
          bracketMatching(),
          findPanel(),
          theme,
          marksField,
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          // Focusable, so ⌘F and the fold keys reach it after a click.
          EditorView.contentAttributes.of({ 'aria-label': `Contents of ${path}`, tabindex: '0' }),
        ],
      }),
    });
    viewRef.current = view;
    fillLanguageSlot(view, codeLanguage, () => viewRef.current === view);
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [language, path, text]);

  useEffect(() => {
    viewRef.current?.dispatch({ effects: setMarks.of({ selection, highlight }) });
  }, [selection, highlight, path, text]);

  // Scroll to a reference each time one is revealed, even the same one twice,
  // opening any fold that hides it.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !highlight || highlightNonce === null) return;
    const { doc } = view.state;
    const first = doc.line(Math.min(Math.max(1, highlight.startLine), doc.lines));
    const last = doc.line(Math.min(Math.max(first.number, highlight.endLine), doc.lines));
    const effects: StateEffect<unknown>[] = [];
    foldedRanges(view.state).between(first.from, last.to, (from, to) => {
      effects.push(unfoldEffect.of({ from, to }));
    });
    effects.push(EditorView.scrollIntoView(first.from, { y: 'center' }));
    view.dispatch({ effects });
  }, [highlightNonce, highlight, path, text]);

  // ⌘F finds in the open file from anywhere on the page, unless the reader
  // is typing somewhere else (the chat composer) or working in a dialog.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const view = viewRef.current;
      if (!view || event.defaultPrevented || event.altKey || event.shiftKey) return;
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'f' || ownedElsewhere(event.target)) return;
      event.preventDefault();
      openSearchPanel(view);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  return <div ref={hostRef} className="explorer-code h-full min-h-0 overflow-hidden" />;
}
