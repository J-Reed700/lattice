import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { WebArticleView } from '@/features/reading/components/WebArticleView';
import { reply } from '@/tests/replyFixtures';
import type { SourceWithMetadata } from '@/types/conversation';
import type { ReplyPiece } from '@/utils/replyPieces';


const mocks = vi.hoisted(() => ({ readWebPage: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { readWebPage: mocks.readWebPage } }));

// Stable between renders: the reader memoises on these.
const messageVerification = new Map();
let conversations: unknown[] = [];
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ messageVerification, conversations }),
}));

function setAnswer(parts: ReplyPiece[]) {
  conversations = [{ id: 'conversation', messages: [{ id: 'answer', role: 'assistant', content: '', reply: parts }] }];
}

const URL = 'https://example.org/indoor-vegetables';

// One page, cited from two sentences a long way apart in the answer.
const ANSWER = reply(
  'Potatoes grown in deep fabric bags give the most calories for the floor space they take ', 6, '.\n\n',
  'Leafy greens should stay a small share of the shelf ', 2, '.\n\n',
  'Dwarf tomato varieties need at least eight hours of strong light every day to set fruit ', 6, '.',
);

// The same answer as a note holds it: the notes format's own `[n]` markers.
const NOTE = [
  'Potatoes grown in deep fabric bags give the most calories for the floor space they take [6].',
  'Leafy greens should stay a small share of the shelf [2].',
  'Dwarf tomato varieties need at least eight hours of strong light every day to set fruit [6].',
].join('\n\n');

const PAGE = [
  'Growing food indoors is mostly a question of light and containers.',
  'For calories, potatoes grown in deep fabric bags give the most food for the floor space they take up.',
  'Herbs are forgiving and will grow on almost any windowsill.',
  'Dwarf tomato varieties set fruit indoors, but they need at least eight hours of strong light every day.',
].join('\n\n');

const source: SourceWithMetadata = {
  documentId: 'web-6',
  chunkId: 'chunk-6',
  fileName: 'Indoor vegetables',
  filePath: URL,
  mimeType: 'text/html',
  category: 'web',
  content: 'A search snippet.',
  score: 0.5,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 6,
  webSnapshot: {
    url: URL,
    title: 'Indoor vegetables',
    text: PAGE,
    fetchedAt: '2026-09-20T00:00:00.000Z',
    truncated: false,
  },
};

function renderReader(occurrence: number | null, sourceOverride = source) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const reader = (at: number | null) => (
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <WebArticleView url={URL} source={sourceOverride} ownerKey="answer" occurrence={at} />
      </TooltipProvider>
    </QueryClientProvider>
  );
  const view = render(reader(occurrence));
  return { ...view, click: (at: number | null) => view.rerender(reader(at)) };
}

const litPassage = async (): Promise<string> => {
  await waitFor(() => expect(document.querySelector('.source-reader-passage')).not.toBeNull());
  return document.querySelector('.source-reader-passage.is-lit')?.textContent ?? '';
};

beforeEach(() => {
  mocks.readWebPage.mockReset();
  setAnswer(ANSWER);
});

describe('a page the answer cites from more than one sentence', () => {
  it('cross-references a journal citation without a loaded chat conversation', async () => {
    conversations = [];
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <WebArticleView url={URL} source={source} citationContent={NOTE} occurrence={1} />
        </TooltipProvider>
      </QueryClientProvider>,
    );
    expect(await litPassage()).toContain('eight hours of strong light');
    expect(document.querySelectorAll('.source-reader-passage')).toHaveLength(2);
    expect(await litPassage()).not.toContain('potatoes');
  });

  it('opens on the passage for the mark that was clicked, not the first one', async () => {
    const first = renderReader(0);
    expect(await litPassage()).toContain('potatoes grown in deep fabric bags');
    expect(screen.getByText(/Potatoes grown in deep fabric bags give the most calories/)).toBeTruthy();
    expect(mocks.readWebPage).not.toHaveBeenCalled();
    first.unmount();

    // The same source, further down the answer: the same page, a different passage.
    renderReader(1);
    expect(await litPassage()).toContain('eight hours of strong light');
    expect(await litPassage()).not.toContain('potatoes');
  });

  it('still marks the other passages, so the rest of what the page backs is one step away', async () => {
    renderReader(1);
    await litPassage();

    expect(document.querySelectorAll('.source-reader-passage')).toHaveLength(2);
    expect(document.querySelectorAll('.source-reader-passage.is-lit')).toHaveLength(1);
  });

  it('opens on the first passage when the source was opened as a whole', async () => {
    renderReader(null);

    expect(await litPassage()).toContain('potatoes grown in deep fabric bags');
    expect(screen.queryByText(/For the sentence/)).toBeNull();
  });

  it('leaves the last passage behind when the next mark has none of its own', async () => {
    setAnswer([...ANSWER, ...reply('\n\nA quarterly budget of forty dollars covers seed, compost and replacement bulbs ', 6, '.')]);

    const reader = renderReader(1);
    expect(await litPassage()).toContain('omato');
    const article = document.querySelector('.source-reader-passage')!.closest('.overflow-y-auto')!;
    article.scrollTop = 400;

    reader.click(2);

    // Sitting on the tomato passage would say the budget sentence came from it.
    await waitFor(() => expect(document.querySelector('.source-reader-passage.is-lit')).toBeNull());
    expect(article.scrollTop).toBe(0);
  });

  it('opens a passage at its first paragraph, not its last', async () => {
    // One answer sentence off two paragraphs. The mark is drawn once in each,
    // and only the first of them is where the passage begins.
    const page = [
      'For calories, potatoes grown in deep fabric bags give the most food for the floor space they take up.',
      'Airflow matters more than most people expect; a small fan keeps mould off damp compost.',
      'Herbs are forgiving and will grow on almost any windowsill.',
      'Dwarf tomato varieties set fruit indoors, but they need at least eight hours of strong light every day.',
    ].join('\n\n');
    const pageSource: SourceWithMetadata = {
      ...source,
      webSnapshot: { ...source.webSnapshot!, text: page },
    };
    setAnswer([
      ...ANSWER,
      ...reply('\n\nHerbs grow on almost any windowsill, and dwarf tomato varieties set fruit indoors given at least eight hours of strong light ', 6, '.'),
    ]);

    const reader = renderReader(0, pageSource);
    await litPassage();

    // The potato passage, then the herb-and-tomato one twice: once per paragraph.
    const marks = [...document.querySelectorAll('.source-reader-passage')] as HTMLElement[];
    expect(marks).toHaveLength(3);

    const article = marks[0]!.closest('.overflow-y-auto') as HTMLElement;
    const scrollTo = vi.fn();
    article.scrollTo = scrollTo as unknown as HTMLElement['scrollTo'];
    Object.defineProperty(article, 'scrollHeight', { value: 2000, configurable: true });
    Object.defineProperty(article, 'clientHeight', { value: 300, configurable: true });
    article.getBoundingClientRect = () => ({ top: 0 }) as DOMRect;
    marks.forEach((mark, index) => {
      mark.getBoundingClientRect = () => ({ top: 100 + index * 400 }) as DOMRect;
    });

    reader.click(2);

    // The mark that opens in the third paragraph, a third of the pane down —
    // not the one closing in the fourth, which is where the passage ends.
    await waitFor(() => expect(scrollTo).toHaveBeenCalledWith(expect.objectContaining({ top: 400 })));
  });

  it('lights nothing, and says so, when the page has nothing like the clicked sentence', async () => {
    setAnswer([...ANSWER, ...reply('\n\nA quarterly budget of forty dollars covers seed, compost and replacement bulbs ', 6, '.')]);

    renderReader(2);
    await waitFor(() => expect(document.querySelector('.source-reader-passage')).not.toBeNull());

    // Another sentence's passage would answer a question nobody asked.
    expect(document.querySelector('.source-reader-passage.is-lit')).toBeNull();
    expect(screen.getByText(/No passage on this page closely matches that sentence/)).toBeTruthy();
  });

  it('never fetches a live page and clearly labels citations without a saved snapshot', async () => {
    const legacySource: SourceWithMetadata = {
      ...source,
      webSnapshot: undefined,
      content: 'A saved legacy citation excerpt.',
    };
    renderReader(null, legacySource);

    expect(screen.getByText('A saved legacy citation excerpt.')).toBeTruthy();
    expect(screen.getByText(/full page snapshot was not saved with this citation/i)).toBeTruthy();
    expect(mocks.readWebPage).not.toHaveBeenCalled();
  });

  it('keeps snapshots separate when two citations share a URL', () => {
    const secondSource: SourceWithMetadata = {
      ...source,
      webSnapshot: { ...source.webSnapshot!, text: 'A different immutable snapshot for another conversation.' },
    };
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { container } = render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <div>
            <WebArticleView url={URL} source={source} />
            <WebArticleView url={URL} source={secondSource} />
          </div>
        </TooltipProvider>
      </QueryClientProvider>,
    );
    const text = container.textContent ?? '';
    expect(text).toContain('Growing food indoors is mostly a question of light and containers.');
    expect(text).toContain('A different immutable snapshot for another conversation.');
    expect(mocks.readWebPage).not.toHaveBeenCalled();
  });

  it('discloses when the saved snapshot was truncated', () => {
    const truncatedSource: SourceWithMetadata = {
      ...source,
      webSnapshot: { ...source.webSnapshot!, truncated: true },
    };
    renderReader(null, truncatedSource);

    expect(screen.getByText(/snapshot was truncated when captured/i)).toBeTruthy();
    expect(mocks.readWebPage).not.toHaveBeenCalled();
  });

  it('highlights and scrolls to initialLocator text when opened from non-chat surface without answer sentences', async () => {
    conversations = [];
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <WebArticleView
            url={URL}
            source={source}
            initialLocator={{
              text: 'Herbs are forgiving and will grow on almost any windowsill.',
              chunkId: 'chunk-herbs',
            }}
          />
        </TooltipProvider>
      </QueryClientProvider>,
    );

    const mark = document.querySelector('.source-reader-passage');
    expect(mark).not.toBeNull();
    expect(mark?.textContent).toContain('Herbs are forgiving and will grow on almost any windowsill.');
    expect(mark?.getAttribute('title')).toContain('Matches: Herbs are forgiving');
    expect(mark?.classList.contains('is-lit')).toBe(true);
  });

  it('highlights saved excerpt on full page text when no initialLocator is provided and no answer sentences', async () => {
    conversations = [];
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const excerptSource: SourceWithMetadata = {
      ...source,
      excerpt: 'Dwarf tomato varieties set fruit indoors',
    };
    render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <WebArticleView url={URL} source={excerptSource} />
        </TooltipProvider>
      </QueryClientProvider>,
    );

    const mark = document.querySelector('.source-reader-passage');
    expect(mark).not.toBeNull();
    expect(mark?.textContent).toContain('Dwarf tomato varieties set fruit indoors');
  });
});

describe('saved Markdown text', () => {
  const MARKDOWN = [
    '## Heading',
    'Paragraph right after the heading mentions beta gamma delta clearly.',
    '',
    '### Minor',
    '',
    '```ts',
    'const a = 1;',
    '',
    'const b = 2;',
    '```',
    '',
    '- first item',
    '- second item',
    '',
    'A plain paragraph.',
  ].join('\n');

  function renderMarkdown(
    options: { quote?: string; documentId?: string; text?: string; onActivePassageChange?: (_text: string | null) => void } = {},
  ) {
    const { quote, documentId = 'web-6', text = MARKDOWN, onActivePassageChange } = options;
    const markdownSource: SourceWithMetadata = {
      ...source,
      documentId,
      webSnapshot: { ...source.webSnapshot!, text },
    };
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return render(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <WebArticleView
            url={null}
            source={markdownSource}
            initialLocator={quote ? { text: quote } : null}
            onActivePassageChange={onActivePassageChange}
          />
        </TooltipProvider>
      </QueryClientProvider>,
    );
  }

  it('draws headings as h4 for level 2 and under and h5 below, without their markers', () => {
    renderMarkdown();

    expect(screen.getByRole('heading', { level: 4, name: 'Heading' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 5, name: 'Minor' })).toBeInTheDocument();
  });

  it('draws fenced code as pre > code with its exact text', () => {
    renderMarkdown();

    expect(document.querySelector('pre > code')?.textContent).toBe('const a = 1;\n\nconst b = 2;');
  });

  it('draws a list as a list with its markers hidden, and paragraphs as paragraphs', () => {
    renderMarkdown();

    const items = screen.getAllByRole('listitem');
    expect(items.map((item) => item.textContent)).toEqual(['first item', 'second item']);
    expect(items[0]!.parentElement!.tagName).toBe('UL');
    expect(screen.queryByText(/- first item/)).not.toBeInTheDocument();
    expect(screen.getByText('A plain paragraph.').tagName).toBe('P');
  });

  it('draws the lead-in line before a list as a paragraph of its own', () => {
    renderMarkdown({ text: 'Ingredients:\n- flour\n- sugar' });

    expect(screen.getByText('Ingredients:').tagName).toBe('P');
    expect(screen.getAllByRole('listitem').map((item) => item.textContent)).toEqual(['flour', 'sugar']);
  });

  it('draws inline formatting as elements without its syntax', () => {
    renderMarkdown({
      text: 'A **bold** word, an *em* word, `code`, ~~gone~~ and [a link](https://example.com/x "T").',
    });

    expect(screen.getByText('bold').tagName).toBe('STRONG');
    expect(screen.getByText('em').tagName).toBe('EM');
    expect(screen.getByText('code').tagName).toBe('CODE');
    expect(screen.getByText('gone').tagName).toBe('DEL');
    const link = screen.getByRole('link', { name: 'a link' });
    expect(link).toHaveAttribute('href', 'https://example.com/x');
    expect(document.querySelector('p.mb-4')!.textContent).toBe('A bold word, an em word, code, gone and a link.');
  });

  it('draws nested lists, ordered lists, quotes, tables and rules as elements', () => {
    renderMarkdown({
      text: [
        '- one',
        '  - nested',
        '- two',
        '',
        '3. third',
        '4. fourth',
        '',
        '> a quoted',
        'line',
        '',
        '| Name | Count |',
        '|:--|--:|',
        '| apples | **3** |',
        '',
        '***',
        '',
        'After.',
      ].join('\n'),
    });

    const outer = screen.getAllByRole('list')[0]!;
    expect(outer.tagName).toBe('UL');
    expect(outer.querySelector(':scope > li > ul > li')?.textContent).toBe('nested');
    const ordered = screen.getAllByRole('list').find((list) => list.tagName === 'OL')!;
    expect(ordered).toHaveAttribute('start', '3');
    expect(ordered.querySelectorAll('li')).toHaveLength(2);
    expect(document.querySelector('blockquote')?.textContent).toBe('a quoted line');
    const table = screen.getByRole('table');
    expect(screen.getAllByRole('columnheader').map((cell) => cell.textContent)).toEqual(['Name', 'Count']);
    expect(screen.getAllByRole('columnheader')[1]).toHaveClass('text-right');
    expect(screen.getAllByRole('cell').map((cell) => cell.textContent)).toEqual(['apples', '3']);
    expect(table.parentElement).toHaveClass('overflow-x-auto');
    expect(document.querySelector('hr')).not.toBeNull();
    expect(screen.getByText('After.')).toBeInTheDocument();
  });

  it('shows syntax it does not recognise as the raw text it is', () => {
    const text = 'Refs [a][1] and [b] and <div>x</div> and 2 * 3 * 4 and snake_case_name and [c](javascript:x).';
    renderMarkdown({ text });

    expect(document.querySelector('p.mb-4')!.textContent).toBe(
      'Refs [a][1] and [b] and <div>x</div> and 2 * 3 * 4 and snake_case_name and c.',
    );
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });

  it('marks one passage across bold text as a single mark with the formatting inside it', async () => {
    renderMarkdown({
      text: 'Intro words.\n\nThe quick **brown fox** jumps over the lazy dog today.',
      quote: 'quick **brown fox** jumps over',
    });

    await waitFor(() => expect(document.querySelector('.source-reader-passage')).not.toBeNull());
    const paragraph = screen.getByText('brown fox').closest('p')!;
    const marks = paragraph.querySelectorAll('.source-reader-passage');
    expect(marks).toHaveLength(1);
    expect(marks[0]!.textContent).toBe('quick brown fox jumps over');
    expect(marks[0]!.querySelector('strong')?.textContent).toBe('brown fox');
    expect(marks[0]).toHaveClass('is-lit');
  });

  it('gives onActivePassageChange the passage as words, without the Markdown syntax', async () => {
    const onActivePassageChange = vi.fn();
    renderMarkdown({
      text: 'Intro words.\n\nThe quick **brown fox** jumps over the [lazy dog](https://example.com) today.',
      quote: 'quick **brown fox** jumps over the [lazy dog](https://example.com)',
      onActivePassageChange,
    });

    await waitFor(() => expect(onActivePassageChange).toHaveBeenCalledWith('quick brown fox jumps over the lazy dog'));
    expect(onActivePassageChange.mock.calls.some(([value]) => typeof value === 'string' && value.includes('*'))).toBe(false);
  });

  it('counts the words a reader sees in the meta line', () => {
    renderMarkdown({ text: '# Title\n\nSome **bold** words [here](https://example.com).' });

    // Title, Some, bold, words, here.
    expect(screen.getByText(/5 words/)).toBeInTheDocument();
  });

  it('does not count table separators or stray punctuation as words', () => {
    renderMarkdown({ text: '| Name | Count |\n|---|---|\n| apples | 3 |\n\nA - B' });

    // Name, Count, apples, 3, A, B.
    expect(screen.getByText(/6 words/)).toBeInTheDocument();
  });

  it('marks exactly the quoted words in a paragraph that follows a heading', async () => {
    renderMarkdown({ quote: 'beta gamma delta' });

    await waitFor(() => expect(document.querySelector('.source-reader-passage')).not.toBeNull());
    const marks = document.querySelectorAll('.source-reader-passage');
    expect(marks).toHaveLength(1);
    expect(marks[0]!.textContent).toBe('beta gamma delta');
    expect(marks[0]!.closest('p')?.textContent).toBe(
      'Paragraph right after the heading mentions beta gamma delta clearly.',
    );
  });

  it('labels a course source without an address "Saved source"', () => {
    renderMarkdown({ documentId: 'course:abc:1' });
    expect(screen.getByText('Saved source')).toBeInTheDocument();
  });

  it('labels another source without an address "Web source"', () => {
    renderMarkdown();
    expect(screen.getByText('Web source')).toBeInTheDocument();
  });
});
