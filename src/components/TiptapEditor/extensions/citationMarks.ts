import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

const CITATION_PATTERN = /\[(\d{1,5})\]/g;

export const citationMarksKey = new PluginKey('citationMarks');

export interface CitationMarksOptions {
  /** Decides, at draw time, whether `[n]` refers to a real source. */
  isCitation: (number: number) => boolean;
  /** Hides valid markers without changing the document or copied markdown. */
  isVisible: () => boolean;
  /** Called when an editable or read-only citation chip is activated. */
  onCitationClick?: (number: number, occurrence: number) => void;
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
    return { isCitation: () => false, isVisible: () => true, onCitationClick: undefined };
  },

  addProseMirrorPlugins() {
    const { isCitation, isVisible, onCitationClick } = this.options;
    return [
      new Plugin({
        key: citationMarksKey,
        props: {
          handleDOMEvents: {
            keydown(_view, event) {
              if (!isVisible() || event.isComposing || event.altKey || event.ctrlKey || event.metaKey
                || (event.key !== 'Enter' && event.key !== ' ')) return false;
              const chip = (event.target as Element | null)?.closest?.<HTMLElement>('[data-cite][data-cite-at]');
              if (!chip) return false;
              // Reuse the click path, including hosts which delegate clicks
              // from a read-only viewer. Don't insert text or scroll on Space.
              event.preventDefault();
              if (!event.repeat) chip.click();
              return true;
            },
            click(_view, event) {
              if (!isVisible() || !onCitationClick) return false;
              const target = event.target as Element | null;
              const chip = target?.closest?.<HTMLElement>('[data-cite][data-cite-at]')
                ?? target?.parentElement?.closest<HTMLElement>('[data-cite][data-cite-at]');
              if (!chip) return false;
              const number = Number(chip.dataset.cite);
              const occurrence = Number(chip.dataset.citeAt);
              if (!Number.isInteger(number) || !Number.isInteger(occurrence)) return false;
              event.preventDefault();
              onCitationClick(number, occurrence);
              return true;
            },
          },
          decorations(state) {
            const decorations: Decoration[] = [];
            const seen = new Map<number, number>();
            const visible = isVisible();
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
                  Decoration.inline(
                    from,
                    from + match[0].length,
                    visible
                      ? {
                          class: 'cite-chip',
                          'data-cite': String(number),
                          'data-cite-at': String(at),
                          role: 'button',
                          tabindex: '0',
                          'aria-label': `Citation ${number}`,
                        }
                      : {
                          class: 'citation-hidden',
                          'aria-hidden': 'true',
                        },
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
