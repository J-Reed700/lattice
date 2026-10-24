import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelCatalogBrowser } from './ModelCatalogBrowser';

type Props = Record<string, any>;

const h = vi.hoisted(() => ({
  catalog: {} as Record<string, any>,
  store: {} as Record<string, any>,
  props: {} as Record<string, Props>,
}));

const { stub } = vi.hoisted(() => ({
  stub: (name: string) => (props: Props) => {
    h.props[name] = props;
    return <div data-testid={name} />;
  },
}));

vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => h.catalog }));
vi.mock('@/features/model/stores/modelCatalogStore', () => ({ useModelCatalogStore: () => h.store }));
vi.mock('./ModelDetailPanel', () => ({ ModelDetailPanel: stub('ModelDetailPanel') }));
vi.mock('./ModelFilterPanel', () => ({ ModelFilterPanel: stub('ModelFilterPanel') }));
vi.mock('./ModelListView', () => ({ ModelListView: stub('ModelListView') }));
vi.mock('./ModelSearchBar', () => ({ ModelSearchBar: stub('ModelSearchBar') }));
vi.mock('./SystemCapabilitiesCard', () => ({ SystemCapabilitiesCard: stub('SystemCapabilitiesCard') }));
vi.mock('./CatalogManagementSection', () => ({ CatalogManagementSection: stub('CatalogManagementSection') }));

const model = (id: string, overrides: Record<string, unknown> = {}) => ({
  id, name: id, category: 'LLM', size_gb: 4, minimum_ram_gb: 8, capabilities: ['chat'], performance_tier: 'Balanced',
  embedding_dimensions: null, default_filename: `${id}-Q4_K_M.gguf`, supported_quantizations: [], ...overrides,
});

const rec = (id: string, overrides: Record<string, unknown> = {}, extra: Record<string, unknown> = {}) => ({
  model: model(id, overrides), ranking_score: 50, popularity_downloads: 100, popularity_likes: 10, ...extra,
});

const noFilters = { category: null, max_size_gb: null, min_downloads: null, required_capabilities: [], embedding_dimensions: null };

const listed = () => (h.props.ModelListView.models as Array<{ model: { id: string } }>).map((entry) => entry.model.id);

describe('ModelCatalogBrowser', () => {
  beforeEach(() => {
    h.props = {};
    h.catalog = {
      selectedModel: null,
      systemCapabilities: { total_ram_gb: 15.6, gpu_acceleration: 'Metal', gpu_type: 'AppleSilicon', vram_gb: null, available_disk_gb: 500 },
      compatibleModels: [
        rec('basil', { size_gb: 2 }, { ranking_score: 60, popularity_downloads: 10, popularity_likes: 5 }),
        rec('anise', { size_gb: 9 }, { ranking_score: 40, popularity_downloads: 1000, popularity_likes: 1 }),
        rec('cumin', { size_gb: 0 }, { ranking_score: 80, popularity_downloads: null, popularity_likes: 50 }),
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

  it('opens on the overview of purposes with this machine summarised', () => {
    render(<ModelCatalogBrowser />);
    expect(screen.getByRole('heading', { name: 'Explore by purpose' })).toBeInTheDocument();
    expect(screen.getByText('Choose a purpose, then compare models and versions.')).toBeInTheDocument();
    expect(screen.getByText('16 GB memory · Metal')).toBeInTheDocument();
    expect(h.props.ModelListView).toMatchObject({ overview: true, page: 1, filtersActive: false, loading: false, error: null });
    expect(h.props.ModelFilterPanel.quantizations).toEqual(['Q4_K_M']);
  });

  it('describes the hardware generically before it is known', () => {
    h.catalog.systemCapabilities = null;
    render(<ModelCatalogBrowser />);
    expect(screen.getByText('Hardware and available memory')).toBeInTheDocument();
  });

  it('toggles between categories and every model', () => {
    render(<ModelCatalogBrowser />);
    fireEvent.click(screen.getByRole('button', { name: 'Browse all models' }));
    expect(screen.getByRole('heading', { name: 'Browse models' })).toBeInTheDocument();
    expect(h.props.ModelListView.overview).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Explore categories' }));
    expect(screen.getByRole('heading', { name: 'Explore by purpose' })).toBeInTheDocument();
  });

  it.each([
    ['popularity', ['anise', 'basil', 'cumin']],
    ['likes', ['cumin', 'basil', 'anise']],
    ['name', ['anise', 'basil', 'cumin']],
    ['size_asc', ['basil', 'anise', 'cumin']],
    ['size_desc', ['anise', 'basil', 'cumin']],
    ['recommended', ['cumin', 'basil', 'anise']],
  ])('sorts by %s', (sortBy, order) => {
    h.catalog.sortBy = sortBy;
    render(<ModelCatalogBrowser />);
    expect(listed()).toEqual(order);
  });

  it('breaks popularity ties by ranking score', () => {
    h.catalog.compatibleModels = [rec('low', {}, { ranking_score: 1 }), rec('high', {}, { ranking_score: 9 })];
    h.catalog.sortBy = 'likes';
    render(<ModelCatalogBrowser />);
    expect(listed()).toEqual(['high', 'low']);
  });

  it('filters by category, size, downloads, capability and embedding width', () => {
    h.catalog.compatibleModels = [
      rec('chat-small', { size_gb: 3, capabilities: ['chat', 'vision'] }, { popularity_downloads: 500 }),
      rec('chat-big', { size_gb: 30, capabilities: ['chat'] }, { popularity_downloads: 500 }),
      rec('unsized', { size_gb: 0, capabilities: [], performance_tier: 'Vision' }, { popularity_downloads: 900 }),
      rec('rare', { size_gb: 2, capabilities: ['vision'] }, { popularity_downloads: 1 }),
    ];
    h.catalog.filters = { ...noFilters, category: 'LLM', max_size_gb: 8, min_downloads: 100, required_capabilities: ['vision'] };
    render(<ModelCatalogBrowser />);
    expect(listed()).toEqual(['unsized', 'chat-small']);
    expect(screen.getByRole('heading', { name: 'Browse models' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Browse all models' })).not.toBeInTheDocument();
    expect(h.props.ModelListView.filtersActive).toBe(true);
  });

  it('filters embeddings by width, keeping chat models and unknown widths', () => {
    h.catalog.compatibleModels = [
      rec('bge', { category: 'Embedding', embedding_dimensions: 384 }),
      rec('e5', { category: 'Embedding', embedding_dimensions: 768 }),
      rec('mystery', { category: 'Embedding', embedding_dimensions: null }),
      rec('chatty'),
    ];
    h.catalog.filters = { ...noFilters, embedding_dimensions: 768 };
    render(<ModelCatalogBrowser />);
    expect(listed().sort()).toEqual(['chatty', 'e5', 'mystery']);
  });

  it('filters by precision and by fit', () => {
    h.catalog.compatibleModels = [
      rec('small', { default_filename: 'small-Q4_K_M.gguf', minimum_ram_gb: 4 }),
      rec('tight', { default_filename: 'tight-Q8_0.gguf', minimum_ram_gb: 10.5 }),
      rec('huge', { default_filename: 'huge-Q8_0.gguf', minimum_ram_gb: 64 }),
    ];
    h.store = { ...h.store, quantizationFilter: 'Q8_0' };
    const first = render(<ModelCatalogBrowser />);
    expect(listed().sort()).toEqual(['huge', 'tight']);
    first.unmount();
    h.store = { ...h.store, quantizationFilter: null, fitFilter: 'fits' };
    const second = render(<ModelCatalogBrowser />);
    expect(listed()).toEqual(['small']);
    second.unmount();
    h.store = { ...h.store, fitFilter: 'fits-or-tight' };
    render(<ModelCatalogBrowser />);
    expect(listed().sort()).toEqual(['small', 'tight']);
  });

  it('shows search results scored by relevance', () => {
    h.catalog.searchQuery = 'thyme';
    h.catalog.searchLoading = true;
    h.catalog.searchError = 'rate limited';
    h.catalog.searchResults = [
      { model: model('thyme-a'), relevance_score: 0.4, popularity_downloads: 5 },
      { model: model('thyme-b'), relevance_score: 0.9 },
    ];
    h.catalog.sortBy = 'recommended';
    render(<ModelCatalogBrowser />);
    expect(screen.getByRole('heading', { name: 'Search results' })).toBeInTheDocument();
    expect(listed()).toEqual(['thyme-a', 'thyme-b']);
    expect(h.props.ModelListView).toMatchObject({ loading: true, error: 'rate limited', overview: false });
    expect(h.props.ModelListView.models[1]).toMatchObject({ ranking_score: 0.9, popularity_downloads: null, popularity_likes: null });
    expect(screen.queryByRole('button', { name: 'Browse all models' })).not.toBeInTheDocument();
  });

  it('retries the search or the catalog, whichever is showing', () => {
    const first = render(<ModelCatalogBrowser />);
    h.props.ModelListView.onRetry();
    expect(h.catalog.loadCompatibleModels).toHaveBeenCalled();
    first.unmount();
    h.catalog.searchQuery = 'thyme';
    render(<ModelCatalogBrowser />);
    h.props.ModelListView.onRetry();
    expect(h.catalog.reloadSearch).toHaveBeenCalled();
  });

  it('swallows a failed retry', async () => {
    h.catalog.loadCompatibleModels.mockRejectedValueOnce(new Error('offline'));
    render(<ModelCatalogBrowser />);
    await act(async () => { h.props.ModelListView.onRetry(); });
    expect(h.catalog.loadCompatibleModels).toHaveBeenCalled();
  });

  it('pages within one result set, and starts over when the set changes', () => {
    const view = render(<ModelCatalogBrowser />);
    act(() => h.props.ModelListView.onPageChange(3));
    expect(h.props.ModelListView.page).toBe(3);
    h.catalog = { ...h.catalog, sortBy: 'name' };
    view.rerender(<ModelCatalogBrowser />);
    expect(h.props.ModelListView.page).toBe(1);
  });

  it('wires category browsing, filter reset and selection', () => {
    render(<ModelCatalogBrowser />);
    h.props.ModelListView.onBrowseCategory('Embedding');
    expect(h.catalog.setFilters).toHaveBeenCalledWith({ category: 'Embedding' });
    expect(h.props.ModelListView.onResetFilters).toBe(h.store.resetFilters);
    expect(h.props.ModelListView.onModelSelect).toBe(h.catalog.selectModel);
  });

  it('jumps to the Hugging Face token field', () => {
    const field = document.createElement('input');
    field.id = 'hf-token';
    field.scrollIntoView = vi.fn();
    document.body.appendChild(field);
    render(<ModelCatalogBrowser />);
    act(() => h.props.ModelListView.onAddToken());
    expect(field.scrollIntoView).toHaveBeenCalledWith({ block: 'center', behavior: 'smooth' });
    expect(field).toHaveFocus();
    field.remove();
    // Without the field it does nothing.
    act(() => h.props.ModelListView.onAddToken());
  });

  it('shows one model in detail once selected', () => {
    const selected = rec('basil');
    h.catalog.selectedModel = selected;
    render(<ModelCatalogBrowser routerModelId="basil" onSetRouterModel={vi.fn()} />);
    expect(screen.getByTestId('ModelDetailPanel')).toBeInTheDocument();
    expect(screen.queryByTestId('ModelListView')).not.toBeInTheDocument();
    expect(h.props.ModelDetailPanel).toMatchObject({ model: selected, onBack: h.catalog.clearSelection, routerModelId: 'basil' });
  });
});
