import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

import type { FoundMark } from '@/utils/foundMark';
import { QUOTE_END, QUOTE_START } from '@/utils/replyPieces';

export const citationMarksKey = new PluginKey('citationMarks');

const QUOTE_MARKS = new RegExp(`[${QUOTE_START}${QUOTE_END}]`, 'g');

/** The quotation mark a private-use mark stands for, drawn in the text's own colour. */
function quoteWidget(opening: boolean): () => HTMLElement {
  return () => {
    const glyph = document.createElement('span');
    glyph.className = 'cite-quote';
    glyph.textContent = opening ? '“' : '”';
    return glyph;
  };
}

export interface CitationMarksOptions {
  /** Decides, at draw time, whether source number `n` is one this answer has. */
  isCitation: (number: number) => boolean;
  /** Finds the citation marks in a run of document text; the host chooses where marks come from. */
  findMarks: (text: string) => FoundMark[];
  /** Turns copied document text into the text that reaches the clipboard, when marks need writing out. */
  copyText?: (text: string) => string;
  /** Hides valid markers without changing the document or copied markdown. */
  isVisible: () => boolean;
  /** Called when an editable or read-only citation chip is activated. */
  onCitationClick?: (number: number, occurrence: number) => void;
  /** Called when the pointer enters a citation chip. */
  onCitationHover?: (number: number, occurrence: number, rect: DOMRect) => void;
  /** Called when the pointer leaves a citation chip. */
  onCitationLeave?: () => void;
}

/**
 * Draws citation marks as chips. Where the marks come from is the host's
 * `findMarks`: a reply's references (`findReplyMarks`) or a note's `[n]`
 * markers (`findNoteMarks`). Decorations rather than nodes, so the markdown
 * round-trip is untouched. The
 * chip carries `data-cite="n"` and `data-cite-at="k"` — this is the k-th mark
 * of source n, counted from zero in reading order — because an answer cites
 * one source from several sentences and each mark means its own sentence.
 * Whoever hosts the viewer handles clicks and hover. `sentencesByOccurrence`
 * counts the same way, from the same list of references.
 *
 * A quotation's pair of private-use marks is drawn as opening and closing
 * quotation marks (`cite-quote`), whoever the host is and whether or not
 * citations are shown.
 */
export const CitationMarks = Extension.create<CitationMarksOptions>({
  name: 'citationMarks',

  addOptions() {
    return {
      isCitation: () => false,
      findMarks: () => [],
      isVisible: () => true,
      onCitationClick: undefined,
      onCitationHover: undefined,
      onCitationLeave: undefined,
    };
  },

  addProseMirrorPlugins() {
    const { isCitation, findMarks, copyText, isVisible, onCitationClick, onCitationHover, onCitationLeave } = this.options;
    const chipOf = (target: EventTarget | null) => {
      const el = target as Element | null;
      return el?.closest?.<HTMLElement>('[data-cite][data-cite-at]')
        ?? el?.parentElement?.closest<HTMLElement>('[data-cite][data-cite-at]');
    };
    return [
      new Plugin({
        key: citationMarksKey,
        props: {
          clipboardTextSerializer(slice) {
            const text = slice.content.textBetween(0, slice.content.size, '\n\n');
            return copyText ? copyText(text) : text;
          },
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
              const chip = chipOf(event.target);
              if (!chip) return false;
              const number = Number(chip.dataset.cite);
              const occurrence = Number(chip.dataset.citeAt);
              if (!Number.isInteger(number) || !Number.isInteger(occurrence)) return false;
              event.preventDefault();
              onCitationClick(number, occurrence);
              return true;
            },
            mouseover(_view, event) {
              if (!isVisible() || !onCitationHover) return false;
              const chip = chipOf(event.target);
              if (!chip) return false;
              const number = Number(chip.dataset.cite);
              const occurrence = Number(chip.dataset.citeAt);
              if (!Number.isInteger(number) || !Number.isInteger(occurrence)) return false;
              onCitationHover(number, occurrence, chip.getBoundingClientRect());
              return false;
            },
            mouseout(_view, event) {
              if (!onCitationLeave) return false;
              const chip = chipOf(event.target);
              if (!chip) return false;
              if (chipOf(event.relatedTarget) === chip) return false;
              onCitationLeave();
              return false;
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
              for (const quote of node.text.matchAll(QUOTE_MARKS)) {
                const at = pos + (quote.index ?? 0);
                decorations.push(
                  Decoration.inline(at, at + 1, { class: 'citation-hidden', 'aria-hidden': 'true' }),
                  Decoration.widget(at, quoteWidget(quote[0] === QUOTE_START), { side: -1, key: `quote-${at}` }),
                );
              }
              for (const found of findMarks(node.text)) {
                const { number } = found;
                const from = pos + found.index;
                if (!isCitation(number)) {
                  if (!found.glyph) continue;
                  decorations.push(
                    Decoration.inline(from, from + found.length, { class: 'citation-hidden', 'aria-hidden': 'true' }),
                    Decoration.widget(
                      from,
                      () => {
                        const written = document.createElement('span');
                        written.textContent = `[${number}]`;
                        return written;
                      },
                      { side: -1, key: `unlinked-${number}` },
                    ),
                  );
                  continue;
                }
                const at = seen.get(number) ?? 0;
                seen.set(number, at + 1);
                decorations.push(
                  Decoration.inline(
                    from,
                    from + found.length,
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
