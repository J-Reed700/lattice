import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelCatalogBrowser } from './ModelCatalogBrowser';

type Props = Record<string, any>;

const h = vi.hoisted(() => ({
  catalog: {} as Record<string, any>,
  store: {} as Record<string, any>,
  props: {} as Record<string, Props>,
  api: { recommendChatModels: vi.fn() },
}));

const { stub } = vi.hoisted(() => ({
  stub: (name: string) => (props: Props) => {
    h.props[name] = props;
    return <div data-testid={name} />;
  },
}));

vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => h.catalog }));
vi.mock('@/features/model/stores/modelCatalogStore', () => ({ useModelCatalogStore: () => h.store }));
vi.mock('./ModelDetailPanel', () => ({ ModelDetailPanel: stub('ModelDetailPanel') }));
vi.mock('./ModelFilterPanel', () => ({ ModelFilterPanel: stub('ModelFilterPanel') }));
vi.mock('./ModelListView', () => ({ ModelListView: stub('ModelListView') }));
vi.mock('./ModelSearchBar', () => ({ ModelSearchBar: stub('ModelSearchBar') }));
vi.mock('./SystemCapabilitiesCard', () => ({ SystemCapabilitiesCard: stub('SystemCapabilitiesCard') }));
vi.mock('./CatalogManagementSection', () => ({ CatalogManagementSection: stub('CatalogManagementSection') }));

const GB = 1_073_741_824;
const RTX = { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated' };

const verdict = (kind: 'fits' | 'tight' | 'too_large') => ({ kind, needsBytes: 11.2 * GB, budgetBytes: 15 * GB, device: RTX });

const recommendation = (modelId: string, name: string) => ({
  modelId,
  name,
  description: `${name} description.`,
  downloadBytes: 5.7 * GB,
  verdict: verdict('fits'),
  labels: [],
  evidence: { kind: 'untested' },
  speed: 'balanced',
  reason: 'Fits.',
});

const model = (id: string, overrides: Record<string, unknown> = {}) => ({
  id, name: id, category: 'LLM', size_gb: 4, minimum_ram_gb: 8, capabilities: ['chat'], performance_tier: 'Balanced',
  embedding_dimensions: null, default_filename: `${id}-Q4_K_M.gguf`, supported_quantizations: [], ...overrides,
});

const rec = (id: string, extra: Record<string, unknown> = {}) => ({
  model: model(id), ranking_score: 50, popularity_downloads: 100, popularity_likes: 10, ...extra,
});

const noFilters = { category: null, max_size_gb: null, min_downloads: null, required_capabilities: [], embedding_dimensions: null };
const listed = () => (h.props.ModelListView?.models as Array<{ model: { id: string } }> | undefined)?.map((entry) => entry.model.id);

function renderBrowser() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<ModelCatalogBrowser />, { wrapper });
}

describe('ModelCatalogBrowser recommended for this computer', () => {
  beforeEach(() => {
    h.props = {};
    h.api.recommendChatModels.mockReset().mockResolvedValue({
      ok: true,
      data: [recommendation('basil', 'Basil 9B'), recommendation('anise', 'Anise 4B')],
    });
    h.catalog = {
      selectedModel: null,
      // Roomy on purpose: the old frontend rule would call every 8 GB model a fit.
      systemCapabilities: { total_ram_gb: 64, gpu_acceleration: 'None', gpu_type: 'None', vram_gb: null, available_disk_gb: 500 },
      compatibleModels: [
        rec('basil', { ranking_score: 60, popularity_downloads: 10 }),
        rec('anise', { ranking_score: 40, popularity_downloads: 1000 }),
        rec('cumin', { ranking_score: 80, popularity_downloads: null }),
      ],
      searchResults: [],
      compatibleModelsLoading: false,
      compatibleModelsError: null,
      searchLoading: false,
      searchError: null,
      reloadSearch: vi.fn().mockResolvedValue(undefined),
      searchQuery: '',
      filters: noFilters,
      sortBy: 'popularity',
      setFilters: vi.fn(),
      selectModel: vi.fn(),
      clearSelection: vi.fn(),
      loadCompatibleModels: vi.fn().mockResolvedValue(undefined),
    };
    h.store = { quantizationFilter: null, fitFilter: 'all', resetFilters: vi.fn() };
  });

  it('opens with a Recommended for this computer section listing the recommender’s models in order', async () => {
    renderBrowser();
    const section = await screen.findByRole('region', { name: 'Recommended for this computer' });
    const text = section.textContent ?? '';
    expect(text).toContain('Basil 9B');
    expect(text).toContain('Anise 4B');
    expect(text.indexOf('Basil 9B')).toBeLessThan(text.indexOf('Anise 4B'));
    expect(within(section).queryByText('cumin')).not.toBeInTheDocument();
  });

  it('puts that section above the rest of the catalog', async () => {
    renderBrowser();
    const section = await screen.findByRole('region', { name: 'Recommended for this computer' });
    const purposes = screen.getByRole('heading', { name: 'Explore by purpose' });
    expect(section.compareDocumentPosition(purposes) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(section.compareDocumentPosition(screen.getByTestId('ModelListView')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('sorts Recommended in the recommender’s order, the rest after', async () => {
    h.catalog.sortBy = 'recommended';
    h.api.recommendChatModels.mockResolvedValue({
      ok: true,
      data: [recommendation('anise', 'Anise 4B'), recommendation('basil', 'Basil 9B')],
    });
    renderBrowser();
    await waitFor(() => expect(listed()).toEqual(['anise', 'basil', 'cumin']));
  });

  it('follows the recommender when it reverses its order', async () => {
    h.catalog.sortBy = 'recommended';
    h.api.recommendChatModels.mockResolvedValue({
      ok: true,
      data: [recommendation('basil', 'Basil 9B'), recommendation('anise', 'Anise 4B')],
    });
    renderBrowser();
    await waitFor(() => expect(listed()).toEqual(['basil', 'anise', 'cumin']));
  });

  it('filters by fit on the backend verdict, not on a figure the frontend computes', async () => {
    h.catalog.sortBy = 'name';
    h.catalog.compatibleModels = [
      rec('a-huge', { fit: verdict('too_large') }),
      rec('b-fits', { fit: verdict('fits') }),
      rec('c-unread', { fit: null }),
      rec('d-tight', { fit: verdict('tight') }),
    ];
    h.store = { ...h.store, fitFilter: 'fits' };
    const first = renderBrowser();
    await screen.findByRole('region', { name: 'Recommended for this computer' });
    expect(listed()).toEqual(['b-fits']);
    first.unmount();
    h.store = { ...h.store, fitFilter: 'fits-or-tight' };
    renderBrowser();
    await screen.findByRole('region', { name: 'Recommended for this computer' });
    expect(listed()).toEqual(['b-fits', 'd-tight']);
  });
});
