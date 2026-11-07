import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { createTestApi } from '@/tests/apiClient';
import { deferred } from '@/tests/deferred';
import type { ModelRecommendation } from '@/types/modelCatalog';

import { ModelListView } from './ModelListView';

vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({ useDownloadState: () => ({ getActiveDownloadForModel: () => undefined }) }));
vi.mock('@/features/settings/hooks/startModelDownload', () => ({ startModelDownload: vi.fn() }));
vi.mock('@/features/settings/hooks/useHuggingFaceTokenQuery', () => ({ useHuggingFaceTokenStatusQuery: () => ({ data: undefined }) }));
vi.mock('@/stores/toastStore', () => ({ useToastStore: (selector: (state: unknown) => unknown) => selector({ addToast: vi.fn() }) }));

const GB = 1_073_741_824;
const device = { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated' as const, totalBytes: 16 * GB };
const fits = { kind: 'fits' as const, needsBytes: 11.2 * GB, budgetBytes: 15 * GB, device };
const tight = { kind: 'tight' as const, needsBytes: 15 * GB, budgetBytes: 15 * GB, device };

const row = (id: string, fit: unknown, repo = `pub/${id}-GGUF`) => ({
  model: {
    id, name: `Model ${id}`, category: 'LLM', description: '', size_gb: 4,
    capabilities: ['chat'], supported_quantizations: ['Q4_K_M'], model_id: repo, default_filename: `${id}-Q4_K_M.gguf`,
    requires_auth: false, files: [], license: 'MIT', embedding_dimensions: null, embedding_compatibility: null,
  },
  compatibility: null,
  ranking_score: 50,
  popularity_downloads: 10,
  popularity_likes: 1,
  fit,
}) as unknown as ModelRecommendation;

const nav = { overview: false, page: 1, onPageChange: vi.fn(), onBrowseCategory: vi.fn() };

function renderList(models: ModelRecommendation[], readModelFits: ReturnType<typeof vi.fn>) {
  const api = createTestApi({ readModelFits: readModelFits as never });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ApiProvider client={api}><QueryClientProvider client={client}>{children}</QueryClientProvider></ApiProvider>
  );
  return render(<ModelListView models={models} loading={false} onModelSelect={vi.fn()} {...nav} />, { wrapper });
}

const rowOf = (name: string) => screen.getByText(name).closest('.model-catalog-row') as HTMLElement;

describe('ModelListView rows that say Checking size', () => {
  beforeEach(() => vi.clearAllMocks());

  it('asks once, for exactly the rows with no verdict, and says Checking size meanwhile', async () => {
    const pending = deferred<{ ok: true; data: unknown[] }>();
    const readModelFits = vi.fn().mockReturnValue(pending.promise);
    renderList([row('alpha', null), row('beta', fits), row('gamma', null)], readModelFits);
    await waitFor(() => expect(readModelFits).toHaveBeenCalledTimes(1));
    expect(readModelFits).toHaveBeenCalledWith([
      expect.objectContaining({ repo: 'pub/alpha-GGUF', file: 'alpha-Q4_K_M.gguf' }),
      expect.objectContaining({ repo: 'pub/gamma-GGUF', file: 'gamma-Q4_K_M.gguf' }),
    ]);
    expect(rowOf('Model alpha')).toHaveTextContent('Checking size');
    expect(rowOf('Model gamma')).toHaveTextContent('Checking size');
    expect(rowOf('Model beta')).toHaveTextContent('Fits');
  });

  it('shows each answer in its row: the verdict with its device in the tooltip, or Size unknown', async () => {
    const readModelFits = vi.fn().mockResolvedValue({ ok: true, data: [tight, null] });
    renderList([row('alpha', null), row('gamma', null)], readModelFits);
    await waitFor(() => expect(rowOf('Model alpha')).toHaveTextContent('Fits with a shorter context'));
    expect(screen.getByText('Fits with a shorter context').closest('[title]')?.getAttribute('title')).toContain('NVIDIA GeForce RTX 4080');
    expect(rowOf('Model gamma')).toHaveTextContent('Size unknown');
    expect(rowOf('Model gamma')).not.toHaveTextContent('Checking size');
  });

  it('never asks when every row arrived with its verdict', async () => {
    const readModelFits = vi.fn();
    renderList([row('alpha', fits), row('beta', tight)], readModelFits);
    expect(await screen.findByText('Model alpha')).toBeInTheDocument();
    expect(readModelFits).not.toHaveBeenCalled();
  });

  it('shows no memory estimate beside the verdict', async () => {
    const readModelFits = vi.fn().mockResolvedValue({ ok: true, data: [fits] });
    renderList([row('alpha', null)], readModelFits);
    await waitFor(() => expect(rowOf('Model alpha')).toHaveTextContent('Fits'));
    expect(rowOf('Model alpha')).not.toHaveTextContent('Est. memory');
  });
});
