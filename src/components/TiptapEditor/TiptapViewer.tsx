import { useEffect, useRef } from 'react';

import { useEditor, EditorContent } from '@tiptap/react';
import { Markdown } from 'tiptap-markdown';

import { createExtensions } from './extensions';
import { CitationMarks, citationMarksKey } from './extensions/citationMarks';
import { ClaimMarks, claimMarksKey, type ClaimMark } from './extensions/claimMarks';
import { getMarkdownFromEditor } from './markdownStorage';

import './tiptap.css';

export interface TiptapViewerProps {
  content: string;
  className?: string;
  onWikilinkClick?: (title: string) => void;
  /** Source numbers that `[n]` may refer to. When given, they render as citation chips. */
  citationNumbers?: readonly number[];
  /** Checked sentences of this answer. When given, each is drawn with its verdict. */
  claims?: readonly ClaimMark[];
}

const NO_CLAIMS: readonly ClaimMark[] = [];

/**
 * Read-only Tiptap renderer for displaying markdown content.
 * html is disabled — Tiptap's ProseMirror schema only renders known node types,
 * and tiptap-markdown with html:false strips raw HTML from input.
 */
export function TiptapViewer({
  content,
  className = '',
  onWikilinkClick,
  citationNumbers,
  claims,
}: TiptapViewerProps) {
  // Read through a ref so sources that arrive after the text still light up.
  const citationNumbersRef = useRef<readonly number[]>(citationNumbers ?? []);
  const citationSignature = (citationNumbers ?? []).join(',');
  const claimsRef = useRef<readonly ClaimMark[]>(claims ?? NO_CLAIMS);

  const editor = useEditor({
    extensions: [
      ...createExtensions({ onWikilinkClick }),
      CitationMarks.configure({
        isCitation: (number) => citationNumbersRef.current.includes(number),
      }),
      ClaimMarks.configure({ getClaims: () => claimsRef.current }),
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

  useEffect(() => {
    citationNumbersRef.current = citationSignature.split(',').filter(Boolean).map(Number);
    if (!editor || editor.isDestroyed) return;
    // An empty transaction: nothing changes but the decorations are redrawn.
    editor.view.dispatch(editor.state.tr.setMeta(citationMarksKey, citationSignature));
  }, [editor, citationSignature]);

  useEffect(() => {
    const previous = claimsRef.current;
    claimsRef.current = claims ?? NO_CLAIMS;
    if (!editor || editor.isDestroyed) return;
    // Most messages have no verdicts and never will; they need no redraw.
    if (previous.length === 0 && claimsRef.current.length === 0) return;
    editor.view.dispatch(editor.state.tr.setMeta(claimMarksKey, true));
  }, [editor, claims]);

  if (!editor) return null;

  return <EditorContent editor={editor} />;
}

TiptapViewer.displayName = 'TiptapViewer';
