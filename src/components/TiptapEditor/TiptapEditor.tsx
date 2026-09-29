import { useCallback, useEffect, useRef } from 'react';

import { useEditor, EditorContent } from '@tiptap/react';
import { Markdown } from 'tiptap-markdown';

import { createExtensions } from './extensions';
import { CitationMarks, citationMarksKey } from './extensions/citationMarks';
import { getMarkdownFromEditor } from './markdownStorage';
import { BubbleMenuBar, type SelectionAction } from './menus/BubbleMenuBar';
import { SlashMenu, type SlashHandlers } from './menus/SlashMenu';

import type { Editor } from '@tiptap/react';

import './tiptap.css';

export interface TiptapEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  placeholder?: string;
  className?: string;
  onWikilinkClick?: (title: string) => void;
  autofocus?: boolean;
  /** Extra verbs for the selection toolbar, after the formatting marks. */
  selectionActions?: SelectionAction[];
  /** Source numbers that should turn `[n]` text into clickable citation chips. */
  citationNumbers?: readonly number[];
  onCitationClick?: (number: number, occurrence: number) => void;
}

const SKIP_SYNC_META = 'externalSync';

/**
 * Editable Tiptap editor with markdown I/O.
 * Maintains the same value/onChange contract as the old textarea-based MarkdownEditor.
 */
export function TiptapEditor({
  value,
  onChange,
  placeholder,
  className = '',
  onWikilinkClick,
  autofocus = false,
  selectionActions,
  citationNumbers = [],
  onCitationClick,
}: TiptapEditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const citationNumbersRef = useRef<readonly number[]>(citationNumbers);
  citationNumbersRef.current = citationNumbers;
  const onCitationClickRef = useRef(onCitationClick);
  onCitationClickRef.current = onCitationClick;
  const citationSignature = citationNumbers.join(',');

  // The slash extension needs its callbacks at `createExtensions` time, and
  // `useEditor` must not be re-created on every keystroke — so the plugin talks
  // to a stable ref that `SlashMenu` fills in.
  const slashHandlers = useRef<SlashHandlers>({
    onStateChange: () => {},
    onKeyDown: () => false,
  });

  const handleUpdate = useCallback(
    ({ editor, transaction }: { editor: Editor; transaction: { getMeta: (key: string) => unknown } }) => {
      if (transaction.getMeta(SKIP_SYNC_META)) return;
      onChangeRef.current(getMarkdownFromEditor(editor));
    },
    [],
  );

  const editor = useEditor({
    extensions: [
      ...createExtensions({
        placeholder,
        onWikilinkClick,
        slashMenu: {
          onStateChange: (state) => slashHandlers.current.onStateChange(state),
          onKeyDown: (key) => slashHandlers.current.onKeyDown(key),
        },
      }),
      CitationMarks.configure({
        isCitation: (number) => citationNumbersRef.current.includes(number),
        onCitationClick: (number, occurrence) => onCitationClickRef.current?.(number, occurrence),
      }),
      Markdown.configure({
        html: false,
        transformPastedText: true,
        transformCopiedText: true,
      }),
    ],
    content: value,
    editable: true,
    autofocus: autofocus ? 'end' : false,
    onUpdate: handleUpdate,
    editorProps: {
      attributes: {
        class: `tiptap-editor focus:outline-none ${className}`,
      },
    },
  });

  // Sync external value changes into the editor using transaction metadata
  // to prevent the onUpdate handler from firing back.
  useEffect(() => {
    if (!editor) return;
    const currentMd = getMarkdownFromEditor(editor);
    if (currentMd !== value) {
      const { tr } = editor.state;
      tr.setMeta(SKIP_SYNC_META, true);
      editor.view.dispatch(tr);
      editor.commands.setContent(value);
    }
  }, [editor, value]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.view.dispatch(editor.state.tr.setMeta(citationMarksKey, citationSignature));
  }, [editor, citationSignature]);

  if (!editor) return null;

  return (
    <>
      <EditorContent editor={editor} />
      <BubbleMenuBar editor={editor} selectionActions={selectionActions} />
      <SlashMenu editor={editor} handlersRef={slashHandlers} />
    </>
  );
}

TiptapEditor.displayName = 'TiptapEditor';
