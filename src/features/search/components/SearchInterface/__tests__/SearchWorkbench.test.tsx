import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { SearchResult } from '@/types';

import { SearchInterface } from '../SearchInterface';

const h = vi.hoisted(() => ({
  searchHybrid: vi.fn(),
  searchDocuments: vi.fn(),
  openFileById: vi.fn(),
  openFile: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  VaultAPI: {
    searchHybrid: h.searchHybrid,
    searchDocuments: h.searchDocuments,
    openFileById: h.openFileById,
    openFile: h.openFile,
  },
  default: {
    searchHybrid: h.searchHybrid,
    searchDocuments: h.searchDocuments,
    openFileById: h.openFileById,
    openFile: h.openFile,
  },
}));

vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: () => null,
}));

const FIXTURE_RESULTS: SearchResult[] = [
  {
    id: 'chunk-plants',
    documentId: 'doc-plants',
    title: 'Plant biology notes.md',
    path: '/Users/test/Notes/Plant biology notes.md',
    content: 'Photosynthesis converts sunlight into chemical energy in chloroplasts.',
    score: 0.88,
    position: null,
    vectorScore: 0.84,
    bm25Score: 9.2,
    vectorRank: 0,
    bm25Rank: 0,
    metadata: {},
    highlights: [],
  },
  {
    id: 'chunk-botany',
    documentId: 'doc-botany',
    title: 'History of botany.pdf',
    path: '/Users/test/Archive/History of botany.pdf',
    content: 'Jan Ingenhousz discovered sunlight drives oxygen release in green plants.',
    score: 0.65,
    position: null,
    vectorScore: 0.61,
    bm25Score: 4.5,
    vectorRank: 1,
    bm25Rank: 1,
    metadata: {},
    highlights: [],
  },
  {
    id: 'chunk-web',
    documentId: 'doc-web',
    title: 'Light reactions overview',
    path: 'https://botany.example.org/light-reactions',
    content: 'Chlorophyll pigments absorb red and blue wavelengths during light reactions.',
    score: 0.76,
    position: null,
    vectorScore: 0.72,
    bm25Score: null,
    vectorRank: 2,
    bm25Rank: null,
    metadata: {},
    highlights: [],
  },
];

function LocationEcho() {
  const location = useLocation();
  return <div data-testid="location-echo">{`${location.pathname}${location.search}`}</div>;
}

function mockWideScreen(matches: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

function renderWorkbench(initialEntry = '/search?q=photosynthesis') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <LocationEcho />
        <SearchInterface />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('SearchInterface workbench facets and quick preview', () => {
  const originalMatchMedia = window.matchMedia;

  beforeEach(() => {
    vi.clearAllMocks();
    h.searchHybrid.mockResolvedValue({ ok: true, data: FIXTURE_RESULTS });
    h.searchDocuments.mockResolvedValue({ ok: true, data: [] });
    h.openFileById.mockResolvedValue({ ok: true, data: { action: 'open_external' } });
  });

  afterEach(() => {
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      configurable: true,
      value: originalMatchMedia,
    });
  });

  it('derives live Type and Source filter chips and genuinely filters results', async () => {
    mockWideScreen(false);
    renderWorkbench();

    await screen.findByText('Plant biology notes.md');
    expect(screen.getByText('3 results')).toBeInTheDocument();

    const facetBar = screen.getByRole('group', { name: 'Filter search results' });
    const pdfChip = within(facetBar).getByRole('button', { name: /PDF/ });
    fireEvent.click(pdfChip);

    expect(screen.getByText('1 of 3 results')).toBeInTheDocument();
    expect(screen.getByText('History of botany.pdf')).toBeInTheDocument();
    expect(screen.queryByText('Plant biology notes.md')).not.toBeInTheDocument();
    expect(screen.queryByText('Light reactions overview')).not.toBeInTheDocument();

    // Combining with a mismatched folder shows an actionable reset state.
    const notesChip = within(facetBar).getByRole('button', { name: /Notes/ });
    fireEvent.click(notesChip);
    expect(screen.getByText('No results match the selected filters.')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Reset filters' }));
    expect(screen.getByText('3 results')).toBeInTheDocument();
    expect(screen.getByText('Plant biology notes.md')).toBeInTheDocument();
  });

  it('shows the right-hand Quick Preview pane on wide screens with Open full reader and Ask in Chat', async () => {
    mockWideScreen(true);
    renderWorkbench();

    const preview = await screen.findByRole('complementary', { name: 'Quick preview' });
    expect(within(preview).getByRole('heading', { name: 'Plant biology notes.md' })).toBeInTheDocument();
    expect(preview).toHaveTextContent(/Photosynthesis converts sunlight/);

    // Hovering / inspecting another result updates the quick preview without opening modals.
    const inspectButtons = screen.getAllByRole('button', { name: 'Inspect in quick preview' });
    fireEvent.click(inspectButtons[2]);
    expect(within(preview).getByRole('heading', { name: 'History of botany.pdf' })).toBeInTheDocument();

    // Open full reader triggers the document open handler.
    fireEvent.click(within(preview).getByRole('button', { name: 'Open full reader' }));
    await waitFor(() => {
      expect(h.openFileById).toHaveBeenCalledWith('doc-botany');
    });

    // Ask in Chat routes to /chat with the selected documentId.
    fireEvent.click(within(preview).getByRole('button', { name: 'Ask in Chat' }));
    expect(screen.getByTestId('location-echo')).toHaveTextContent('/chat?new=1&documentId=doc-botany');
  });
});
