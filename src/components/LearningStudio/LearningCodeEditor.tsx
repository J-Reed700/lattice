import { useLayoutEffect, useRef } from 'react';

import { indentWithTab, redo, undo } from '@codemirror/commands';
import { bracketMatching, indentUnit } from '@codemirror/language';
import { openSearchPanel } from '@codemirror/search';
import { Annotation, EditorState, Prec, StateEffect, Transaction } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { basicSetup } from 'codemirror';
import { Search } from 'lucide-react';

import { codeHighlighting } from '@/lib/code/highlight';
import { PLAIN_TEXT_LABEL, resolveLanguage, type CodeLanguage } from '@/lib/code/languages';
import { fillLanguageSlot, languageSlot } from '@/lib/code/languageSlot';

import './learningCodeEditor.css';

export interface LearningCodeEditorProps {
  /** The active file path. Its name or extension selects the language. */
  path: string;
  /** The parent-owned file contents. */
  value: string;
  /** Receives edits made by the learner. */
  onChange: (value: string) => void;
  /** Prevents all document edits while preserving search and selection. */
  readOnly?: boolean;
  /** Accessible name for the code editing region. */
  ariaLabel?: string;
  /** Minimum editor height in pixels or CSS units. */
  minHeight?: number | string;
  /** Hides the component path label when a parent file tab already identifies the file. */
  showPath?: boolean;
  /** Runs when the learner presses Mod-Enter (Cmd-Enter on macOS). */
  onRunShortcut?: () => void;
}

const externalDocumentSync = Annotation.define<boolean>();

const editorTheme = EditorView.theme({
  '&': {
    color: 'var(--code-fg)',
    backgroundColor: 'var(--code-bg)',
    fontSize: '13px',
    fontFamily: 'var(--font-mono, "JetBrains Mono Variable", "JetBrains Mono", ui-monospace, SFMono-Regular, monospace)',
  },
  '.cm-content': { caretColor: 'var(--code-caret)', padding: '15px 0 22px' },
  '.cm-line': { padding: '0 18px', lineHeight: '1.72' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--code-caret)', borderLeftWidth: '2px' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--code-selection) !important' },
  '.cm-gutters': {
    color: 'var(--code-gutter-fg)',
    backgroundColor: 'var(--code-gutter-bg)',
    border: 'none',
    borderRight: '1px solid var(--code-divider)',
    minWidth: '48px',
  },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 12px 0 8px', minWidth: '48px', textAlign: 'right' },
  '.cm-activeLine': { backgroundColor: 'var(--code-active-line)' },
  '.cm-activeLineGutter': { color: 'var(--code-gutter-active)', backgroundColor: 'var(--code-active-line)' },
  '.cm-matchingBracket': { color: 'var(--code-match-fg) !important', backgroundColor: 'var(--code-match-bg)' },
  '.cm-nonmatchingBracket': { color: 'var(--code-invalid) !important', backgroundColor: 'var(--code-match-bg)' },
  '.cm-searchMatch': { backgroundColor: 'var(--code-match-bg)' },
  '.cm-searchMatch-selected': { outline: '1px solid var(--code-caret)' },
  '.cm-panels': { color: 'var(--code-fg)', backgroundColor: 'var(--code-panel)', borderColor: 'var(--code-divider)' },
  '.cm-panels input, .cm-panels button': { color: 'var(--code-fg)', backgroundColor: 'var(--code-control)', borderColor: 'var(--code-divider)' },
  '.cm-tooltip': { color: 'var(--code-fg)', backgroundColor: 'var(--code-panel)', borderColor: 'var(--code-divider)' },
  '&.cm-focused': { outline: 'none' },
  '&.cm-focused .cm-content': { outline: 'none' },
});

function buildExtensions(language: CodeLanguage | null, readOnly: boolean, onChange: (value: string) => void, ariaLabel: string, onRunShortcut?: () => void) {
  return [
    basicSetup,
    languageSlot(language),
    codeHighlighting,
    editorTheme,
    indentUnit.of('  '),
    EditorView.lineWrapping,
    EditorState.readOnly.of(readOnly),
    EditorView.editable.of(!readOnly),
    bracketMatching(),
    Prec.high(keymap.of([
      indentWithTab,
      { key: 'Mod-z', run: undo },
      { key: 'Shift-Mod-z', run: redo },
      ...(onRunShortcut ? [{ key: 'Mod-Enter', run: () => { onRunShortcut(); return true; } }] : []),
    ])),
    EditorView.contentAttributes.of({
      'aria-label': ariaLabel,
      'aria-multiline': 'true',
      spellcheck: 'false',
      autocapitalize: 'off',
      autocomplete: 'off',
      autocorrect: 'off',
    }),
    EditorView.updateListener.of((update) => {
      if (update.docChanged && !update.transactions.some((transaction) => transaction.annotation(externalDocumentSync))) {
        onChange(update.state.doc.toString());
      }
    }),
  ];
}

/** A small, fully local CodeMirror editor for Learning Studio files. */
export function LearningCodeEditor({
  path,
  value,
  onChange,
  readOnly = false,
  ariaLabel,
  minHeight = 300,
  showPath = true,
  onRunShortcut,
}: LearningCodeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const latestPropsRef = useRef({ onChange, onRunShortcut, ariaLabel });
  const activePathRef = useRef(path);
  const activeAriaLabelRef = useRef(ariaLabel);
  const pathGenerationRef = useRef(0);
  const activeLanguage = resolveLanguage(path);
  const languageRef = useRef(activeLanguage);
  const readOnlyRef = useRef(readOnly);

  latestPropsRef.current = { onChange, onRunShortcut, ariaLabel };

  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const state = EditorState.create({
      doc: value,
      extensions: buildExtensions(
        activeLanguage,
        readOnly,
        (nextValue) => {
          if (pathGenerationRef.current === 0) latestPropsRef.current.onChange(nextValue);
        },
        ariaLabel ?? `Code editor for ${path || 'untitled file'}`,
        () => { if (pathGenerationRef.current === 0) latestPropsRef.current.onRunShortcut?.(); },
      ),
    });
    const view = new EditorView({ state, parent: host });
    viewRef.current = view;
    activePathRef.current = path;
    activeAriaLabelRef.current = ariaLabel;
    languageRef.current = activeLanguage;
    readOnlyRef.current = readOnly;
    fillLanguageSlot(view, activeLanguage, () => viewRef.current === view && languageRef.current === activeLanguage);

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // A view owns its editing state. Changes synchronize through the next
    // layout effect rather than recreating the view on each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useLayoutEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const pathChanged = activePathRef.current !== path;
    const languageChanged = languageRef.current !== activeLanguage;
    const readOnlyChanged = readOnlyRef.current !== readOnly;
    const ariaLabelChanged = activeAriaLabelRef.current !== ariaLabel;
    const latestValue = view.state.doc.toString();

    if (pathChanged) {
      // A file switch gets a fresh state so undo cannot move edits across files.
      pathGenerationRef.current += 1;
      const generation = pathGenerationRef.current;
      view.setState(EditorState.create({
        doc: value,
        extensions: buildExtensions(
          activeLanguage,
          readOnly,
          (nextValue) => {
            if (pathGenerationRef.current === generation) latestPropsRef.current.onChange(nextValue);
          },
          ariaLabel ?? `Code editor for ${path || 'untitled file'}`,
          () => { if (pathGenerationRef.current === generation) latestPropsRef.current.onRunShortcut?.(); },
        ),
      }));
    } else {
      if (languageChanged || readOnlyChanged || ariaLabelChanged) {
        const generation = pathGenerationRef.current;
        view.dispatch({ effects: StateEffect.reconfigure.of(buildExtensions(
        activeLanguage,
        readOnly,
        (nextValue) => {
          if (pathGenerationRef.current === generation) latestPropsRef.current.onChange(nextValue);
        },
        ariaLabel ?? `Code editor for ${path || 'untitled file'}`,
        () => { if (pathGenerationRef.current === generation) latestPropsRef.current.onRunShortcut?.(); },
        )) });
      }

      if (latestValue !== value) {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: value },
          annotations: [externalDocumentSync.of(true), Transaction.addToHistory.of(false)],
        });
      }
    }

    activePathRef.current = path;
    activeAriaLabelRef.current = ariaLabel;
    languageRef.current = activeLanguage;
    readOnlyRef.current = readOnly;
    // A rebuilt state starts without a grammar that is still loading.
    if (pathChanged || languageChanged || readOnlyChanged || ariaLabelChanged) {
      fillLanguageSlot(view, activeLanguage, () => viewRef.current === view && languageRef.current === activeLanguage);
    }
  }, [activeLanguage, ariaLabel, path, readOnly, value]);

  return (
    <section className="learning-code-editor min-w-0 overflow-hidden rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] shadow-sm" aria-label={`${path} editor`}>
      <header className="learning-code-editor__toolbar flex min-h-11 items-center justify-between gap-3 border-b border-[hsl(var(--border-subtle))] px-3 sm:px-4">
        <div className="flex min-w-0 items-center gap-2">
          <span className="learning-code-editor__dot" aria-hidden="true" />
          {showPath && <span className="truncate font-mono text-xs text-[hsl(var(--text-secondary))]" title={path}>{path || 'Untitled'}</span>}
          <span className="learning-code-editor__language shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold uppercase tracking-[.12em]">{activeLanguage?.label ?? PLAIN_TEXT_LABEL}</span>
          {readOnly && <span className="learning-code-editor__readonly shrink-0 text-[11px] font-semibold uppercase tracking-[.12em]">Read only</span>}
        </div>
        <button
          type="button"
          className="learning-code-editor__find inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2 text-xs font-medium transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
          aria-label="Find in code"
          title="Find in code (⌘/Ctrl+F)"
          onClick={() => {
            const view = viewRef.current;
            if (!view) return;
            openSearchPanel(view);
          }}
        >
          <Search size={13} aria-hidden="true" />
          <span className="hidden sm:inline">Find</span>
          <kbd className="hidden rounded border px-1 py-0.5 font-mono text-[11px] sm:inline">⌘F</kbd>
        </button>
      </header>
      <div
        className="learning-code-editor__body overflow-auto"
        style={{ minHeight: typeof minHeight === 'number' ? `${minHeight}px` : minHeight, maxHeight: '65vh' }}
        aria-label={ariaLabel ?? `Code editor for ${path || 'untitled file'}`}
      >
        <div ref={hostRef} />
      </div>
      <footer className="learning-code-editor__hint flex flex-wrap items-center justify-between gap-x-3 gap-y-1 border-t border-[hsl(var(--border-subtle))] px-3 py-2 text-[11px] text-[hsl(var(--text-muted))] sm:px-4">
        <span>{readOnly ? 'Read-only file' : 'Tab indents · ⌘/Ctrl+Z undo · ⌘/Ctrl+F find'}</span>
        {!readOnly && <span>Press Esc, then Tab to move focus</span>}
      </footer>
    </section>
  );
}

export default LearningCodeEditor;
