import { useEffect, useRef } from 'react';

import { useEditor, EditorContent } from '@tiptap/react';
import { Markdown } from 'tiptap-markdown';

import { createExtensions } from './extensions';
import { CitationMarks, citationMarksKey } from './extensions/citationMarks';
import { ClaimMarks, claimMarksKey, type ClaimMark } from './extensions/claimMarks';
import { CodeRefMarks, codeRefMarksKey } from './extensions/codeRefMarks';
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
  /** Presentation-only clean reading mode. The markdown and evidence remain intact. */
  showEvidence?: boolean;
  /** Draw `path:line` inline code as line-reference chips (Explorer conversations). */
  codeRefs?: boolean;
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
  showEvidence = true,
  codeRefs = false,
}: TiptapViewerProps) {
  // Read through a ref so sources that arrive after the text still light up.
  const citationNumbersRef = useRef<readonly number[]>(citationNumbers ?? []);
  const evidenceVisibleRef = useRef(showEvidence);
  const citationSignature = `${showEvidence ? 'visible' : 'hidden'}:${(citationNumbers ?? []).join(',')}`;
  const claimsRef = useRef<readonly ClaimMark[]>(showEvidence ? claims ?? NO_CLAIMS : NO_CLAIMS);
  const codeRefsRef = useRef(codeRefs);

  const editor = useEditor({
    extensions: [
      ...createExtensions({ onWikilinkClick }),
      CitationMarks.configure({
        isCitation: (number) => citationNumbersRef.current.includes(number),
        isVisible: () => evidenceVisibleRef.current,
      }),
      ClaimMarks.configure({ getClaims: () => claimsRef.current }),
      CodeRefMarks.configure({ isEnabled: () => codeRefsRef.current }),
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
    // useEditor can replace an instance after this render captured it. Its
    // destroyed instance has no command manager; wait for the replacement
    // render, which reruns this effect with the latest content.
    if (!editor || editor.isDestroyed) return;
    const currentMd = getMarkdownFromEditor(editor);
    if (currentMd !== content) {
      editor.commands.setContent(content);
    }
  }, [editor, content]);

  useEffect(() => {
    citationNumbersRef.current = citationNumbers ?? [];
    evidenceVisibleRef.current = showEvidence;
    if (!editor || editor.isDestroyed) return;
    // An empty transaction: nothing changes but the decorations are redrawn.
    editor.view.dispatch(editor.state.tr.setMeta(citationMarksKey, citationSignature));
  }, [editor, citationNumbers, citationSignature, showEvidence]);

  useEffect(() => {
    const previous = claimsRef.current;
    claimsRef.current = showEvidence ? claims ?? NO_CLAIMS : NO_CLAIMS;
    if (!editor || editor.isDestroyed) return;
    // Most messages have no verdicts and never will; they need no redraw.
    if (previous.length === 0 && claimsRef.current.length === 0) return;
    editor.view.dispatch(editor.state.tr.setMeta(claimMarksKey, true));
  }, [editor, claims, showEvidence]);

  useEffect(() => {
    if (codeRefsRef.current === codeRefs) return;
    codeRefsRef.current = codeRefs;
    if (!editor || editor.isDestroyed) return;
    editor.view.dispatch(editor.state.tr.setMeta(codeRefMarksKey, codeRefs));
  }, [editor, codeRefs]);

  if (!editor) return null;

  return <EditorContent editor={editor} />;
}

TiptapViewer.displayName = 'TiptapViewer';
