import { useEffect, useRef, useState } from 'react';

import { useEditor, EditorContent } from '@tiptap/react';
import { Markdown } from 'tiptap-markdown';

import { MediaLightbox } from '@/components/MediaLightbox';
import { hasUnclosedFence } from '@/utils/assistantMarkdown';

import { createExtensions } from './extensions';
import { chatImagesKey, figureCredit, type ChatFigure, type ChatImageHost } from './extensions/chatImage';
import { CitationMarks, citationMarksKey } from './extensions/citationMarks';
import { ClaimMarks, claimMarksKey, type ClaimMark } from './extensions/claimMarks';
import { Diagrams, diagramsKey } from './extensions/codeBlockView';
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
  /** The text is still arriving. A fence it has not closed yet is not drawn as a diagram. */
  streaming?: boolean;
  /** Draw ```mermaid fences as diagrams (default on). Read when the viewer is created. */
  diagrams?: boolean;
  /**
   * A chat answer's pictures, for its `![caption](img-N)` refs. Given (even
   * empty), no other image address is loaded; absent, images render plainly.
   * Whether it is given is read when the viewer is created.
   */
  images?: readonly ChatFigure[];
}

const NO_CLAIMS: readonly ClaimMark[] = [];
const NO_FIGURES: readonly ChatFigure[] = [];

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
  streaming = false,
  diagrams = true,
  images,
}: TiptapViewerProps) {
  // Read through a ref so sources that arrive after the text still light up.
  const citationNumbersRef = useRef<readonly number[]>(citationNumbers ?? []);
  const evidenceVisibleRef = useRef(showEvidence);
  const citationSignature = `${showEvidence ? 'visible' : 'hidden'}:${(citationNumbers ?? []).join(',')}`;
  const claimsRef = useRef<readonly ClaimMark[]>(showEvidence ? claims ?? NO_CLAIMS : NO_CLAIMS);
  const codeRefsRef = useRef(codeRefs);
  const suspendLastBlock = streaming && hasUnclosedFence(content);
  const suspendLastBlockRef = useRef(suspendLastBlock);
  const figuresRef = useRef<readonly ChatFigure[]>(images ?? NO_FIGURES);
  const streamingRef = useRef(streaming);
  const [openFigure, setOpenFigure] = useState<ChatFigure | null>(null);
  const [chatImages] = useState<ChatImageHost | undefined>(() => images === undefined ? undefined : {
    figures: () => figuresRef.current,
    pending: () => streamingRef.current,
    citationsVisible: () => evidenceVisibleRef.current,
    onOpen: setOpenFigure,
  });

  const editor = useEditor({
    extensions: [
      ...createExtensions({ onWikilinkClick, diagrams, chatImages }),
      Diagrams.configure({ suspendLastBlock: () => suspendLastBlockRef.current }),
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

  // Before the content effect below: a chunk that opens a fence must arrive
  // already marked unfinished, or its partial source would start a drawing.
  useEffect(() => {
    if (suspendLastBlockRef.current === suspendLastBlock) return;
    suspendLastBlockRef.current = suspendLastBlock;
    if (!editor || editor.isDestroyed) return;
    editor.view.dispatch(editor.state.tr.setMeta(diagramsKey, suspendLastBlock));
  }, [editor, suspendLastBlock]);

  // Also before the content effect: text naming `img-2` must find it.
  useEffect(() => {
    const figures = images ?? NO_FIGURES;
    if (figuresRef.current === figures && streamingRef.current === streaming) return;
    figuresRef.current = figures;
    streamingRef.current = streaming;
    if (!chatImages || !editor || editor.isDestroyed) return;
    editor.view.dispatch(editor.state.tr.setMeta(chatImagesKey, true));
  }, [editor, chatImages, images, streaming]);

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
    // Pictures' caption chips follow the same switch.
    const tr = editor.state.tr.setMeta(citationMarksKey, citationSignature);
    editor.view.dispatch(chatImages ? tr.setMeta(chatImagesKey, true) : tr);
  }, [editor, chatImages, citationNumbers, citationSignature, showEvidence]);

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

  return (
    <>
      <EditorContent editor={editor} />
      {openFigure && (
        <MediaLightbox
          open
          onOpenChange={(open) => { if (!open) setOpenFigure(null); }}
          title={openFigure.sourceTitle ?? 'Picture'}
          caption={(
            <>
              {openFigure.caption}
              {figureCredit(openFigure) && <span className="media-lightbox-credit">{figureCredit(openFigure)}</span>}
            </>
          )}
          actions={(
            <a href={openFigure.sourceUrl} target="_blank" rel="noopener noreferrer" className="media-lightbox-action">
              Open source page
            </a>
          )}
        >
          <img
            className="media-lightbox-image"
            src={openFigure.url}
            alt={openFigure.caption}
            width={openFigure.width}
            height={openFigure.height}
          />
        </MediaLightbox>
      )}
    </>
  );
}

TiptapViewer.displayName = 'TiptapViewer';
