import { useEditor, EditorContent } from '@tiptap/react';
import { useEffect } from 'react';
import { Markdown } from 'tiptap-markdown';

import { createExtensions } from './extensions';
import { getMarkdownFromEditor } from './markdownStorage';

import './tiptap.css';

export interface TiptapViewerProps {
  content: string;
  className?: string;
  onWikilinkClick?: (title: string) => void;
}

/**
 * Read-only Tiptap renderer for displaying markdown content.
 * html is disabled — Tiptap's ProseMirror schema only renders known node types,
 * and tiptap-markdown with html:false strips raw HTML from input.
 */
export function TiptapViewer({
  content,
  className = '',
  onWikilinkClick,
}: TiptapViewerProps) {
  const editor = useEditor({
    extensions: [
      ...createExtensions({ onWikilinkClick }),
      Markdown.configure({
        html: false,
        transformPastedText: false,
        transformCopiedText: false,
      }),
    ],
    content,
    editable: false,
    editorProps: {
      attributes: {
        class: `tiptap-viewer ${className}`,
      },
    },
  });

  useEffect(() => {
    if (editor && content !== undefined) {
      const currentMd = getMarkdownFromEditor(editor);
      if (currentMd !== content) {
        editor.commands.setContent(content);
      }
    }
  }, [editor, content]);

  if (!editor) return null;

  return <EditorContent editor={editor} />;
}

TiptapViewer.displayName = 'TiptapViewer';
