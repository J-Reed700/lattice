import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { ClaimMarks, claimMarksKey, type ClaimMark } from '../extensions/claimMarks';

import type { DecorationSet } from '@tiptap/pm/view';

/** The text each claim was drawn over, keyed by its index, in a rendered answer. */
function markedText(markdown: string, claims: ClaimMark[]): Map<string, string> {
  const editor = new Editor({
    extensions: [
      ...createExtensions(),
      ClaimMarks.configure({ getClaims: () => claims }),
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable: false,
  });
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === claimMarksKey);
  const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
  const marked = new Map<string, string>();
  for (const decoration of set.find()) {
    const attrs = (decoration as unknown as { type: { attrs: Record<string, string> } }).type.attrs;
    marked.set(
      `${attrs['data-claim']}:${attrs.class}`,
      editor.state.doc.textBetween(decoration.from, decoration.to),
    );
  }
  editor.destroy();
  return marked;
}

describe('ClaimMarks', () => {
  it('finds a sentence the verifier cut from raw markdown in the rendered answer', () => {
    const marked = markedText(
      '- The relationship is **non-linear**. Below ~30% cover the benefit is about half [2].',
      [{ sentence: '- The relationship is **non-linear**.', verdict: 'supported' }],
    );

    expect(marked.get('0:claim claim-supported')).toBe('The relationship is non-linear');
  });

  it('marks doubt on the words it is about, citation markers and all', () => {
    const marked = markedText(
      'Thermal imagery overstates what residents feel by a factor of three [4], so quote air temperature.',
      [{ sentence: 'Thermal imagery overstates what residents feel by a factor of three [4], so quote air temperature.', verdict: 'unsupported' }],
    );

    expect(marked.get('0:claim claim-unsupported')).toBe(
      'Thermal imagery overstates what residents feel by a factor of three [4], so quote air temperature',
    );
  });

  it('gives a sentence said twice to each verdict in reading order', () => {
    const marked = markedText(
      'Canopy cover lowers air temperature [1].\n\nCanopy cover lowers air temperature [2].',
      [
        { sentence: 'Canopy cover lowers air temperature [1].', verdict: 'supported' },
        { sentence: 'Canopy cover lowers air temperature [2].', verdict: 'contradicted' },
      ],
    );

    expect([...marked.keys()]).toEqual(['0:claim claim-supported', '1:claim claim-contradicted']);
  });

  it('leaves unmarked what it cannot find or cannot tell apart', () => {
    const marked = markedText('Canopy cover lowers air temperature across the pooled studies.', [
      { sentence: 'A sentence the answer never contained at all.', verdict: 'unsupported' },
      // Too short to be sure it is this sentence and not the same words elsewhere.
      { sentence: 'Canopy cover.', verdict: 'unsupported' },
    ]);

    expect(marked.size).toBe(0);
  });
});
