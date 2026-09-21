import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import type { SourceWithMetadata } from '@/types/conversation';

import { WebArticleView } from '../WebArticleView';

const mocks = vi.hoisted(() => ({ readWebPage: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { readWebPage: mocks.readWebPage } }));

let answer = '';
// Stable between renders: the reader memoises on these.
const messageVerification = new Map();
let conversations: unknown[] = [];
vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ messageVerification, conversations }),
}));

function setAnswer(content: string) {
  answer = content;
  conversations = [{ id: 'conversation', messages: [{ id: 'answer', role: 'assistant', content: answer }] }];
}

const URL = 'https://example.org/indoor-vegetables';

// One page, cited from two sentences a long way apart in the answer.
const ANSWER = [
  'Potatoes grown in deep fabric bags give the most calories for the floor space they take [6].',
  '',
  'Leafy greens should stay a small share of the shelf [2].',
  '',
  'Dwarf tomato varieties need at least eight hours of strong light every day to set fruit [6].',
].join('\n');

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
};

function renderReader(occurrence: number | null) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const reader = (at: number | null) => (
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <WebArticleView url={URL} source={source} ownerKey="answer" occurrence={at} />
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
  mocks.readWebPage.mockResolvedValue({
    ok: true,
    data: { url: URL, title: 'Indoor vegetables', text: PAGE, wordCount: 60, fetchedAt: '2026-09-20T00:00:00.000Z', fromCache: true },
  });
  setAnswer(ANSWER);
});

describe('a page the answer cites from more than one sentence', () => {
  it('opens on the passage for the mark that was clicked, not the first one', async () => {
    const first = renderReader(0);
    expect(await litPassage()).toContain('potatoes grown in deep fabric bags');
    expect(screen.getByText(/Potatoes grown in deep fabric bags give the most calories/)).toBeTruthy();
    first.unmount();

    // The same [6], further down the answer: the same page, a different passage.
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
    setAnswer(
      `${ANSWER}\n\nA quarterly budget of forty dollars covers seed, compost and replacement bulbs [6].`
    );

    const reader = renderReader(1);
    expect(await litPassage()).toContain('omato');
    const article = document.querySelector('.source-reader-passage')!.closest('.overflow-y-auto')!;
    article.scrollTop = 400;

    reader.click(2);

    // Sitting on the tomato passage would say the budget sentence came from it.
    await waitFor(() => expect(document.querySelector('.source-reader-passage.is-lit')).toBeNull());
    expect(article.scrollTop).toBe(0);
  });

  it('lights nothing, and says so, when the page has nothing like the clicked sentence', async () => {
    setAnswer(
      `${ANSWER}\n\nA quarterly budget of forty dollars covers seed, compost and replacement bulbs [6].`
    );

    renderReader(2);
    await waitFor(() => expect(document.querySelector('.source-reader-passage')).not.toBeNull());

    // Another sentence's passage would answer a question nobody asked.
    expect(document.querySelector('.source-reader-passage.is-lit')).toBeNull();
    expect(screen.getByText(/No passage on this page closely matches that sentence/)).toBeTruthy();
  });
});
