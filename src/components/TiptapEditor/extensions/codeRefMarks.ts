import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

import { fileRefFromHref, scanFileRefs, type FileRefMatch, type LineRange } from './fileRefs';

import type { Node as ProseMirrorNode } from '@tiptap/pm/model';

/**
 * A line reference, with a path relative to the Explorer's scope. The backend
 * prompt asks for `path:10-24` in backticks; `fileRefs` reads the many other
 * ways models write them too.
 */
export interface CodeRef extends LineRange {
  path: string;
}

/**
 * Reads a line reference out of the whole content of one inline code span:
 * `src/main.rs:10-24`, `src/main.rs#L12`, `src/main.rs (line 3)` and the like.
 * `null` for anything else, a path without lines included.
 */
export function parseCodeRef(text: string): CodeRef | null {
  const trimmed = text.trim();
  const refs = scanFileRefs(trimmed, () => true);
  const [ref] = refs;
  if (refs.length !== 1 || !ref.lines || ref.start !== 0 || ref.end !== trimmed.length) return null;
  return { path: ref.path, ...ref.lines };
}

/**
 * Every line reference in a markdown answer, in order, however it is written.
 * Fenced blocks are code, not references. While the answer streams, a
 * reference that runs to the end of the text may still be growing
 * (`src/a.rs:1` on its way to `:12`), so it waits until something follows it.
 */
export function findCodeRefs(markdown: string, { complete = true }: { complete?: boolean } = {}): CodeRef[] {
  const text = markdown.replace(/```[\s\S]*?(```|$)/g, '');
  return scanFileRefs(text, () => true)
    .filter((ref) => complete || ref.end < text.length)
    .flatMap((ref) => (ref.lines ? [{ path: ref.path, ...ref.lines }] : []));
}

export const codeRefMarksKey = new PluginKey('codeRefMarks');

export interface CodeRefMarksOptions {
  /** Read at draw time, so a host can switch the chips on after mounting. */
  isEnabled: () => boolean;
}

function chipAttrs(ref: FileRefMatch): Record<string, string> {
  const { path, lines } = ref;
  const attrs: Record<string, string> = { class: 'code-ref-chip', 'data-code-ref': path, role: 'button' };
  if (lines) {
    attrs['data-code-ref-start'] = String(lines.startLine);
    attrs['data-code-ref-end'] = String(lines.endLine);
    const said = lines.startLine === lines.endLine ? `line ${lines.startLine}` : `lines ${lines.startLine}–${lines.endLine}`;
    attrs['aria-label'] = `Open ${path}, ${said}`;
  } else {
    attrs['aria-label'] = `Open ${path}`;
  }
  return attrs;
}

/**
 * The references in one paragraph, list item or table cell, read as a whole
 * so that one split by formatting — `**src/main.rs**:12`, `` `main.rs` line 4 ``,
 * "lines 3–9 of `lib.rs`" — still reads as one. A link pointing into the
 * folder is a reference by its address, whatever its text says.
 */
function blockDecorations(block: ProseMirrorNode, contentStart: number): Decoration[] {
  const text = block.textBetween(0, block.content.size, '\n', '￼');
  const inCode: boolean[] = [];
  const links: { from: number; to: number; href: string }[] = [];
  block.forEach((child, offset) => {
    if (!child.isText || !child.text) return;
    const code = child.marks.some((mark) => mark.type.spec.code);
    for (let index = 0; index < child.text.length; index += 1) inCode[offset + index] = code;
    const link = child.marks.find((mark) => mark.type.name === 'link');
    const href = typeof link?.attrs.href === 'string' ? link.attrs.href : null;
    if (!href) return;
    const previous = links[links.length - 1];
    if (previous?.to === offset && previous.href === href) previous.to = offset + child.nodeSize;
    else links.push({ from: offset, to: offset + child.nodeSize, href });
  });

  const decorations: Decoration[] = [];
  for (const link of links) {
    const ref = fileRefFromHref(link.href);
    if (ref) decorations.push(Decoration.inline(contentStart + link.from, contentStart + link.to, chipAttrs(ref)));
  }
  for (const ref of scanFileRefs(text, (index) => inCode[index] === true)) {
    // A link's own text is either already a chip or a link to the web.
    if (links.some((link) => ref.start < link.to && ref.end > link.from)) continue;
    decorations.push(Decoration.inline(contentStart + ref.start, contentStart + ref.end, chipAttrs(ref)));
  }
  return decorations;
}

/**
 * Draws file references in a read-only answer as chips.
 *
 * Decorations rather than nodes, like `citationMarks`: the document keeps the
 * literal text, so copy, export and the markdown round-trip are untouched. The
 * chip carries `data-code-ref` and, when it names lines, those lines; whoever
 * hosts the viewer handles the click.
 */
export const CodeRefMarks = Extension.create<CodeRefMarksOptions>({
  name: 'codeRefMarks',

  addOptions() {
    return { isEnabled: () => false };
  },

  addProseMirrorPlugins() {
    const { isEnabled } = this.options;
    return [
      new Plugin({
        key: codeRefMarksKey,
        props: {
          decorations(state) {
            if (!isEnabled()) return DecorationSet.empty;
            const decorations: Decoration[] = [];
            state.doc.descendants((node, pos) => {
              if (!node.isTextblock) return true;
              // Code blocks are code, not references.
              if (!node.type.spec.code) decorations.push(...blockDecorations(node, pos + 1));
              return false;
            });
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
