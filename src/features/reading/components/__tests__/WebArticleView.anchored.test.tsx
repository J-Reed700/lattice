import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { WebArticleView } from '@/features/reading/components/WebArticleView';
import type { AnchorDto } from '@/lib/bindings';
import { reply } from '@/tests/replyFixtures';
import type { SourceWithMetadata } from '@/types/conversation';

vi.mock('@/lib/api', () => ({ default: { readWebPage: vi.fn() } }));

const messageVerification = new Map();
let conversations: unknown[] = [];
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ messageVerification, conversations }),
}));

const URL = 'https://example.org/tea';

// Multi-byte characters stand before every sentence the anchors name.
const PAGE = [
  'Café ☕ notes on 日本茶.',
  '## Brewing',
  'The kettle boils at one hundred degrees. Tea is hot and sweet.',
  '## Storage',
  'Herbs keep on any windowsill. Leaves stay dry in a tin.',
].join('\n\n');

const bytesBefore = (index: number): number => new TextEncoder().encode(PAGE.slice(0, index)).length;

function anchorOn(exact: string, base = 0): AnchorDto {
  const at = PAGE.indexOf(exact);
  return { exact, span: { start: base + bytesBefore(at), end: base + bytesBefore(at + exact.length) } };
}

const baseSource: SourceWithMetadata = {
  documentId: 'web-6',
  chunkId: 'chunk-6',
  fileName: 'On tea',
  filePath: URL,
  mimeType: 'text/html',
  category: 'web',
  content: 'A search snippet.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 6,
  webSnapshot: { url: URL, title: 'On tea', text: PAGE, fetchedAt: null, truncated: false },
};

function renderReader(source: SourceWithMetadata, occurrence: number | null, extra: { locatorText?: string } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <WebArticleView
          url={URL}
          source={source}
          ownerKey="answer"
          occurrence={occurrence}
          initialLocator={extra.locatorText ? { text: extra.locatorText, chunkId: 'chunk-6' } : null}
        />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

const marked = (): string[] =>
  Array.from(document.querySelectorAll('.source-reader-passage')).map((mark) => mark.textContent ?? '');
const lit = (): string => document.querySelector('.source-reader-passage.is-lit')?.textContent ?? '';

beforeEach(() => {
  // The answer's own words would match "kettle" lexically; an anchored reader must ignore them.
  conversations = [
    {
      id: 'conversation',
      messages: [
        {
          id: 'answer',
          role: 'assistant',
          content: '',
          reply: reply('The kettle boils at one hundred degrees on the hob ', 6, '.'),
        },
      ],
    },
  ];
});

describe('WebArticleView over anchored chips', () => {
  const tea = anchorOn('Tea is hot and sweet.');
  const herbs = anchorOn('Herbs keep on any windowsill.');

  it('marks exactly the anchored sentences and nothing lexical', async () => {
    renderReader(
      {
        ...baseSource,
        anchoredBy: [
          { occurrence: 0, relation: 'restates', anchors: [tea] },
          { occurrence: 2, relation: 'quotes', anchors: [herbs] },
        ],
      },
      0,
    );

    await waitFor(() => expect(marked()).not.toHaveLength(0));
    expect(marked().join('|')).toBe('Tea is hot and sweet.|Herbs keep on any windowsill.');
    expect(lit()).toBe('Tea is hot and sweet.');
  });

  it('lights the sentence of the chip that was clicked, found by its occurrence', async () => {
    renderReader(
      {
        ...baseSource,
        anchoredBy: [
          { occurrence: 0, relation: 'restates', anchors: [tea] },
          { occurrence: 2, relation: 'quotes', anchors: [herbs] },
        ],
      },
      2,
    );

    await waitFor(() => expect(lit()).toBe('Herbs keep on any windowsill.'));
  });

  it('places anchors in a passage shown alone from the byte it begins at', async () => {
    const start = PAGE.indexOf('The kettle');
    const passage = PAGE.slice(start, PAGE.indexOf('## Storage'));
    const base = 5000 + bytesBefore(start);
    const alone: SourceWithMetadata = {
      ...baseSource,
      webSnapshot: { url: URL, title: 'On tea', text: passage, fetchedAt: null, truncated: false },
      anchorBase: base,
      anchoredBy: [{ occurrence: 0, relation: 'restates', anchors: [anchorOn('Tea is hot and sweet.', base - bytesBefore(start))] }],
    };
    renderReader(alone, 0);

    await waitFor(() => expect(lit()).toBe('Tea is hot and sweet.'));
    expect(marked()).toEqual(['Tea is hot and sweet.']);
  });

  it('says so when no sentence of the clicked chip is in the saved text', async () => {
    renderReader(
      {
        ...baseSource,
        anchoredBy: [{ occurrence: 0, relation: 'quotes', anchors: [{ exact: 'A sentence nobody saved here.' }] }],
      },
      0,
    );

    await screen.findByRole('heading', { name: 'On tea' });

    expect(screen.getByText('The cited sentence is not in the saved text of this source.')).toBeInTheDocument();
    expect(marked()).toEqual([]);
  });

  it('does not say so when a sentence of the clicked chip is placed', async () => {
    renderReader(
      {
        ...baseSource,
        anchoredBy: [{ occurrence: 0, relation: 'restates', anchors: [tea, { exact: 'Not on this page.' }] }],
      },
      0,
    );

    await waitFor(() => expect(lit()).toBe('Tea is hot and sweet.'));
    expect(screen.queryByText('The cited sentence is not in the saved text of this source.')).toBeNull();
  });

  it('marks the source passage for a chip with no anchors, as it does without them', async () => {
    renderReader(
      { ...baseSource, anchoredBy: [{ occurrence: 0, relation: 'backs', anchors: [] }] },
      0,
      { locatorText: 'Leaves stay dry in a tin.' },
    );

    await waitFor(() => expect(lit()).toBe('Leaves stay dry in a tin.'));
  });
});
