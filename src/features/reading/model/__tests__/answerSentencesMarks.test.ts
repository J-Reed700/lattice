import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '@/components/TiptapEditor/extensions';
import { CitationMarks, citationMarksKey } from '@/components/TiptapEditor/extensions/citationMarks';
import { reply } from '@/tests/replyFixtures';
import { drawReply, findReplyMarks } from '@/utils/replyPieces';

import { sentencesByOccurrence } from '../answerSentences';

import type { DecorationSet } from '@tiptap/pm/view';

interface DrawnChip {
  number: number;
  at: number;
  /** The block the chip was drawn in, letters and digits only. */
  block: string;
}

const lettersOnly = (text: string): string => text.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

const ANSWER = reply(
  'Potatoes give the most calories per square foot indoors ', 6, '.\n\n',
  '- Microgreens are ready in ten days ', 2, 6, '. They need no deep soil.\n\n',
  '```\nyield[6] = 4\n```\n\n',
  'Skip `grid[6]` lookups; dwarf tomatoes want a **five-gallon** pot ', 6, '.',
);

/** Every chip a rendered answer draws, in reading order. */
function drawnChips(): DrawnChip[] {
  const drawn = drawReply(ANSWER);
  const editor = new Editor({
    extensions: [
      ...createExtensions(),
      CitationMarks.configure({ isCitation: () => true, findMarks: (text) => findReplyMarks(text, drawn.numbers) }),
      Markdown.configure({ html: false }),
    ],
    content: drawn.markdown,
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

describe('answer sentences and the drawn citation marks', () => {
  // The reader is told "mark k of source n" and looks the sentence up in the
  // answer's pieces. If the two ever count differently, a click opens the wrong passage.
  it('agrees with the reader about which sentence each mark sits in', () => {
    for (const number of [2, 6]) {
      const sentences = sentencesByOccurrence(ANSWER, number);
      const chips = drawnChips().filter((chip) => chip.number === number);

      expect(chips).toHaveLength(sentences.length);
      for (const chip of chips) {
        const sentence = sentences[chip.at]!;
        const words = sentence.split(/\s*;\s*/).pop()!;
        expect(chip.block).toContain(lettersOnly(words));
      }
    }
  });
});
