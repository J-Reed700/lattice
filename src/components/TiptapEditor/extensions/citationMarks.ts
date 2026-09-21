import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

const CITATION_PATTERN = /\[(\d{1,5})\]/g;

export const citationMarksKey = new PluginKey('citationMarks');

export interface CitationMarksOptions {
  /** Decides, at draw time, whether `[n]` refers to a real source. */
  isCitation: (number: number) => boolean;
}

/**
 * Draws `[n]` markers in a read-only answer as citation chips.
 *
 * Decorations rather than nodes: the document keeps the literal `[n]`, so copy,
 * export and the markdown round-trip are untouched. The chip carries
 * `data-cite="n"` and `data-cite-at="k"` — this is the k-th mark of source n,
 * counted from zero in reading order — because an answer cites one source from
 * several sentences and each mark means its own sentence. Whoever hosts the
 * viewer handles clicks and hover. `sentencesByOccurrence` counts the same way.
 */
export const CitationMarks = Extension.create<CitationMarksOptions>({
  name: 'citationMarks',

  addOptions() {
    return { isCitation: () => false };
  },

  addProseMirrorPlugins() {
    const { isCitation } = this.options;
    return [
      new Plugin({
        key: citationMarksKey,
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];
            const seen = new Map<number, number>();
            state.doc.descendants((node, pos, parent) => {
              if (!node.isText || !node.text) return;
              if (parent?.type.spec.code) return;
              if (node.marks.some((mark) => mark.type.spec.code)) return;
              for (const match of node.text.matchAll(CITATION_PATTERN)) {
                const number = Number(match[1]);
                if (!isCitation(number)) continue;
                const from = pos + (match.index ?? 0);
                const at = seen.get(number) ?? 0;
                seen.set(number, at + 1);
                decorations.push(
                  Decoration.inline(from, from + match[0].length, {
                    class: 'cite-chip',
                    'data-cite': String(number),
                    'data-cite-at': String(at),
                    role: 'button',
                    'aria-label': `Citation ${number}`,
                  }),
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
