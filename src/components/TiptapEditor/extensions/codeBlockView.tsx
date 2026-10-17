import { Extension } from '@tiptap/core';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer } from '@tiptap/react';

import { MermaidFigure } from '../mermaid/MermaidFigure';

import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type { NodeViewProps } from '@tiptap/react';

export const diagramsKey = new PluginKey('diagrams');

export interface DiagramsOptions {
  /**
   * Read at draw time. True while an answer is still streaming and its last
   * fence is open: that code block is unfinished and is not drawn yet.
   */
  suspendLastBlock: () => boolean;
}

/**
 * Marks the code block an open fence is still writing.
 *
 * A node decoration rather than a flag the view reads: when the answer
 * finishes the text may not change at all, and only a changed decoration
 * makes ProseMirror hand the node view new props.
 */
export const Diagrams = Extension.create<DiagramsOptions>({
  name: 'diagrams',

  addOptions() {
    return { suspendLastBlock: () => false };
  },

  addProseMirrorPlugins() {
    const { suspendLastBlock } = this.options;
    return [
      new Plugin({
        key: diagramsKey,
        props: {
          decorations(state) {
            if (!suspendLastBlock()) return DecorationSet.empty;
            // An open fence runs to the end of the document, so its block is
            // the last textblock, at whatever depth (a list item, a quote).
            // The empty paragraph the editor keeps at the end does not count.
            let last: { node: ProseMirrorNode; pos: number } | null = null;
            state.doc.descendants((node, pos) => {
              if (!node.isTextblock) return true;
              if (node.content.size > 0 || node.type.name === 'codeBlock') last = { node, pos };
              return false;
            });
            const block = last as { node: ProseMirrorNode; pos: number } | null;
            if (block?.node.type.name !== 'codeBlock') return DecorationSet.empty;
            return DecorationSet.create(state.doc, [
              Decoration.node(block.pos, block.pos + block.node.nodeSize, { 'data-streaming': 'true' }, { streamingBlock: true }),
            ]);
          },
        },
      }),
    ];
  },
});

function languageOf(node: ProseMirrorNode): string | null {
  return typeof node.attrs.language === 'string' ? node.attrs.language : null;
}

/**
 * The code block ProseMirror would draw itself, built synchronously so the
 * code is in the DOM on the first render. Asking for a new view when the
 * language changes lets a block that becomes ```mermaid move to the diagram
 * view.
 */
function plainCodeBlock(node: ProseMirrorNode, prefix: string) {
  const language = languageOf(node);
  const dom = document.createElement('pre');
  const contentDOM = dom.appendChild(document.createElement('code'));
  if (language) contentDOM.className = `${prefix}${language}`;
  return {
    dom,
    contentDOM,
    update: (next: ProseMirrorNode) => next.type === node.type && languageOf(next) === language,
  };
}

function CodeBlockView({ node, editor, extension, decorations }: NodeViewProps) {
  const language = languageOf(node);
  const prefix = (extension.options as { languageClassPrefix?: string }).languageClassPrefix ?? 'language-';
  const code = <NodeViewContent<'code'> as="code" className={language ? `${prefix}${language}` : undefined} />;
  const streaming = decorations.some((decoration) => (decoration.spec as { streamingBlock?: boolean }).streamingBlock);

  if (language !== 'mermaid' || editor.isEditable || streaming) {
    return <NodeViewWrapper as="pre">{code}</NodeViewWrapper>;
  }
  return (
    <NodeViewWrapper as="figure" className="diagram-figure">
      <MermaidFigure source={node.textContent}>{code}</MermaidFigure>
    </NodeViewWrapper>
  );
}

/**
 * The lowlight code block, drawn by a node view that turns a closed ```mermaid
 * fence into a diagram in read-only editors. An editor created editable (the
 * notes editor) keeps ProseMirror's own code block: diagrams are for reading,
 * and a React node view would change how the block edits. Other languages get
 * a plain view: a React view mounts a tick late and costs a React root per
 * block, which every code block in a long answer would pay.
 */
export const CodeBlockWithDiagrams = CodeBlockLowlight.extend({
  addNodeView() {
    if (this.editor.options.editable) return null;
    const diagramView = ReactNodeViewRenderer(CodeBlockView, { contentDOMElementTag: 'span' });
    const prefix = this.options.languageClassPrefix ?? 'language-';
    return (props) => (languageOf(props.node) === 'mermaid' ? diagramView(props) : plainCodeBlock(props.node, prefix));
  },
});
