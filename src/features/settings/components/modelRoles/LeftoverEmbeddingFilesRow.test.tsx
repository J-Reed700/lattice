import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';

import { LeftoverEmbeddingFilesRow } from './LeftoverEmbeddingFilesRow';

const folder = '/home/me/.cache/lattice/models/qwen3-embedding-0.6b';
const leftovers = {
  paths: [`${folder}/model.safetensors`, `${folder}/config.json`],
  total_bytes: 1_191_587_143,
};

function renderRow() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <LeftoverEmbeddingFilesRow />
    </QueryClientProvider>,
  );
}

describe('LeftoverEmbeddingFilesRow', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows nothing when no old files are left', async () => {
    const list = vi
      .spyOn(VaultAPI, 'listLeftoverEmbeddingFiles')
      .mockResolvedValue({ ok: true, data: { paths: [], total_bytes: 0 } });
    renderRow();
    await waitFor(() => expect(list).toHaveBeenCalled());
    expect(screen.queryByText('Old embedding model files')).not.toBeInTheDocument();
  });

  it('shows the folder and size, and removes the files only after confirmation', async () => {
    vi.spyOn(VaultAPI, 'listLeftoverEmbeddingFiles')
      .mockResolvedValueOnce({ ok: true, data: leftovers })
      .mockResolvedValue({ ok: true, data: { paths: [], total_bytes: 0 } });
    const remove = vi
      .spyOn(VaultAPI, 'removeLeftoverEmbeddingFiles')
      .mockResolvedValue({ ok: true, data: leftovers });
    const user = userEvent.setup();
    renderRow();

    expect(await screen.findByText('Old embedding model files')).toBeVisible();
    expect(screen.getByText(folder)).toBeVisible();
    expect(screen.getByText(/1\.11 GB/)).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Remove' }));
    expect(remove).not.toHaveBeenCalled();
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: /^Remove/ }));

    await waitFor(() => expect(remove).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.queryByText('Old embedding model files')).not.toBeInTheDocument(),
    );
  });
});
