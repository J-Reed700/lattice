import type { Editor } from '@tiptap/react';
import { useEditor, EditorContent } from '@tiptap/react';
import { useCallback, useEffect, useRef } from 'react';
import { Markdown } from 'tiptap-markdown';

import { createExtensions } from './extensions';
import { getMarkdownFromEditor } from './markdownStorage';

import './tiptap.css';

export interface TiptapEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  placeholder?: string;
  className?: string;
  onWikilinkClick?: (title: string) => void;
  autofocus?: boolean;
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
}: TiptapEditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const handleUpdate = useCallback(
    ({ editor, transaction }: { editor: Editor; transaction: { getMeta: (key: string) => unknown } }) => {
      if (transaction.getMeta(SKIP_SYNC_META)) return;
      onChangeRef.current(getMarkdownFromEditor(editor));
    },
    [],
  );

  const editor = useEditor({
    extensions: [
      ...createExtensions({ placeholder, onWikilinkClick }),
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

  if (!editor) return null;

  return <EditorContent editor={editor} />;
}

TiptapEditor.displayName = 'TiptapEditor';
