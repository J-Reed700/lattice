import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ArticleText, type ArticlePassage } from '@/features/reading/components/ArticleText';

const mocks = vi.hoisted(() => ({ why: vi.fn(), error: vi.fn() }));
vi.mock('@/utils/openExternalUrl', () => ({ whyExternalUrlDidNotOpen: mocks.why }));
vi.mock('@/stores/toastStore', () => ({ toast: { error: mocks.error } }));

beforeEach(() => {
  vi.resetAllMocks();
});

/** The passage covering `quote`, found in the raw text. */
function passageOf(text: string, quote: string, title?: string): ArticlePassage {
  const start = text.indexOf(quote);
  if (start < 0) throw new Error(`"${quote}" is not in the text`);
  return { start, end: start + quote.length, title };
}

describe('ArticleText', () => {
  it('draws a table with its header, body, alignment and a scroll wrapper', () => {
    const text = '| Left | Mid | Right |\n|:--|:-:|--:|\n| a | `b|c` | **d** |';
    render(<ArticleText text={text} />);

    const table = screen.getByRole('table');
    expect(table.parentElement).toHaveClass('mb-4', 'overflow-x-auto');
    const headers = screen.getAllByRole('columnheader');
    expect(headers.map((cell) => cell.textContent)).toEqual(['Left', 'Mid', 'Right']);
    expect(headers.map((cell) => cell.className.match(/text-(left|center|right)/)?.[0])).toEqual([
      'text-left',
      'text-center',
      'text-right',
    ]);
    expect(screen.getAllByRole('cell').map((cell) => cell.textContent)).toEqual(['a', 'b|c', 'd']);
    expect(screen.getByText('d').tagName).toBe('STRONG');
  });

  it('keeps code block content literal', () => {
    render(<ArticleText text={'```md\n**not bold** [x](https://a.test)\n```'} />);
    expect(document.querySelector('pre > code')?.textContent).toBe('**not bold** [x](https://a.test)');
    expect(document.querySelector('strong')).toBeNull();
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });

  it('draws an image as its alt text in an italic muted span', () => {
    render(<ArticleText text="See ![a chart](c.png) here." />);
    const alt = screen.getByText('a chart');
    expect(alt.tagName).toBe('SPAN');
    expect(alt).toHaveClass('italic');
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });

  it('opens a link with one attempt, does not navigate, and says nothing when it opened', async () => {
    mocks.why.mockResolvedValue(null);
    render(<ArticleText text="Read [the docs](https://example.com/docs)." />);

    const link = screen.getByRole('link', { name: 'the docs' });
    expect(link).toHaveAttribute('title', 'https://example.com/docs');
    expect(link).toHaveClass('underline');
    expect(fireEvent.click(link)).toBe(false);
    await waitFor(() => expect(mocks.why).toHaveBeenCalledTimes(1));
    expect(mocks.why).toHaveBeenCalledWith('https://example.com/docs');
    await Promise.resolve();
    expect(mocks.error).not.toHaveBeenCalled();
  });

  it('says why when a link could not be opened, after a single attempt', async () => {
    mocks.why.mockResolvedValue('The shell refused.');
    render(<ArticleText text="[docs](https://example.com)" />);

    fireEvent.click(screen.getByRole('link', { name: 'docs' }));
    await waitFor(() => expect(mocks.error).toHaveBeenCalledWith("Couldn't open link", { message: 'The shell refused.' }));
    expect(mocks.why).toHaveBeenCalledTimes(1);
  });

  it('shows a mailto link as its label with no link', () => {
    render(<ArticleText text="[mail me](mailto:a@b.test) or <mailto:c@d.test>" />);
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
    expect(document.querySelector('p')!.textContent).toBe('mail me or mailto:c@d.test');
  });

  describe('marks', () => {
    it('puts one mark around a passage across formatting, with the elements inside it', () => {
      const text = 'Say **hello *brave* world** now';
      const passages = [passageOf(text, 'hello *brave* world', 'Matches: hello')];
      render(<ArticleText text={text} passages={passages} activeIndex={0} />);

      const marks = document.querySelectorAll('mark.source-reader-passage');
      expect(marks).toHaveLength(1);
      expect(marks[0]).toHaveClass('is-lit');
      expect(marks[0]).toHaveAttribute('title', 'Matches: hello');
      expect(marks[0]!.textContent).toBe('hello brave world');
      expect(marks[0]!.querySelector('strong')).not.toBeNull();
      expect(marks[0]!.querySelector('em')?.textContent).toBe('brave');
    });

    it('splits formatting at the mark boundary, not the mark at the formatting', () => {
      const text = 'A **bold phrase here** end';
      render(<ArticleText text={text} passages={[passageOf(text, 'phrase')]} />);

      const mark = document.querySelector('mark')!;
      expect(mark.textContent).toBe('phrase');
      expect(mark.parentElement?.tagName).toBe('P');
      expect(mark.querySelector('strong')?.textContent).toBe('phrase');
      expect([...document.querySelectorAll('strong')].map((node) => node.textContent)).toEqual(['bold ', 'phrase', ' here']);
    });

    it('does not light a passage that is not the active one', () => {
      const text = 'one two three';
      render(<ArticleText text={text} passages={[passageOf(text, 'two')]} activeIndex={3} />);
      expect(document.querySelector('mark')).not.toHaveClass('is-lit');
    });

    it('registers the first mark of each passage, even when the passage starts inside a list marker or bold syntax', () => {
      const text = 'Intro.\n\n- alpha beta\n- gamma delta\n\n**bold start** after';
      const passages = [
        { start: text.indexOf('- alpha'), end: text.indexOf('beta') + 4 },
        { start: text.indexOf('**bold'), end: text.indexOf('start') + 5 },
      ];
      const registered = new Map<number, HTMLElement | null>();
      const onMarkElement = vi.fn((index: number, element: HTMLElement | null) => registered.set(index, element));
      render(<ArticleText text={text} passages={passages} onMarkElement={onMarkElement} />);

      expect(registered.get(0)?.textContent).toBe('alpha beta');
      expect(registered.get(1)?.textContent).toBe('bold start');
    });

    it('registers only the first piece of a passage that runs across blocks', () => {
      const text = 'First paragraph ends here.\n\n- item one\n- item two\n\nLast one starts there.';
      const passages = [{ start: text.indexOf('ends'), end: text.indexOf('starts') }];
      const registered = vi.fn();
      render(<ArticleText text={text} passages={passages} onMarkElement={registered} />);

      const marks = document.querySelectorAll('mark');
      expect(marks.length).toBeGreaterThan(2);
      expect(marks[0]!.textContent).toBe('ends here.');
      expect(registered).toHaveBeenCalledTimes(1);
      expect(registered).toHaveBeenCalledWith(0, marks[0]);
    });

    it('marks inside list items, quote lines and table cells', () => {
      const text = '- first item\n\n> quoted words\n\n| h | k |\n|---|---|\n| cell one | two |';
      const passages = ['first item', 'quoted words', 'cell one'].map((quote) => passageOf(text, quote));
      render(<ArticleText text={text} passages={passages} />);

      const marks = [...document.querySelectorAll('mark')];
      expect(marks.map((mark) => mark.textContent)).toEqual(['first item', 'quoted words', 'cell one']);
      expect(marks.map((mark) => mark.closest('li, blockquote, td')?.tagName)).toEqual(['LI', 'BLOCKQUOTE', 'TD']);
    });

    it('draws an escape or entity wholly inside the mark that contains its start', () => {
      const text = 'a \\* b &amp; c';
      const passages = [{ start: text.indexOf('\\'), end: text.indexOf('\\') + 1 }];
      render(<ArticleText text={text} passages={passages} />);

      expect(document.querySelector('mark')!.textContent).toBe('*');
      expect(document.querySelector('p')!.textContent).toBe('a * b & c');
    });

    it('draws no mark for a passage that covers only syntax', () => {
      const text = 'a **b** c';
      const onMarkElement = vi.fn();
      render(<ArticleText text={text} passages={[{ start: 2, end: 4 }]} onMarkElement={onMarkElement} />);
      expect(document.querySelector('mark')).toBeNull();
      expect(onMarkElement).not.toHaveBeenCalled();
    });
  });

  describe('maxChars', () => {
    const text = ['a'.repeat(50), 'b'.repeat(50), 'c'.repeat(50)].join('\n\n');

    it('shows whole blocks until the visible characters reach the limit', () => {
      render(<ArticleText text={text} maxChars={60} />);
      expect(document.querySelectorAll('p')).toHaveLength(2);
    });

    it('always shows at least one block', () => {
      render(<ArticleText text={text} maxChars={1} />);
      expect(document.querySelectorAll('p')).toHaveLength(1);
    });

    it('counts visible characters, not syntax', () => {
      render(<ArticleText text={`${'**a**'.repeat(10)}\n\n${'**b**'.repeat(10)}\n\nlast`} maxChars={15} />);
      expect(document.querySelectorAll('p')).toHaveLength(2);
    });
  });
});
