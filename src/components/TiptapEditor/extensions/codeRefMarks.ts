import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

import { FILE_MARK, fileLabel, markIndex, type FileTarget } from '@/utils/replyPieces';

import { fileRefFromHref, type LineRange } from './fileRefs';

export const codeRefMarksKey = new PluginKey('codeRefMarks');

export interface CodeRefMarksOptions {
  /** Read at draw time, so a host can switch the chips on after mounting. */
  isEnabled: () => boolean;
  /** The file of each file mark in the document (see `drawReply`), by the index the mark carries. */
  getFiles: () => readonly FileTarget[];
}

function chipAttrs(path: string, lines: LineRange | null): Record<string, string> {
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
 * Draws file references in a read-only answer as chips: the file marks a reply
 * carries (`drawReply`), and a link whose address points into the folder.
 *
 * Decorations rather than nodes, like `citationMarks`: the document keeps the
 * literal text, so copy, export and the markdown round-trip are untouched. The
 * chip carries `data-code-ref` and, when it names lines, those lines; whoever
 * hosts the viewer handles the click.
 */
export const CodeRefMarks = Extension.create<CodeRefMarksOptions>({
  name: 'codeRefMarks',

  addOptions() {
    return { isEnabled: () => false, getFiles: () => [] };
  },

  addProseMirrorPlugins() {
    const { isEnabled, getFiles } = this.options;
    return [
      new Plugin({
        key: codeRefMarksKey,
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];
            const files = getFiles();
            const linked = isEnabled();
            state.doc.descendants((node, pos, parent) => {
              if (!node.isText || !node.text || parent?.type.spec.code) return;
              if (linked) {
                const href = node.marks.find((mark) => mark.type.name === 'link')?.attrs.href;
                const ref = typeof href === 'string' ? fileRefFromHref(href) : null;
                if (ref) decorations.push(Decoration.inline(pos, pos + node.nodeSize, chipAttrs(ref.path, ref.lines)));
              }
              for (const found of node.text.matchAll(FILE_MARK)) {
                const file = files[markIndex(found[1])];
                if (!file) continue;
                const markFrom = pos + (found.index ?? 0);
                const labelFrom = markFrom + found[0].length;
                decorations.push(
                  Decoration.inline(markFrom, labelFrom, { class: 'citation-hidden', 'aria-hidden': 'true' }),
                  Decoration.inline(
                    labelFrom,
                    labelFrom + fileLabel(file).length,
                    chipAttrs(file.name, file.lines && { startLine: file.lines.first, endLine: file.lines.last }),
                  ),
                );
              }
            });
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
