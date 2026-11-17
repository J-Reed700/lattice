import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { UtilityModelNotice } from '@/features/chat/components/UtilityModelNotice';
import type { ModelRef } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ models: vi.fn(), downloaded: vi.fn(), settings: vi.fn() }));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: mocks.settings }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: mocks.models }));
vi.mock('@/lib/api', () => ({ default: { isModelDownloaded: mocks.downloaded } }));

function show() {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <MemoryRouter><UtilityModelNotice /></MemoryRouter>
  </QueryClientProvider>);
}

describe('UtilityModelNotice', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.settings.mockReturnValue({ data: { llm: { utilityModel: null } }, isPending: false, error: null });
  });

  it('explains unavailable utility planning and links directly to the catalog', () => {
    mocks.models.mockReturnValue({ downloadedModels: [], isLoading: false, error: null });
    show();
    expect(screen.getByRole('status')).toHaveTextContent('No downloaded utility model is available');
    expect(screen.getByRole('link')).toHaveAttribute('href', '/settings?tab=models');
  });

  it('does not mistake a remote utility model for a missing download', () => {
    mocks.models.mockReturnValue({ downloadedModels: [{ model_id: 'remote', backend: 'ollama', is_active_for_utility: true }], isLoading: false });
    show();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(mocks.downloaded).not.toHaveBeenCalled();
  });

  it('reports a local model whose file is no longer ready', async () => {
    mocks.models.mockReturnValue({ downloadedModels: [{ model_id: 'local', model_name: 'Utility', backend: 'local', is_active_for_utility: true }], isLoading: false });
    mocks.downloaded.mockResolvedValue({ ok: true, data: false });
    show();
    expect(await screen.findByRole('status')).toHaveTextContent('not fully downloaded or its file is missing');
  });

  it('reports status errors without claiming a model is missing', () => {
    mocks.models.mockReturnValue({ downloadedModels: [], isLoading: false, error: 'Offline' });
    show();
    expect(screen.getByRole('status')).toHaveTextContent('availability could not be checked');
  });

  it.each<ModelRef>([
    { kind: 'llama_cpp', model: 'remote.gguf' },
    { kind: 'cloud', provider: 'anthropic', model: 'utility-test-model' },
  ])('shows no warning when Settings save %j as the utility model', (utilityModel) => {
    mocks.models.mockReturnValue({ downloadedModels: [], isLoading: false, error: null });
    mocks.settings.mockReturnValue({ data: { llm: { utilityModel } }, isPending: false, error: null });
    show();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('trusts the utility model saved in Settings over a leftover flag on a downloaded model', async () => {
    mocks.models.mockReturnValue({ downloadedModels: [{ model_id: 'local', model_name: 'Old utility', backend: 'local', is_active_for_utility: true }], isLoading: false, error: null });
    mocks.settings.mockReturnValue({ data: { llm: { utilityModel: { kind: 'llama_cpp', model: 'remote.gguf' } } }, isPending: false, error: null });
    mocks.downloaded.mockResolvedValue({ ok: true, data: false });
    show();
    await act(async () => {});
    expect(mocks.downloaded).not.toHaveBeenCalled();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('shows nothing while Settings are still loading', () => {
    mocks.models.mockReturnValue({ downloadedModels: [], isLoading: false, error: null });
    mocks.settings.mockReturnValue({ data: undefined, isPending: true, error: null });
    show();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('says availability could not be checked when Settings fail to load', () => {
    mocks.models.mockReturnValue({ downloadedModels: [], isLoading: false, error: null });
    mocks.settings.mockReturnValue({ data: undefined, isPending: false, error: new Error('settings offline') });
    show();
    expect(screen.getByRole('status')).toHaveTextContent('availability could not be checked');
  });
});
