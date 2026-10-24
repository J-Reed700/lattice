import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';
import type { ModelMetadata, ModelRecommendation } from '@/types/modelCatalog';

import { ModelDetailPanel } from './ModelDetailPanel';

const h = vi.hoisted(() => ({
  capabilities: null as null | Record<string, unknown>,
  hfToken: undefined as undefined | { isSet: boolean },
  downloaded: {
    isModelDownloaded: vi.fn(),
    setActiveChatModel: vi.fn(),
    setActiveEmbeddingModel: vi.fn(),
    warmUpActiveChatModel: vi.fn(),
  },
  activeDownload: undefined as undefined | { state: string; percentage?: number },
  hasActiveDownload: false,
  settings: undefined as undefined | { llm: { provider: string } },
  updateSettings: vi.fn(),
  addToast: vi.fn(),
  startModelDownload: vi.fn(),
  pickerProps: null as null | Record<string, any>,
}));

vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => ({ systemCapabilities: h.capabilities }) }));
vi.mock('@/features/settings/hooks/useHuggingFaceTokenQuery', () => ({ useHuggingFaceTokenStatusQuery: () => ({ data: h.hfToken }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => h.downloaded }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({
    getActiveDownloadForModel: () => h.activeDownload,
    hasActiveDownloadForModel: () => h.hasActiveDownload,
  }),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: h.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: h.updateSettings }),
}));
vi.mock('@/stores/toastStore', () => ({
  useToastStore: (selector: (state: unknown) => unknown) => selector({ addToast: h.addToast }),
}));
vi.mock('@/features/settings/hooks/startModelDownload', () => ({ startModelDownload: h.startModelDownload }));
vi.mock('./ModelVariantPicker', () => ({
  ModelVariantPicker: (props: Record<string, any>) => {
    h.pickerProps = props;
    return <button type="button" onClick={() => props.onSelect({ ...props.model, id: 'variant-q8', default_filename: 'saga-7b-Q8_0.gguf', size_gb: 7.7 })}>Pick Q8</button>;
  },
}));

const metadata = (overrides: Partial<ModelMetadata> = {}): ModelMetadata => ({
  id: 'saga-7b-q4',
  name: 'Saga 7B',
  category: 'LLM',
  description: 'A storytelling model.',
  size_gb: 4.4,
  minimum_ram_gb: 8,
  recommended_ram_gb: 16,
  context_length: 32768,
  performance_tier: 'Balanced',
  supported_quantizations: ['Q4_K_M'],
  capabilities: ['chat', 'tools'],
  download_url: null,
  license: 'Apache-2.0',
  requires_auth: false,
  model_id: 'saga/Saga-7B-GGUF',
  default_filename: 'saga-7b-Q4_K_M.gguf',
  files: [],
  total_size_bytes: 4_724_464_025,
  embedding_dimensions: null,
  embedding_compatibility: null,
  ...overrides,
} as ModelMetadata);

const recommendation = (overrides: Partial<ModelMetadata> = {}, extra: Partial<ModelRecommendation> = {}): ModelRecommendation => ({
  model: metadata(overrides),
  compatibility: null,
  ranking_score: 90,
  popularity_downloads: 12_000,
  popularity_likes: 310,
  ...extra,
} as unknown as ModelRecommendation);

let queryClient: QueryClient;
const renderPanel = (model: ModelRecommendation, props: Record<string, unknown> = {}) => {
  const onBack = vi.fn();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  render(<ModelDetailPanel model={model} onBack={onBack} {...props} />, { wrapper });
  return { onBack };
};

const toastTitles = () => h.addToast.mock.calls.map(([toast]) => toast.title);

describe('ModelDetailPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient();
    h.capabilities = { total_ram_gb: 32, vram_gb: null, gpu_type: 'AppleSilicon', gpu_acceleration: 'Metal', available_disk_gb: 500 };
    h.hfToken = undefined;
    h.activeDownload = undefined;
    h.hasActiveDownload = false;
    h.settings = { llm: { provider: 'local' } };
    h.downloaded.isModelDownloaded.mockResolvedValue(false);
    h.downloaded.setActiveChatModel.mockResolvedValue(undefined);
    h.downloaded.setActiveEmbeddingModel.mockResolvedValue(undefined);
    h.downloaded.warmUpActiveChatModel.mockResolvedValue(undefined);
    h.startModelDownload.mockResolvedValue({ alreadyDownloaded: false });
    h.pickerProps = null;
  });

  it('describes the model, its fit and its specifications', async () => {
    const { onBack } = renderPanel(recommendation());
    expect(screen.getByRole('heading', { name: 'Saga 7B' })).toBeInTheDocument();
    expect(screen.getByText('saga/Saga-7B-GGUF')).toBeInTheDocument();
    expect(screen.getByText('A storytelling model.')).toBeInTheDocument();
    expect(screen.getByText('saga-7b-Q4_K_M.gguf')).toBeInTheDocument();
    expect(screen.getByText('Chat · Q4_K_M · 4.4 GB')).toBeInTheDocument();
    expect(screen.getByText('Checking…')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Download' })).toBeInTheDocument();
    expect(screen.getByText('8.0 GB')).toBeInTheDocument();
    expect(screen.getByText('16.0 GB')).toBeInTheDocument();
    expect(screen.getByText(`${(32768).toLocaleString()} tokens`)).toBeInTheDocument();
    expect(screen.getByText('GGUF')).toBeInTheDocument();
    expect(screen.getByText('Apache-2.0')).toBeInTheDocument();
    expect(screen.getByText('chat, tools')).toBeInTheDocument();
    expect(screen.getByText(new Intl.NumberFormat(undefined).format(12_000))).toBeInTheDocument();
    expect(screen.getAllByText(/^Fits · /).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(onBack).toHaveBeenCalled();
  });

  it('says what it cannot estimate', async () => {
    h.capabilities = null;
    renderPanel(recommendation(
      { size_gb: 0, minimum_ram_gb: 0, recommended_ram_gb: 0, format: 'safetensors', capabilities: [], model_id: null, default_filename: null, download_url: null, supported_quantizations: [] } as never,
      { popularity_downloads: null, popularity_likes: null } as never,
    ));
    expect(screen.getByText('Not enough size or hardware information to estimate fit.')).toBeInTheDocument();
    expect(screen.getAllByText('Unknown')).toHaveLength(3);
    expect(screen.getByText('Safetensors')).toBeInTheDocument();
    // No source to download from: no control at all.
    await waitFor(() => expect(h.downloaded.isModelDownloaded).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: 'Download' })).not.toBeInTheDocument();
    expect(screen.queryByText('Checking…')).not.toBeInTheDocument();
    // Header subtitle and the selected download both fall back to the catalog id.
    expect(screen.getAllByText('saga-7b-q4')).toHaveLength(2);
  });

  it('shows a smaller model in megabytes', () => {
    renderPanel(recommendation({ size_gb: 0.35 }));
    expect(screen.getAllByText(/350 MB/).length).toBeGreaterThan(0);
  });

  it('switches to another version of the model', async () => {
    renderPanel(recommendation());
    expect(h.pickerProps?.capabilities).toBe(h.capabilities);
    fireEvent.click(screen.getByRole('button', { name: 'Pick Q8' }));
    expect(await screen.findByText('saga-7b-Q8_0.gguf')).toBeInTheDocument();
    expect(screen.getByText('Chat · Q8_0 · 7.7 GB')).toBeInTheDocument();
    await waitFor(() => expect(h.downloaded.isModelDownloaded).toHaveBeenCalledWith('variant-q8'));
  });

  describe('download control', () => {
    it('starts a download', async () => {
      const pending = deferred<{ alreadyDownloaded: boolean }>();
      h.startModelDownload.mockReturnValueOnce(pending.promise);
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Download' }));
      expect(screen.getByText('Starting…')).toBeInTheDocument();
      expect(h.startModelDownload).toHaveBeenCalledWith(expect.objectContaining({ metadata: expect.objectContaining({ id: 'saga-7b-q4' }), addToast: h.addToast }));
      await act(async () => { pending.resolve({ alreadyDownloaded: false }); await pending.promise; });
      expect(screen.getByRole('button', { name: 'Download' })).toBeInTheDocument();
    });

    it('marks a model already on disk as downloaded', async () => {
      h.startModelDownload.mockResolvedValueOnce({ alreadyDownloaded: true });
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Download' }));
      expect(await screen.findByText('Downloaded')).toBeInTheDocument();
    });

    it('shows download progress', async () => {
      h.hasActiveDownload = true;
      h.activeDownload = { state: 'Downloading', percentage: 41.6 };
      renderPanel(recommendation());
      expect(await screen.findByText('Downloading 42%')).toBeInTheDocument();
    });

    it('shows a queued download without a percentage', async () => {
      h.hasActiveDownload = true;
      h.activeDownload = { state: 'Queued' };
      renderPanel(recommendation());
      expect(await screen.findByText('Downloading…')).toBeInTheDocument();
    });

    it('rechecks the disk when a download completes', async () => {
      h.activeDownload = { state: 'Completed' };
      h.downloaded.isModelDownloaded.mockResolvedValue(true);
      renderPanel(recommendation());
      expect(await screen.findByText('Downloaded')).toBeInTheDocument();
      expect(h.downloaded.isModelDownloaded.mock.calls.length).toBeGreaterThanOrEqual(2);
    });

    it('keeps the control usable when the disk check fails', async () => {
      const error = vi.spyOn(console, 'error').mockImplementation(() => {});
      h.downloaded.isModelDownloaded.mockRejectedValue(new Error('fs error'));
      renderPanel(recommendation());
      expect(await screen.findByRole('button', { name: 'Download' })).toBeInTheDocument();
      expect(error).toHaveBeenCalled();
      error.mockRestore();
    });

    it('asks for a Hugging Face token for a gated model', async () => {
      const onAddToken = vi.fn();
      renderPanel(recommendation({ requires_auth: true }), { onAddToken });
      const buttons = screen.getAllByRole('button', { name: 'Token required · Add token' });
      expect(buttons).toHaveLength(2);
      fireEvent.click(buttons[0]);
      expect(onAddToken).toHaveBeenCalled();
      await waitFor(() => expect(h.downloaded.isModelDownloaded).toHaveBeenCalled());
    });

    it('notes a gated model when the token is set or cannot be added here', async () => {
      h.hfToken = { isSet: true };
      const first = render(<QueryClientProvider client={queryClient}><ModelDetailPanel model={recommendation({ requires_auth: true })} onBack={vi.fn()} /></QueryClientProvider>);
      expect(screen.getByText('Gated · token set')).toBeInTheDocument();
      await screen.findByRole('button', { name: 'Download' });
      first.unmount();
      h.hfToken = { isSet: false };
      renderPanel(recommendation({ requires_auth: true }));
      expect(screen.getByText('Hugging Face token required')).toBeInTheDocument();
      await screen.findByRole('button', { name: 'Download' });
    });

    it('blocks an embedding model this build cannot run', async () => {
      renderPanel(recommendation({
        category: 'Embedding',
        embedding_compatibility: { kind: 'incompatible', reason: 'Needs a newer runtime' },
      } as never));
      const blocked = screen.getByRole('button', { name: 'Not supported on this build' });
      expect(blocked).toHaveAttribute('title', 'Needs a newer runtime');
      expect(screen.getAllByText('Needs a newer runtime').length).toBeGreaterThan(0);
      expect(await screen.findByRole('button', { name: 'Use for embeddings' })).toBeDisabled();
    });

    it('names an unknown embedding architecture', async () => {
      renderPanel(recommendation({ category: 'Embedding', embedding_compatibility: { kind: 'unknown' } } as never));
      expect(screen.getAllByText('This architecture is not recognized by the local embedding runtime.').length).toBeGreaterThan(0);
      await waitFor(() => expect(h.downloaded.isModelDownloaded).toHaveBeenCalled());
    });
  });

  describe('using a downloaded chat model', () => {
    beforeEach(() => {
      h.downloaded.isModelDownloaded.mockResolvedValue(true);
    });

    it('sets it as the chat model and warms it up', async () => {
      renderPanel(recommendation());
      const use = await screen.findByRole('button', { name: 'Use for chat' });
      await waitFor(() => expect(use).toBeEnabled());
      fireEvent.click(use);
      await waitFor(() => expect(toastTitles()).toContain('Chat model ready'));
      expect(h.downloaded.setActiveChatModel).toHaveBeenCalledWith('saga-7b-q4');
      expect(h.updateSettings).not.toHaveBeenCalled();
    });

    it('switches the provider back to local first', async () => {
      h.settings = { llm: { provider: 'ollama' } };
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for chat' }));
      await waitFor(() => expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { provider: 'local' } }));
    });

    it('reports a failed warm-up as a partial success', async () => {
      h.downloaded.warmUpActiveChatModel.mockRejectedValueOnce(new Error('out of memory'));
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for chat' }));
      await waitFor(() => expect(h.addToast).toHaveBeenCalledWith(expect.objectContaining({
        type: 'warning', title: 'Chat model updated', message: expect.stringContaining('out of memory'),
      })));
    });

    it('reports a chat model that could not be set', async () => {
      h.downloaded.setActiveChatModel.mockRejectedValueOnce(new Error('model file corrupt'));
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for chat' }));
      await waitFor(() => expect(h.addToast).toHaveBeenCalledWith(expect.objectContaining({ type: 'error', title: "Couldn't set the chat model" })));
    });

    it('shows progress while setting', async () => {
      const pending = deferred<void>();
      h.downloaded.setActiveChatModel.mockReturnValueOnce(pending.promise);
      renderPanel(recommendation());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for chat' }));
      expect(screen.getByRole('button', { name: 'Setting…' })).toBeDisabled();
      await act(async () => { pending.resolve(); await pending.promise; });
    });

    describe('routing', () => {
      it('sets it as the router model', async () => {
        const onSetRouterModel = vi.fn().mockResolvedValue(undefined);
        renderPanel(recommendation(), { onSetRouterModel });
        const route = await screen.findByRole('button', { name: 'Use for routing' });
        await waitFor(() => expect(route).toBeEnabled());
        fireEvent.click(route);
        await waitFor(() => expect(toastTitles()).toContain('Router model updated'));
        expect(onSetRouterModel).toHaveBeenCalledWith('saga-7b-q4');
      });

      it('reports a router model that could not be set', async () => {
        const onSetRouterModel = vi.fn().mockRejectedValue(new Error('router busy'));
        renderPanel(recommendation(), { onSetRouterModel });
        fireEvent.click(await screen.findByRole('button', { name: 'Use for routing' }));
        await waitFor(() => expect(toastTitles()).toContain("Couldn't set the router model"));
      });

      it('marks the active router model', async () => {
        renderPanel(recommendation(), { onSetRouterModel: vi.fn(), routerModelId: 'saga-7b-q4' });
        expect(screen.getByRole('button', { name: 'Routing' })).toBeDisabled();
        expect(screen.getByText('Chat · Q4_K_M · 4.4 GB · Router')).toBeInTheDocument();
        await screen.findByText('Downloaded');
      });

      it('shows progress while routing is set', async () => {
        const pending = deferred<void>();
        renderPanel(recommendation(), { onSetRouterModel: vi.fn(() => pending.promise) });
        fireEvent.click(await screen.findByRole('button', { name: 'Use for routing' }));
        await waitFor(() => expect(screen.getAllByRole('button', { name: 'Setting…' })).toHaveLength(1));
        await act(async () => { pending.resolve(); await pending.promise; });
      });
    });
  });

  describe('using a downloaded embedding model', () => {
    const embedding = () => recommendation({ category: 'Embedding', embedding_dimensions: 768, default_filename: 'bge-small.gguf', supported_quantizations: [] } as never);

    beforeEach(() => {
      h.downloaded.isModelDownloaded.mockResolvedValue(true);
    });

    it('sets it as the embedding model', async () => {
      renderPanel(embedding());
      expect(screen.getByText('768')).toBeInTheDocument();
      const use = await screen.findByRole('button', { name: 'Use for embeddings' });
      await waitFor(() => expect(use).toBeEnabled());
      fireEvent.click(use);
      await waitFor(() => expect(toastTitles()).toContain('Embedding model updated'));
      expect(h.downloaded.setActiveEmbeddingModel).toHaveBeenCalledWith('saga-7b-q4');
      expect(screen.queryByRole('button', { name: 'Use for chat' })).not.toBeInTheDocument();
    });

    it('reports an embedding model that could not be set', async () => {
      h.downloaded.setActiveEmbeddingModel.mockRejectedValueOnce(new Error('dimension mismatch'));
      renderPanel(embedding());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for embeddings' }));
      await waitFor(() => expect(toastTitles()).toContain("Couldn't set the embedding model"));
    });

    it('shows progress while setting', async () => {
      const pending = deferred<void>();
      h.downloaded.setActiveEmbeddingModel.mockReturnValueOnce(pending.promise);
      renderPanel(embedding());
      fireEvent.click(await screen.findByRole('button', { name: 'Use for embeddings' }));
      expect(screen.getByRole('button', { name: 'Setting…' })).toBeDisabled();
      await act(async () => { pending.resolve(); await pending.promise; });
    });
  });

  it('asks for a download before a model can be used', async () => {
    // The buttons are disabled until the check says the file is there; the
    // handlers still refuse if reached.
    h.downloaded.isModelDownloaded.mockResolvedValue(false);
    renderPanel(recommendation(), { onSetRouterModel: vi.fn() });
    await screen.findByRole('button', { name: 'Download' });
    expect(screen.getByRole('button', { name: 'Use for chat' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Use for routing' })).toBeDisabled();
  });
});
