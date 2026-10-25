import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';


import VaultAPI from '@/lib/api';
import { deferred } from '@/tests/deferred';
import type { ModelRecommendation } from '@/types/modelCatalog';

import { ModelListView } from './ModelListView';

const h = vi.hoisted(() => ({
  downloadedModels: [] as Array<{ id: string; model_id?: string }>,
  tokenStatus: undefined as undefined | { isSet: boolean },
  capabilities: { total_ram_gb: 16 },
  startModelDownload: vi.fn(),
  addToast: vi.fn(),
  rows: {} as Record<string, Record<string, any>>,
}));

vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: h.downloadedModels }) }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ getActiveDownloadForModel: (id: string) => (id === 'busy' ? { state: 'Downloading' } : undefined) }),
}));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => ({ systemCapabilities: h.capabilities }) }));
vi.mock('@/features/settings/hooks/startModelDownload', () => ({ startModelDownload: h.startModelDownload }));
vi.mock('@/features/settings/hooks/useHuggingFaceTokenQuery', () => ({ useHuggingFaceTokenStatusQuery: () => ({ data: h.tokenStatus }) }));
vi.mock('@/stores/toastStore', () => ({ useToastStore: (selector: (state: unknown) => unknown) => selector({ addToast: h.addToast }) }));
vi.mock('./ModelRow', () => ({
  ModelRow: (props: Record<string, any>) => {
    h.rows[props.model.model.id] = props;
    return (
      <div>
        <button type="button" onClick={props.onSelect}>Open {props.model.model.id}</button>
        <button type="button" onClick={props.onDownload}>Download {props.model.model.id}</button>
      </div>
    );
  },
}));

const rec = (id: string, modelId?: string) => ({ model: { id, name: id, category: 'LLM', model_id: modelId } }) as unknown as ModelRecommendation;

const queryClient = new QueryClient();
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
const nav = { overview: false, page: 1, onPageChange: vi.fn(), onBrowseCategory: vi.fn() };

describe('ModelListView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.rows = {};
    h.downloadedModels = [];
    h.tokenStatus = undefined;
    h.startModelDownload.mockResolvedValue({ alreadyDownloaded: false });
  });

  it('shows placeholders while loading', () => {
    const { container } = render(<ModelListView models={[]} loading onModelSelect={vi.fn()} {...nav} />, { wrapper });
    expect(screen.queryByText('No models match.')).not.toBeInTheDocument();
    expect(container.firstElementChild?.children).toHaveLength(5);
  });

  it('explains a failed load and retries', () => {
    const onRetry = vi.fn();
    const first = render(<ModelListView models={[]} loading={false} error="catalog offline" onRetry={onRetry} onModelSelect={vi.fn()} {...nav} />, { wrapper });
    expect(screen.getByText("Couldn't load the catalog. catalog offline")).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalled();
    first.unmount();
    render(<ModelListView models={[]} loading={false} error="catalog offline" onModelSelect={vi.fn()} {...nav} />, { wrapper });
    expect(screen.queryByRole('button', { name: 'Retry' })).not.toBeInTheDocument();
  });

  it('offers to reset filters only when filters are why nothing matches', () => {
    const onResetFilters = vi.fn();
    const first = render(<ModelListView models={[]} loading={false} filtersActive onResetFilters={onResetFilters} onModelSelect={vi.fn()} {...nav} />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: 'Reset filters' }));
    expect(onResetFilters).toHaveBeenCalled();
    first.unmount();
    render(<ModelListView models={[]} loading={false} onModelSelect={vi.fn()} {...nav} />, { wrapper });
    expect(screen.getByText('No models match.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reset filters' })).not.toBeInTheDocument();
  });

  it('tells each row what is downloaded, downloading and gated', () => {
    h.downloadedModels = [{ id: 'local-a' }, { id: 'x', model_id: 'repo/b' }];
    h.tokenStatus = { isSet: true };
    const onAddToken = vi.fn();
    render(<ModelListView models={[rec('local-a'), rec('repo/b'), rec('busy'), rec('fresh')]} loading={false} onAddToken={onAddToken} onModelSelect={vi.fn()} {...nav} overview={false} />, { wrapper });
    expect(h.rows['local-a']).toMatchObject({ isDownloaded: true, compact: false, hasHfToken: true, onAddToken, capabilities: h.capabilities });
    expect(h.rows['repo/b'].isDownloaded).toBe(true);
    expect(h.rows.busy.activeDownload).toEqual({ state: 'Downloading' });
    expect(h.rows.fresh).toMatchObject({ isDownloaded: false, isStarting: false, activeDownload: undefined });
  });

  it('opens a model', () => {
    const onModelSelect = vi.fn();
    const model = rec('fresh');
    render(<ModelListView models={[model]} loading={false} onModelSelect={onModelSelect} {...nav} />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: 'Open fresh' }));
    expect(onModelSelect).toHaveBeenCalledWith(model);
    expect(h.rows.fresh.hasHfToken).toBe(false);
  });

  it('starts one download per model at a time', async () => {
    const pending = deferred<{ alreadyDownloaded: boolean }>();
    h.startModelDownload.mockReturnValueOnce(pending.promise);
    const model = rec('fresh');
    render(<ModelListView models={[model]} loading={false} onModelSelect={vi.fn()} {...nav} />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: 'Download fresh' }));
    expect(h.rows.fresh.isStarting).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Download fresh' }));
    expect(h.startModelDownload).toHaveBeenCalledTimes(1);
    expect(h.startModelDownload).toHaveBeenCalledWith({ metadata: model.model, addToast: h.addToast, queryClient }, VaultAPI);
    await act(async () => { pending.resolve({ alreadyDownloaded: false }); await pending.promise; });
    expect(h.rows.fresh.isStarting).toBe(false);
  });
});
