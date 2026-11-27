import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { SavedSourceCitations, type SavedSourceCitation } from '@/features/learning/sources/SavedSourceCitations';
import { anchoredCardDetails } from '@/features/reading/citations';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { SourceWithMetadata } from '@/types/conversation';

const mocks = vi.hoisted(() => ({ workspace: vi.fn(), version: vi.fn() }));
const ok = <T,>(data: T) => ({ ok: true as const, data });

const summary = (id: string, title: string) => ({
  id, versionNumber: 1, title, publisher: null, resolvedUrl: null, excerpt: '', contentSha256: `sha-${id}`,
  wordCount: 3, truncated: false, extractionVersion: 'reader-1', acquiredAt: 1_790_000_000_000, snapshotHash: null,
});
const library = {
  programId: 'program-1',
  sources: [
    { id: 'source-a', kind: 'text', versions: [summary('va', 'Library alpha')] },
    { id: 'source-b', kind: 'text', versions: [summary('vb', 'Library beta')] },
  ],
};

function renderWith(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const api = { getLearningSourceWorkspace: mocks.workspace, getLearningSourceVersion: mocks.version } as unknown as ApiClient;
  const tree = (node: ReactNode) => (
    <QueryClientProvider client={client}><ApiProvider client={api}>{node}</ApiProvider></QueryClientProvider>
  );
  const view = render(tree(ui));
  return { ...view, rerenderWith: (node: ReactNode) => view.rerender(tree(node)) };
}

const citations: SavedSourceCitation[] = [
  {
    number: 1,
    versionId: 'va',
    quote: 'first quote',
    title: 'Own title',
    support: { relation: 'quotes', anchors: [{ exact: 'first quote', span: null }] },
  },
  {
    number: 2,
    versionId: 'vb',
    quote: 'second quote',
    support: { relation: 'quotes', anchors: [{ exact: 'second quote', span: null }] },
  },
  {
    number: 3,
    versionId: 'unknown',
    quote: '  ',
    support: { relation: 'cites', anchors: [] },
  },
];

const openChip = (n: number) => fireEvent.click(screen.getByRole('button', { name: new RegExp(`Citation ${n}`) }));
const placeholderSource: SourceWithMetadata = {
  documentId: 'x', chunkId: 'x', fileName: 'x', filePath: '', mimeType: 'text/plain', category: 'c', content: '', score: 1, fileSizeBytes: 0, modifiedAt: '',
};

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
  vi.resetAllMocks();
  useReaderStore.setState({ session: null });
  useCitationDisplayStore.setState({ visible: true });
  mocks.workspace.mockResolvedValue(ok(library));
  mocks.version.mockImplementation(async (request: { sourceId: string; versionId: string }) =>
    ok({ sourceId: request.sourceId, version: summary(request.versionId, 'Detail'), fullText: `Text of ${request.versionId}`, usage: [] }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('SavedSourceCitations', () => {
  it('without quotes draws one labelled row with a chip and title per citation', async () => {
    const { container } = renderWith(<SavedSourceCitations programId="program-1" citations={citations} ownerKey="owner-1" label="Cited" />);

    expect(screen.getByText('Cited')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /Citation \d/ })).toHaveLength(3);
    expect(container.querySelector('ol')).toBeNull();
    // citation.title wins, then the library's title, then the generic fallback.
    expect(screen.getByText('Own title')).toBeInTheDocument();
    expect(await screen.findByText('Library beta')).toBeInTheDocument();
    expect(screen.getByText('Saved source')).toBeInTheDocument();
  });

  it('defaults the row label to Sources', () => {
    renderWith(<SavedSourceCitations programId="program-1" citations={citations} ownerKey="owner-1" />);
    expect(screen.getByText('Sources')).toBeInTheDocument();
  });

  it('with quotes lists each chip, quote and title in an ordered list labelled by the label', async () => {
    renderWith(<SavedSourceCitations programId="program-1" citations={citations} quotes ownerKey="owner-1" label="Passages" />);

    const list = screen.getByRole('list', { name: 'Passages' });
    const items = within(list).getAllByRole('listitem');
    expect(items).toHaveLength(3);
    expect(within(items[0]!).getByRole('button', { name: /Citation 1/ })).toBeInTheDocument();
    expect(items[0]).toHaveTextContent('first quote — Own title');
    await waitFor(() => expect(items[1]).toHaveTextContent('second quote — Library beta'));
    // A blank quote has no quote and no dash.
    expect(items[2]!.textContent).toContain('Saved source');
    expect(items[2]!.textContent).not.toContain('—');
  });

  it('shows a quote as the words a reader sees, and opens the reader on the raw quote', async () => {
    const raw: SavedSourceCitation[] = [
      {
        number: 1,
        versionId: 'va',
        quote: 'The **x** matters, see [docs](https://example.com).',
        support: {
          relation: 'quotes',
          anchors: [{ exact: 'The **x** matters, see [docs](https://example.com).', span: null }],
        },
      },
    ];
    renderWith(<SavedSourceCitations programId="program-1" citations={raw} quotes ownerKey="owner-1" label="Passages" />);

    const item = within(screen.getByRole('list', { name: 'Passages' })).getAllByRole('listitem')[0]!;
    expect(item.querySelector('q')).toHaveTextContent('The x matters, see docs.');
    expect(item.textContent).not.toContain('*');
    openChip(1);
    expect(await screen.findAllByText(/The x matters, see docs\./)).toHaveLength(2);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(useReaderStore.getState().session?.ownerKey).toBe('owner-1'));
    expect(useReaderStore.getState().session?.citations[0]?.excerpt ?? '').toContain('**x**');
  });

  it('uses backend citation numbers and omits quotation marks for cites relations', async () => {
    const cited: SavedSourceCitation[] = [
      {
        number: 7,
        versionId: 'va',
        quote: 'paraphrased claim reference',
        support: {
          relation: 'cites',
          anchors: [{ exact: 'paraphrased claim reference', span: null }],
        },
      },
    ];
    renderWith(<SavedSourceCitations programId="program-1" citations={cited} quotes ownerKey="owner-1" label="Passages" />);

    const item = within(screen.getByRole('list', { name: 'Passages' })).getAllByRole('listitem')[0]!;
    expect(within(item).getByRole('button', { name: /Citation 7/ })).toBeInTheDocument();
    expect(item.querySelector('q')).toBeNull();
    await waitFor(() => expect(item).toHaveTextContent('paraphrased claim reference — Library alpha'));
    openChip(7);
    const card = await screen.findByRole('group', { name: 'Source 7, cited (not checked) in Library alpha' });
    expect(card).toHaveTextContent('Cited (not checked)');
    expect(card).toHaveTextContent('paraphrased claim reference');
    expect(card.textContent).not.toContain('“');
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(useReaderStore.getState().session?.citations[0]?.citationId).toBe(7));
    expect(useReaderStore.getState().session?.citations[0]?.anchoredBy?.[0]).toEqual({
      occurrence: 0,
      relation: 'cites',
      anchors: [{ exact: 'paraphrased claim reference', span: null }],
    });
  });

  it('clears preview byte spans so chips still render quoted excerpts while full spans and trails open in the reader', async () => {
    const exact = 'Exact sentence inside saved source.';
    const fullText = [
      '# Course Guide',
      '## Background',
      'Opening context longer than the cited excerpt so preview offsets exceed the preview length.',
      exact,
    ].join('\n\n');
    const start = new TextEncoder().encode(fullText.slice(0, fullText.indexOf(exact))).length;
    const end = start + new TextEncoder().encode(exact).length;
    mocks.version.mockResolvedValueOnce(
      ok({ sourceId: 'source-a', version: summary('va', 'Library alpha'), fullText, usage: [] }),
    );
    const withRemoteSpan: SavedSourceCitation[] = [
      {
        number: 4,
        versionId: 'va',
        quote: exact,
        support: {
          relation: 'quotes',
          anchors: [{ exact, span: { start, end } }],
        },
      },
    ];
    renderWith(<SavedSourceCitations programId="program-1" citations={withRemoteSpan} quotes ownerKey="owner-1" />);

    openChip(4);
    const card = await screen.findByRole('group', { name: 'Source 4, quoted from Library alpha' });
    expect(card).toHaveTextContent('Quoted from');
    expect(card).toHaveTextContent(`“${exact}”`);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(useReaderStore.getState().session?.citations[0]?.citationId).toBe(4));
    expect(anchoredCardDetails(useReaderStore.getState().session!.citations[0]!, null)).toMatchObject({
      relation: 'quotes',
      excerpts: [exact],
      trail: ['Course Guide', 'Background'],
    });
  });

  it('shows a quote that is all syntax as written rather than nothing', () => {
    renderWith(
      <SavedSourceCitations
        programId="program-1"
        citations={[
          {
            number: 1,
            versionId: 'va',
            quote: '**',
            support: { relation: 'quotes', anchors: [{ exact: '**', span: null }] },
          },
        ]}
        quotes
        ownerKey="owner-1"
        label="Passages"
      />,
    );
    expect(screen.getByRole('list', { name: 'Passages' }).querySelector('q')).toHaveTextContent('**');
  });

  it('ownerKey mode opens the docked reader for that owner at the chip index', async () => {
    renderWith(<SavedSourceCitations programId="program-1" citations={citations.slice(0, 2)} ownerKey="owner-1" />);
    openChip(2);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    await waitFor(() => expect(useReaderStore.getState().session?.ownerKey).toBe('owner-1'));
    expect(useReaderStore.getState().session?.index).toBe(1);
    expect(useReaderStore.getState().session?.citations).toHaveLength(2);
  });

  it('closes a reader it opened on unmount, but not one another owner opened', async () => {
    const mine = renderWith(<SavedSourceCitations programId="program-1" citations={citations.slice(0, 1)} ownerKey="owner-1" />);
    openChip(1);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(useReaderStore.getState().session?.ownerKey).toBe('owner-1'));
    mine.unmount();
    expect(useReaderStore.getState().session).toBeNull();

    const other = renderWith(<SavedSourceCitations programId="program-1" citations={citations.slice(0, 1)} ownerKey="owner-1" />);
    useReaderStore.getState().open('someone-else', [placeholderSource], 0);
    other.unmount();
    expect(useReaderStore.getState().session?.ownerKey).toBe('someone-else');
  });

  it('onOpen mode calls onOpen with the citation and index, and neither opens nor closes the reader', async () => {
    const onOpen = vi.fn();
    const view = renderWith(<SavedSourceCitations programId="program-1" citations={citations.slice(0, 2)} onOpen={onOpen} />);
    openChip(2);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith(citations[1], 1);
    expect(useReaderStore.getState().session).toBeNull();
    expect(mocks.version).not.toHaveBeenCalled();

    useReaderStore.getState().open('someone-else', [placeholderSource], 0);
    view.unmount();
    expect(useReaderStore.getState().session?.ownerKey).toBe('someone-else');
  });

  it('renders nothing in reading mode', () => {
    useCitationDisplayStore.setState({ visible: false });
    const { container } = renderWith(<SavedSourceCitations programId="program-1" citations={citations} ownerKey="owner-1" />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing without citations', () => {
    const { container } = renderWith(<SavedSourceCitations programId="program-1" citations={[]} ownerKey="owner-1" />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows the disabled reason instead of "View source"', async () => {
    renderWith(<SavedSourceCitations programId="program-1" citations={citations.slice(0, 1)} ownerKey="owner-1" disabledReason="Not available offline" />);
    openChip(1);
    expect(await screen.findByText('Not available offline')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument();
  });

  it('renders unnumbered entries explicitly without dropping them or fabricating citation numbers', async () => {
    const mixed: SavedSourceCitation[] = [
      { ...citations[0]!, number: 4 },
      { versionId: 'vb', quote: 'unnumbered legacy quote', number: undefined },
    ];
    const { rerenderWith } = renderWith(
      <SavedSourceCitations programId="program-1" citations={mixed} ownerKey="owner-1" label="Sources" />,
    );

    expect(screen.getAllByRole('button', { name: /Citation \d/ })).toHaveLength(1);
    expect(screen.getByRole('button', { name: /Citation 4/ })).toBeInTheDocument();
    expect(await screen.findByText('Library beta')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'View source' }));
    await waitFor(() => expect(useReaderStore.getState().session?.citations).toHaveLength(2));
    expect(useReaderStore.getState().session?.index).toBe(1);
    expect(useReaderStore.getState().session?.citations[1]?.citationId).toBeUndefined();

    rerenderWith(
      <SavedSourceCitations programId="program-1" citations={mixed} quotes ownerKey="owner-1" label="Passages" />,
    );
    expect(screen.getAllByRole('button', { name: /Citation \d/ })).toHaveLength(1);
    expect(screen.getByText('unnumbered legacy quote')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'View source' })).toBeInTheDocument();
  });

  it('requires exactly one of ownerKey or onOpen', () => {
    const noop = () => {};
    const both = (
      // @ts-expect-error ownerKey and onOpen are mutually exclusive
      <SavedSourceCitations programId="program-1" citations={[]} ownerKey="owner-1" onOpen={noop} />
    );
    const neither = (
      // @ts-expect-error one of ownerKey or onOpen is required
      <SavedSourceCitations programId="program-1" citations={[]} />
    );
    expect(both).toBeTruthy();
    expect(neither).toBeTruthy();
  });
});
