import { BlockMath, InlineMath } from '@tiptap/extension-mathematics';
import 'katex/dist/katex.min.css';

import { addMathSyntax, mathSource } from './mathSyntax';

import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type MarkdownIt from 'markdown-it';

/** What the math nodes ask of tiptap-markdown's serializer. */
interface MarkdownWriter {
  text(text: string, escape?: boolean): void;
  closeBlock(node: ProseMirrorNode): void;
}

/** The opening delimiter it was written with, so it serialises back as written. */
const delimiter = (written: string) => ({
  delimiter: {
    default: written,
    parseHTML: (element: HTMLElement) => element.getAttribute('data-delimiter') ?? written,
    renderHTML: (attributes: Record<string, unknown>) => ({ 'data-delimiter': attributes.delimiter }),
  },
});

const source = (node: ProseMirrorNode) =>
  mathSource(String(node.attrs.latex ?? ''), String(node.attrs.delimiter ?? '$'));

/**
 * A formula inside a line. Its text is its source, `$…$`, so copying,
 * `getText()` and the claim marks read the formula as it was written.
 */
const MathInline = InlineMath.extend({
  addAttributes() {
    return { ...this.parent?.(), ...delimiter('$') };
  },
  renderText({ node }) {
    return `$${String(node.attrs.latex ?? '')}$`;
  },
  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownWriter, node: ProseMirrorNode) {
          state.text(source(node), false);
        },
        parse: {
          setup(md: MarkdownIt) {
            addMathSyntax(md);
          },
        },
      },
    };
  },
});

/** A displayed formula on lines of its own; its text is `$$…$$`. */
const MathBlock = BlockMath.extend({
  addAttributes() {
    return { ...this.parent?.(), ...delimiter('$$') };
  },
  renderText({ node }) {
    return `$$${String(node.attrs.latex ?? '')}$$`;
  },
  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownWriter, node: ProseMirrorNode) {
          state.text(source(node), false);
          state.closeBlock(node);
        },
        parse: {
          setup(md: MarkdownIt) {
            addMathSyntax(md);
          },
        },
      },
    };
  },
});

/**
 * TeX math for the read-only viewer. Not for the note editor: re-serialising
 * a note would rewrite the dollars a user typed as prose.
 *
 * KaTeX throws on LaTeX it cannot read; the nodes catch it and show the
 * source instead (`.inline-math-error`, `.block-math-error`). `strict: false`
 * keeps Unicode symbols in a formula (∩, ≤) from warning in the console.
 * `trust` stays off, so a formula cannot load a link or a picture.
 */
export const ViewerMath = [
  MathInline.configure({ katexOptions: { throwOnError: true, strict: false } }),
  MathBlock.configure({ katexOptions: { displayMode: true, throwOnError: true, strict: false } }),
];

const FORMULA = '[data-type="inline-math"][data-latex], [data-type="block-math"][data-latex]';
const TEXT_BLOCKS = 'p, li, h1, h2, h3, h4, h5, h6, pre, blockquote, tr';

/**
 * The text of rendered markdown with each formula read as its source, or
 * null when it shows none (the caller's own reading is then exact). KaTeX
 * draws a formula twice, as MathML and as glyph boxes, so the page's own
 * text reads it three times over and never as it was written.
 */
export function textWithFormulas(root: Element | DocumentFragment): string | null {
  if (!root.querySelector(FORMULA)) return null;
  const copy = root.cloneNode(true) as Element | DocumentFragment;
  copy.querySelectorAll(FORMULA).forEach((formula) => formula.replaceWith(formulaSource(formula)));
  copy.querySelectorAll(TEXT_BLOCKS).forEach((block) => block.append('\n'));
  return copy.textContent ?? '';
}

/** {@link textWithFormulas} for a selection, which may lie inside one formula. */
export function selectedTextWithFormulas(range: Range): string | null {
  const container = range.commonAncestorContainer;
  const inside = (container instanceof Element ? container : container.parentElement)?.closest(FORMULA);
  return inside ? formulaSource(inside).trim() : textWithFormulas(range.cloneContents());
}

function formulaSource(formula: Element): string {
  const latex = formula.getAttribute('data-latex') ?? '';
  return formula.getAttribute('data-type') === 'block-math' ? `\n$$${latex}$$\n` : `$${latex}$`;
}
