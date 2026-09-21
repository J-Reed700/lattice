import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { sentencesByOccurrence } from '../../Chat/reader/answerSentences';
import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';

import type { DecorationSet } from '@tiptap/pm/view';

interface DrawnChip {
  number: number;
  at: number;
  /** The block the chip was drawn in, letters and digits only. */
  block: string;
}

const lettersOnly = (text: string): string => text.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

/** Every chip a rendered answer draws, in reading order. */
function drawnChips(markdown: string): DrawnChip[] {
  const editor = new Editor({
    extensions: [
      ...createExtensions(),
      CitationMarks.configure({ isCitation: () => true }),
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable: false,
  });
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === citationMarksKey);
  const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
  const chips = set
    .find()
    .sort((a, b) => a.from - b.from)
    .map((decoration) => {
      const attrs = (decoration as unknown as { type: { attrs: Record<string, string> } }).type.attrs;
      return {
        number: Number(attrs['data-cite']),
        at: Number(attrs['data-cite-at']),
        block: lettersOnly(editor.state.doc.resolve(decoration.from).parent.textContent),
      };
    });
  editor.destroy();
  return chips;
}

const ANSWER = [
  'Potatoes give the most calories per square foot indoors [6].',
  '',
  '- Microgreens are ready in ten days [2][6]. They need no deep soil.',
  '',
  '```',
  'yield[6] = 4',
  '```',
  '',
  'Skip `grid[6]` lookups; dwarf tomatoes want a **five-gallon** pot [6].',
].join('\n');

describe('CitationMarks', () => {
  it('numbers the marks of one source in reading order, leaving code alone', () => {
    const sixes = drawnChips(ANSWER).filter((chip) => chip.number === 6);

    expect(sixes.map((chip) => chip.at)).toEqual([0, 1, 2]);
  });

  it('counts each source on its own', () => {
    const chips = drawnChips('Kale bolts in heat [1]. Chard does not [2]. Both want nitrogen [1].');

    expect(chips.map((chip) => `${chip.number}@${chip.at}`)).toEqual(['1@0', '2@0', '1@1']);
  });

  // The reader is told "mark k of source n" and looks the sentence up in the raw
  // answer. If the two ever count differently, a click opens the wrong passage.
  it('agrees with the reader about which sentence each mark sits in', () => {
    for (const number of [2, 6]) {
      const sentences = sentencesByOccurrence(ANSWER, number);
      const chips = drawnChips(ANSWER).filter((chip) => chip.number === number);

      expect(chips).toHaveLength(sentences.length);
      for (const chip of chips) {
        const sentence = sentences[chip.at]!;
        // Inline code is dropped from the sentence but kept in the block.
        const words = sentence.split(/\s*;\s*/).pop()!;
        expect(chip.block).toContain(lettersOnly(words));
      }
    }
  });
});
