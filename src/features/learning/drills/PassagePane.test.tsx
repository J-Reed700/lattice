import { useState } from 'react';

import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import type { CodeReviewDrillDto } from '@/lib/bindings';

import { PassagePane } from './PassagePane';
import { pickSentence, sentencesOf } from './passageText';

const TEXT = 'Spacing helps recall.\nIt helped by 40% a week later.\nThe benefit was larger for known material.\nTests make spacing work best.\n';
const DRILL: CodeReviewDrillDto = {
  title: 'Why spacing helps',
  material: { kind: 'passage', sourceTitle: 'Spacing and memory' },
  files: [{ path: 'explanation.md', text: TEXT }],
};

/** The pane with its selection held, as the review holds it. */
function Held({ onSelect, initial = null, writingFix = false, fixEdits = {}, onFixEdit = () => {} }: {
  onSelect?: (range: ExplorerLineRange | null) => void;
  initial?: ExplorerLineRange | null;
  writingFix?: boolean;
  fixEdits?: Record<string, string>;
  onFixEdit?: (path: string, text: string) => void;
}) {
  const [selection, setSelection] = useState<ExplorerLineRange | null>(initial);
  return (
    <PassagePane
      drill={DRILL}
      writingFix={writingFix}
      selection={selection}
      onSelectLines={(range) => {
        setSelection(range);
        onSelect?.(range);
      }}
      highlight={null}
      highlightNonce={null}
      fixEdits={fixEdits}
      onFixEdit={onFixEdit}
    />
  );
}

const sentence = (line: number) => screen.getByRole('button', { name: new RegExp(`^Sentence ${line}:`) });

describe('the lines of an explanation', () => {
  it('are its sentences, numbered from 1 by the line each is on', () => {
    expect(sentencesOf(TEXT)).toEqual([
      { line: 1, text: 'Spacing helps recall.' },
      { line: 2, text: 'It helped by 40% a week later.' },
      { line: 3, text: 'The benefit was larger for known material.' },
      { line: 4, text: 'Tests make spacing work best.' },
    ]);
    expect(sentencesOf('')).toEqual([]);
    expect(sentencesOf('One.\r\nTwo.\r\n').map((sentence) => sentence.text)).toEqual(['One.', 'Two.']);
  });

  it('are picked alone, as a run from the first one picked, or cleared by picking the only one again', () => {
    expect(pickSentence(null, 4, false)).toEqual({ startLine: 4, endLine: 4 });
    expect(pickSentence({ startLine: 4, endLine: 4 }, 5, true)).toEqual({ startLine: 4, endLine: 5 });
    expect(pickSentence({ startLine: 4, endLine: 4 }, 2, true)).toEqual({ startLine: 2, endLine: 4 });
    expect(pickSentence({ startLine: 4, endLine: 5 }, 4, false)).toEqual({ startLine: 4, endLine: 4 });
    expect(pickSentence({ startLine: 4, endLine: 4 }, 4, false)).toBeNull();
    expect(pickSentence(null, 4, true)).toEqual({ startLine: 4, endLine: 4 });
  });
});

describe('PassagePane', () => {
  it('reads as one flowing paragraph, and each sentence is a button that names its number, from 1 to n', () => {
    const { container } = render(<Held />);
    const paragraphs = container.querySelectorAll('p');
    expect(paragraphs).toHaveLength(1);
    expect(within(paragraphs[0] as HTMLElement).getAllByRole('button')).toHaveLength(4);
    expect(screen.getAllByRole('button').map((button) => button.getAttribute('data-line'))).toEqual(['1', '2', '3', '4']);
    expect(sentence(2)).toHaveTextContent('It helped by 40% a week later.');
    expect(screen.getByRole('button', { name: /^Sentence 3: .*The benefit was larger for known material\.$/ })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'The explanation, sentence by sentence' })).toBeInTheDocument();
  });

  it('runs the sentences on inside the paragraph, and the number of one takes no room', () => {
    const { container } = render(<Held initial={{ startLine: 3, endLine: 3 }} />);
    for (const button of screen.getAllByRole('button')) {
      expect(button.classList.contains('inline')).toBe(true);
      expect(button.className).toContain('box-decoration-break:clone');
    }
    for (const number of container.querySelectorAll('sup')) {
      expect(number.classList.contains('absolute')).toBe(true);
    }
  });

  it('makes each sentence an inline span with the role of a button, which a browser lays out as text', () => {
    render(<Held />);
    for (const line of [1, 2, 3, 4]) expect(sentence(line).tagName).toBe('SPAN');
  });

  it('picks a sentence with Enter or Space, extends with shift, and keeps Space from scrolling the page', () => {
    const onSelect = vi.fn();
    render(<Held onSelect={onSelect} />);
    fireEvent.keyDown(sentence(2), { key: 'Enter' });
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 2, endLine: 2 });
    fireEvent.keyDown(sentence(4), { key: 'Enter', shiftKey: true });
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 2, endLine: 4 });
    fireEvent.keyDown(sentence(1), { key: 'Enter' });
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 1, endLine: 1 });
    const notPrevented = fireEvent.keyDown(sentence(3), { key: ' ' });
    expect(notPrevented).toBe(false);
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 3, endLine: 3 });
    fireEvent.keyDown(sentence(4), { key: ' ', shiftKey: true });
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 3, endLine: 4 });
  });

  it('reports the line of the sentence clicked, and clears it when it is clicked again', async () => {
    const onSelect = vi.fn();
    render(<Held onSelect={onSelect} />);
    const user = userEvent.setup();
    await user.click(sentence(3));
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 3, endLine: 3 });
    expect(sentence(3)).toHaveAttribute('aria-pressed', 'true');
    expect(sentence(4)).toHaveAttribute('aria-pressed', 'false');
    await user.click(sentence(3));
    expect(onSelect).toHaveBeenLastCalledWith(null);
    expect(sentence(3)).toHaveAttribute('aria-pressed', 'false');
  });

  it('extends the sentence picked to a run with a shift-click', async () => {
    const onSelect = vi.fn();
    render(<Held onSelect={onSelect} />);
    const user = userEvent.setup();
    await user.click(sentence(2));
    fireEvent.click(sentence(4), { shiftKey: true });
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 2, endLine: 4 });
    for (const line of [2, 3, 4]) expect(sentence(line)).toHaveAttribute('aria-pressed', 'true');
    expect(sentence(1)).toHaveAttribute('aria-pressed', 'false');
    await user.click(sentence(1));
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 1, endLine: 1 });
  });

  it('shows the number of a sentence that is picked, to the eye only', () => {
    const { container } = render(<Held initial={{ startLine: 3, endLine: 3 }} />);
    const numbers = [...container.querySelectorAll('sup')];
    expect(numbers.map((number) => number.textContent)).toEqual(['1', '2', '3', '4']);
    expect(numbers.map((number) => number.classList.contains('opacity-100'))).toEqual([false, false, true, false]);
    expect(numbers.every((number) => number.getAttribute('aria-hidden') === 'true')).toBe(true);
  });

  it('is one stop in the tab order, moves between sentences with the arrow keys and picks with Enter', async () => {
    const onSelect = vi.fn();
    render(<Held onSelect={onSelect} />);
    const user = userEvent.setup();
    expect(screen.getAllByRole('button').filter((button) => button.tabIndex === 0)).toHaveLength(1);
    await user.tab();
    expect(sentence(1)).toHaveFocus();
    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(sentence(3)).toHaveFocus();
    expect(sentence(3)).toHaveAttribute('tabindex', '0');
    expect(sentence(1)).toHaveAttribute('tabindex', '-1');
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 3, endLine: 3 });
    await user.keyboard('{ArrowUp}{Home}');
    expect(sentence(1)).toHaveFocus();
    await user.keyboard('{End}');
    expect(sentence(4)).toHaveFocus();
  });

  it('extends the run with shift and the arrow keys', async () => {
    const onSelect = vi.fn();
    render(<Held onSelect={onSelect} />);
    const user = userEvent.setup();
    await user.click(sentence(2));
    await user.keyboard('{Shift>}{ArrowDown}{ArrowDown}{/Shift}');
    expect(onSelect).toHaveBeenLastCalledWith({ startLine: 2, endLine: 4 });
    expect(sentence(4)).toHaveFocus();
  });

  it('is an editor of the whole explanation, one sentence to a line, while a fix is written', () => {
    const onFixEdit = vi.fn();
    render(<Held writingFix onFixEdit={onFixEdit} />);
    const editor = screen.getByRole('textbox', { name: 'Edit the explanation' });
    expect(editor).toHaveValue(TEXT);
    expect(screen.getByText(/one sentence to a line/)).toBeInTheDocument();
    expect(screen.queryByRole('button')).toBeNull();
    fireEvent.change(editor, { target: { value: 'Changed.\n' } });
    expect(onFixEdit).toHaveBeenCalledWith('explanation.md', 'Changed.\n');
  });

  it('shows the copy of the learner where they changed the explanation', () => {
    render(<Held writingFix fixEdits={{ 'explanation.md': 'Mine.\n' }} />);
    expect(screen.getByRole('textbox', { name: 'Edit the explanation' })).toHaveValue('Mine.\n');
  });
});
