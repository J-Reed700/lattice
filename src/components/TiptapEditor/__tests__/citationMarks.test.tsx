import { fireEvent, render, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Editor } from '@tiptap/core';
import { MemoryRouter } from 'react-router';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it, vi } from 'vitest';

import { sentencesByOccurrence } from '@/features/chat/components/reader/answerSentences';

import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';
import { TiptapEditor } from '../TiptapEditor';
import { TiptapViewer } from '../TiptapViewer';

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
  it('tabs through read-only markers and delegates Enter/Space to the same occurrence as a click', async () => {
    const user = userEvent.setup();
    const activate = vi.fn();
    const view = render(<div onClick={(event) => {
      const chip = (event.target as Element).closest<HTMLElement>('[data-cite]');
      if (chip) activate(Number(chip.dataset.cite), Number(chip.dataset.citeAt));
    }}>
      <button type="button">Before citations</button>
      <TiptapViewer content="First claim [4]. Second claim [4]." citationNumbers={[4]} />
    </div>);
    const chips = await waitFor(() => {
      const found = view.container.querySelectorAll<HTMLElement>('[data-cite="4"]');
      expect(found).toHaveLength(2);
      return found;
    });
    await user.click(view.getByRole('button', { name: 'Before citations' }));
    await user.tab();
    expect(chips[0]).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(activate).toHaveBeenLastCalledWith(4, 0);
    await user.tab();
    expect(chips[1]).toHaveFocus();
    await user.keyboard(' ');
    expect(activate).toHaveBeenLastCalledWith(4, 1);
    expect(activate).toHaveBeenCalledTimes(2);
    expect(view.container).toHaveTextContent('First claim [4]. Second claim [4].');
  });

  it('removes hidden citations from keyboard navigation', async () => {
    const user = userEvent.setup();
    const view = render(<>
      <button type="button">Before citations</button>
      <TiptapViewer content="A claim [4]." citationNumbers={[4]} showEvidence={false} />
      <button type="button">After citations</button>
    </>);
    await waitFor(() => expect(view.container.querySelector('.citation-hidden')).not.toBeNull());
    expect(view.container.querySelector('[data-cite]')).toBeNull();
    expect(view.queryByRole('button', { name: 'Citation 4' })).toBeNull();
    await user.click(view.getByRole('button', { name: 'Before citations' }));
    await user.tab();
    expect(view.getByRole('button', { name: 'After citations' })).toHaveFocus();
  });
  it('toggles real viewer citations and highlights without changing prose, code or unknown markers', async () => {
    const content = 'A careful comparison records the measure [4]. Leave [99] and `grid[4]` alone.';
    const props = { content, citationNumbers: [4], claims: [{ sentence: 'A careful comparison records the measure [4].', verdict: 'supported' as const }] };
    const view = render(<TiptapViewer {...props} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite="4"]')).not.toBeNull());
    expect(view.container.querySelector('.claim-supported')).not.toBeNull();

    view.rerender(<TiptapViewer {...props} showEvidence={false} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite]')).toBeNull());
    expect(view.container.querySelector('.claim')).toBeNull();
    expect(view.container.querySelectorAll('.citation-hidden')).toHaveLength(1);
    expect(view.container.querySelector('.citation-hidden')).toHaveTextContent('[4]');
    expect(view.container).toHaveTextContent('Leave [99]');
    expect(view.container.querySelector('code')).toHaveTextContent('grid[4]');

    view.rerender(<TiptapViewer {...props} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite="4"]')).not.toBeNull());
    expect(view.container.querySelector('.claim-supported')).not.toBeNull();
    expect(view.container.querySelector('.citation-hidden')).toBeNull();
    expect(view.container).toHaveTextContent('A careful comparison records the measure [4].');
  });

  it('can hide a valid marker without removing it from the document', () => {
    const editor = new Editor({
      extensions: [
        ...createExtensions(),
        CitationMarks.configure({ isCitation: () => true, isVisible: () => false }),
        Markdown.configure({ html: false }),
      ],
      content: 'A sourced sentence [3].',
      editable: false,
    });
    const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === citationMarksKey);
    const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
    const attrs = (set.find()[0] as unknown as { type: { attrs: Record<string, string> } }).type.attrs;

    expect(attrs.class).toBe('citation-hidden');
    expect(attrs['data-cite']).toBeUndefined();
    expect(editor.state.doc.textContent).toContain('[3]');
    editor.destroy();
  });

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

  it('opens the mapped citation with the occurrence of the clicked inline mark', async () => {
    const onCitationClick = vi.fn();
    const { container } = render(
      <MemoryRouter>
        <TiptapEditor
          value="First claim [4]. Second claim [4]."
          onChange={vi.fn()}
          citationNumbers={[4]}
          onCitationClick={onCitationClick}
        />
      </MemoryRouter>,
    );
    const chips = await waitFor(() => {
      const found = container.querySelectorAll('[data-cite="4"]');
      expect(found).toHaveLength(2);
      return found;
    });

    fireEvent.click(chips[1]!);
    expect(onCitationClick).toHaveBeenCalledWith(4, 1);
    fireEvent.keyDown(chips[0]!, { key: 'Enter' });
    expect(onCitationClick).toHaveBeenLastCalledWith(4, 0);
    fireEvent.keyDown(chips[1]!, { key: ' ' });
    expect(onCitationClick).toHaveBeenLastCalledWith(4, 1);
    fireEvent.keyDown(chips[1]!, { key: ' ', repeat: true });
    fireEvent.keyDown(chips[1]!, { key: 'Enter', ctrlKey: true });
    expect(onCitationClick).toHaveBeenCalledTimes(3);
    expect(container).toHaveTextContent('First claim [4]. Second claim [4].');
  });
});
