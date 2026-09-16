import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { UtilityModelNotice } from './UtilityModelNotice';

const mocks = vi.hoisted(() => ({ models: vi.fn(), downloaded: vi.fn() }));
vi.mock('@/hooks/useDownloadedModels', () => ({ useDownloadedModels: mocks.models }));
vi.mock('@/lib/api', () => ({ default: { isModelDownloaded: mocks.downloaded } }));

function show() {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <MemoryRouter><UtilityModelNotice /></MemoryRouter>
  </QueryClientProvider>);
}

describe('UtilityModelNotice', () => {
  beforeEach(() => vi.resetAllMocks());

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
});
