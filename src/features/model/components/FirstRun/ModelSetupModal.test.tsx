import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { router } from '@/routes';
import { toast } from '@/stores/toastStore';

import { ModelSetupModal } from './ModelSetupModal';

const mocks = vi.hoisted(() => ({
  catalog: { systemCapabilities: null as null | { available_disk_gb: number }, compatibleModels: [] as unknown[] },
  api: { checkFirstRunStatus: vi.fn(), downloadDefaultEmbeddingModel: vi.fn(), downloadModel: vi.fn() },
}));
vi.mock('@/routes', () => ({ router: { navigate: vi.fn() } }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => mocks.catalog }));
vi.mock('@/lib/api', () => ({ default: mocks.api, VaultAPI: mocks.api }));

const GB = 1_073_741_824;
const status = (overrides: Record<string, unknown> = {}) => JSON.stringify({
  needs_setup: true,
  embedding_model: { model_id: 'bge-small', display_name: 'BGE Small', estimated_size_bytes: 134_217_728 },
  chat_model: { model_id: 'herb-4b', display_name: 'Herb 4B', estimated_size_bytes: 2.5 * GB },
  total_estimated_size_bytes: 2_818_572_288,
  ...overrides,
});

function renderModal() {
  const props = { onOpenChange: vi.fn(), onComplete: vi.fn(), onDismiss: vi.fn() };
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const view = render(<ModelSetupModal open {...props} />, { wrapper });
  return { ...props, ...view };
}

const install = () => screen.findByRole('button', { name: /^Install/ });

beforeEach(() => {
  mocks.catalog = { systemCapabilities: null, compatibleModels: [] };
  mocks.api.checkFirstRunStatus.mockReset().mockResolvedValue({ ok: true, data: status() });
  mocks.api.downloadDefaultEmbeddingModel.mockReset().mockResolvedValue({ ok: true, data: 'embed-job' });
  mocks.api.downloadModel.mockReset().mockResolvedValue({ ok: true, data: { id: 'chat-job' } });
  vi.mocked(router.navigate).mockReset();
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('ModelSetupModal', () => {
  it('offers the recommended bundle with each model’s size', async () => {
    renderModal();
    expect(await install()).toHaveTextContent('Install · 2.6 GB');
    expect(screen.getByText('Herb 4B')).toBeInTheDocument();
    expect(screen.getByText('2.5 GB')).toBeInTheDocument();
    expect(screen.getByText('BGE Small')).toBeInTheDocument();
    expect(screen.getByText('128 MB')).toBeInTheDocument();
    expect(screen.queryByText('can see images')).not.toBeInTheDocument();
  });

  it('marks a chat model the catalog says can see pictures', async () => {
    mocks.catalog.compatibleModels = [{ model: { id: 'herb-4b', capabilities: ['vision'] } }];
    renderModal();
    expect(await screen.findByText('can see images')).toBeInTheDocument();
  });

  it('shows nothing when no setup is needed', async () => {
    mocks.api.checkFirstRunStatus.mockResolvedValue({ ok: true, data: status({ needs_setup: false }) });
    renderModal();
    await waitFor(() => expect(mocks.api.checkFirstRunStatus).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('installs both models in the background and closes at once', async () => {
    const props = renderModal();
    fireEvent.click(await install());
    await waitFor(() => expect(mocks.api.downloadDefaultEmbeddingModel).toHaveBeenCalled());
    await waitFor(() => expect(mocks.api.downloadModel).toHaveBeenCalledWith('herb-4b'));
    expect(toast.success).toHaveBeenCalledWith('Installing models · 2.6 GB', { message: 'Downloading in the background.' });
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onComplete).toHaveBeenCalled();
    expect(props.onDismiss).not.toHaveBeenCalled();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('reports a download that could not start', async () => {
    mocks.api.downloadDefaultEmbeddingModel.mockResolvedValue({ ok: false, error: 'disk full' });
    mocks.api.downloadModel.mockResolvedValue({ ok: false, error: 'hub offline' });
    renderModal();
    fireEvent.click(await install());
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('Embedding model download failed', { message: 'disk full' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('Chat model download failed', { message: 'hub offline' }));
  });

  it('installs the embedding model alone, with an estimated size label', async () => {
    mocks.api.checkFirstRunStatus.mockResolvedValue({ ok: true, data: status({ chat_model: null, total_estimated_size_bytes: null }) });
    renderModal();
    const button = await install();
    expect(button).toHaveTextContent('Install · ~5 GB');
    expect(screen.queryByText('BGE Small')).not.toBeInTheDocument();
    fireEvent.click(button);
    expect(mocks.api.downloadModel).not.toHaveBeenCalled();
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Installing models', { message: 'Downloading in the background.' }));
  });

  it('cannot install without an embedding model', async () => {
    mocks.api.checkFirstRunStatus.mockResolvedValue({ ok: true, data: status({ embedding_model: null }) });
    renderModal();
    expect(await install()).toBeDisabled();
  });

  it('blocks the install when the disk is too small', async () => {
    mocks.catalog.systemCapabilities = { available_disk_gb: 1.2 };
    renderModal();
    expect(await install()).toBeDisabled();
    expect(screen.getByText('Not enough free disk. Needs 2.6 GB, 1.2 GB free.')).toBeInTheDocument();
  });

  it('ignores an unknown free disk figure', async () => {
    mocks.catalog.systemCapabilities = { available_disk_gb: 0 };
    renderModal();
    expect(await install()).toBeEnabled();
  });

  it('skips for now, remembering the offer', async () => {
    const props = renderModal();
    await install();
    fireEvent.click(screen.getByRole('button', { name: 'Not now' }));
    expect(props.onDismiss).toHaveBeenCalled();
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onComplete).toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('sends the user to choose models in Settings', async () => {
    const props = renderModal();
    await install();
    fireEvent.click(screen.getByRole('button', { name: 'Choose models' }));
    expect(props.onDismiss).toHaveBeenCalled();
    expect(router.navigate).toHaveBeenCalledWith('/settings');
    expect(props.onComplete).toHaveBeenCalled();
  });
});
