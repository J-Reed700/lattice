import { render, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it, vi } from 'vitest';

import { reply, said } from '@/tests/replyFixtures';
import { drawReply, findReplyMarks, markFor, writeMarksAsNumbers, type ReplyPiece } from '@/utils/replyPieces';

import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';
import { selectionText } from '../selectionText';
import { TiptapViewer, type TiptapViewerProps } from '../TiptapViewer';

import type { DecorationSet } from '@tiptap/pm/view';

interface DrawnChip {
  number: number;
  at: number;
  /** The block the chip was drawn in, letters and digits only. */
  block: string;
}

const lettersOnly = (text: string): string => text.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');

function Answer({ parts, ...props }: { parts: ReplyPiece[] } & Omit<TiptapViewerProps, 'content' | 'marks'>) {
  const drawn = drawReply(parts);
  return <TiptapViewer content={drawn.markdown} marks={drawn.numbers} {...props} />;
}

function answerEditor(parts: ReplyPiece[], options: { isVisible?: () => boolean } = {}): Editor {
  const drawn = drawReply(parts);
  return new Editor({
    extensions: [
      ...createExtensions(),
      CitationMarks.configure({
        isCitation: () => true,
        findMarks: (text) => findReplyMarks(text, drawn.numbers),
        copyText: (text) => writeMarksAsNumbers(text, drawn.numbers),
        ...options,
      }),
      Markdown.configure({ html: false }),
    ],
    content: drawn.markdown,
    editable: false,
  });
}

function citationDecorations(editor: Editor): DecorationSet {
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === citationMarksKey);
  return plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
}

/** Every chip a rendered answer draws, in reading order. */
function drawnChips(parts: ReplyPiece[]): DrawnChip[] {
  const editor = answerEditor(parts);
  const chips = citationDecorations(editor)
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

const ANSWER = reply(
  'Potatoes give the most calories per square foot indoors ', 6, '.\n\n',
  '- Microgreens are ready in ten days ', 2, 6, '. They need no deep soil.\n\n',
  '```\nyield[6] = 4\n```\n\n',
  'Skip `grid[6]` lookups; dwarf tomatoes want a **five-gallon** pot ', 6, '.',
);

describe('CitationMarks', () => {
  it('tabs through read-only markers and delegates Enter/Space to the same occurrence as a click', async () => {
    const user = userEvent.setup();
    const activate = vi.fn();
    const view = render(<div onClick={(event) => {
      const chip = (event.target as Element).closest<HTMLElement>('[data-cite]');
      if (chip) activate(Number(chip.dataset.cite), Number(chip.dataset.citeAt));
    }}>
      <button type="button">Before citations</button>
      <Answer parts={reply('First claim ', 4, '. Second claim ', 4, '.')} citationNumbers={[4]} />
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
    expect(view.container).toHaveTextContent('First claim');
    expect(view.container).not.toHaveTextContent('[s4]');
  });

  it('removes hidden citations from keyboard navigation', async () => {
    const user = userEvent.setup();
    const view = render(<>
      <button type="button">Before citations</button>
      <Answer parts={reply('A claim ', 4, '.')} citationNumbers={[4]} showEvidence={false} />
      <button type="button">After citations</button>
    </>);
    await waitFor(() => expect(view.container.querySelector('.citation-hidden')).not.toBeNull());
    expect(view.container.querySelector('[data-cite]')).toBeNull();
    expect(view.queryByRole('button', { name: 'Citation 4' })).toBeNull();
    await user.click(view.getByRole('button', { name: 'Before citations' }));
    await user.tab();
    expect(view.getByRole('button', { name: 'After citations' })).toHaveFocus();
  });

  it('toggles real viewer citations and highlights without changing prose, code or unknown references', async () => {
    const props = {
      parts: reply('A careful comparison records the measure ', 4, '. Leave ', 99, ' and `grid[4]` alone.'),
      citationNumbers: [4],
      claims: [{ sentence: 'A careful comparison records the measure.', verdict: 'supported' as const }],
    };
    const view = render(<Answer {...props} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite="4"]')).not.toBeNull());
    expect(view.container.querySelector('.claim-supported')).not.toBeNull();
    expect(view.container).toHaveTextContent('Leave [99]');

    view.rerender(<Answer {...props} showEvidence={false} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite]')).toBeNull());
    expect(view.container.querySelector('.claim')).toBeNull();
    expect(view.container.querySelectorAll('.citation-hidden')).toHaveLength(2);
    expect(view.container).toHaveTextContent('Leave [99]');
    expect(view.container.querySelector('code')).toHaveTextContent('grid[4]');

    view.rerender(<Answer {...props} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite="4"]')).not.toBeNull());
    expect(view.container.querySelector('.claim-supported')).not.toBeNull();
    expect(view.container.querySelectorAll('.citation-hidden')).toHaveLength(1);
    expect(view.container).toHaveTextContent('Leave [99]');
  });

  it('can hide a valid marker without removing it from the document', () => {
    const editor = answerEditor(reply('A sourced sentence ', 3, '.'), { isVisible: () => false });
    const attrs = (citationDecorations(editor).find()[0] as unknown as { type: { attrs: Record<string, string> } })
      .type.attrs;

    expect(attrs.class).toBe('citation-hidden');
    expect(attrs['data-cite']).toBeUndefined();
    expect(editor.state.doc.textContent).toContain(markFor(0));
    editor.destroy();
  });

  it('numbers the marks of one source in reading order, leaving code alone', () => {
    const sixes = drawnChips(ANSWER).filter((chip) => chip.number === 6);

    expect(sixes.map((chip) => chip.at)).toEqual([0, 1, 2]);
  });

  it('counts each source on its own', () => {
    const chips = drawnChips(reply('Kale bolts in heat ', 1, '. Chard does not ', 2, '. Both want nitrogen ', 1, '.'));

    expect(chips.map((chip) => `${chip.number}@${chip.at}`)).toEqual(['1@0', '2@0', '1@1']);
  });

  it('draws a reference as a chip at its place in the text', async () => {
    const view = render(<Answer parts={reply('Kale bolts ', 3, ' in heat.')} citationNumbers={[3]} />);
    const chip = await waitFor(() => {
      const found = view.container.querySelector<HTMLElement>('[data-cite="3"]');
      expect(found).not.toBeNull();
      return found!;
    });

    expect(chip.previousSibling).toHaveTextContent('Kale bolts');
    expect(chip.nextSibling).toHaveTextContent('in heat.');
  });

  it('leaves text that looks like a marker as plain text', async () => {
    const view = render(<TiptapViewer content="A note says [3] here." citationNumbers={[3]} marks={[]} />);
    await waitFor(() => expect(view.container).toHaveTextContent('A note says [3] here.'));

    expect(view.container.querySelector('[data-cite]')).toBeNull();
    expect(view.container.querySelector('.citation-hidden')).toBeNull();
  });

  it('copies a chip as its number', () => {
    const editor = answerEditor(reply('Kale bolts ', 3, ' in heat.'));
    const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === citationMarksKey);
    const slice = editor.state.doc.slice(0, editor.state.doc.content.size);

    expect(plugin?.props.clipboardTextSerializer?.call(plugin, slice, editor.view)).toBe('Kale bolts [3] in heat.');
    editor.destroy();
  });

  it('draws a quotation between two quote widgets and hides the private-use marks', async () => {
    const parts: ReplyPiece[] = [said('The guide says '), { type: 'quoted', text: 'Tea is hot.' }, said(' today.')];
    const view = render(<Answer parts={parts} citationNumbers={[]} />);
    await waitFor(() => expect(view.container.querySelectorAll('.cite-quote')).toHaveLength(2));
    const [open, close] = Array.from(view.container.querySelectorAll('.cite-quote'));

    expect(open).toHaveTextContent('\u201c');
    expect(close).toHaveTextContent('\u201d');
    expect(open!.compareDocumentPosition(close!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(view.container.querySelectorAll('.citation-hidden')).toHaveLength(2);
    expect(view.container.querySelector('[data-cite]')).toBeNull();
  });

  it('keeps the private-use characters out of a selection over a quotation', async () => {
    const parts: ReplyPiece[] = [said('The guide says '), { type: 'quoted', text: 'Tea is hot.' }, said(' today.')];
    const view = render(<Answer parts={parts} citationNumbers={[]} />);
    await waitFor(() => expect(view.container.querySelectorAll('.cite-quote')).toHaveLength(2));
    const range = document.createRange();
    range.selectNodeContents(view.container);
    const selection = { getRangeAt: () => range, toString: () => range.toString() } as unknown as Selection;

    expect(range.toString()).toMatch(/[\uE00E\uE00F]/);
    expect(selectionText(selection)).not.toMatch(/[\uE000-\uE00F]/);
    expect(selectionText(selection)).toContain('Tea is hot.');
  });

  it('copies a quotation in curly quotes', () => {
    const editor = answerEditor([said('He wrote '), { type: 'quoted', text: 'Tea is hot.' }, said('.')]);
    const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === citationMarksKey);
    const slice = editor.state.doc.slice(0, editor.state.doc.content.size);

    expect(plugin?.props.clipboardTextSerializer?.call(plugin, slice, editor.view)).toBe('He wrote \u201cTea is hot.\u201d.');
    editor.destroy();
  });
});
