import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelsTab } from './ModelsTab';
import { VaultAPI } from '../../../lib/api';
import { makeAppSettings } from '../../../tests/fixtures/appSettings';

vi.mock('./useLlmSettings', () => ({
  useLlmSettings: () => ({
    llmSettings: makeAppSettings().llm,
    isLoading: false,
    saveLlmUpdates: vi.fn().mockResolvedValue(true),
    reload: vi.fn(),
  }),
}));

vi.mock('../ModelCatalog', () => ({
  ModelCatalogBrowser: () => <div data-testid="catalog" />,
}));

vi.mock('../HuggingFaceSettings', () => ({
  HuggingFaceSettings: () => <div data-testid="hf-settings" />,
}));

describe('ModelsTab downloads folder', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows the path when the backend can read it', async () => {
    vi.spyOn(VaultAPI, 'getModelDownloadPath').mockResolvedValue({
      ok: true,
      data: '/Users/example/.cache/lattice/models',
    });

    render(<ModelsTab />);

    expect(await screen.findByText('/Users/example/.cache/lattice/models')).toBeInTheDocument();
  });

  it('offers Try again on the degraded state, and re-runs the loader', async () => {
    const user = userEvent.setup();
    const getPath = vi
      .spyOn(VaultAPI, 'getModelDownloadPath')
      .mockResolvedValueOnce({ ok: false, error: 'no disk access' })
      .mockResolvedValue({ ok: true, data: '/Users/example/.cache/lattice/models' });

    render(<ModelsTab />);

    expect(
      await screen.findByText(
        "Couldn't read the models folder. Check that the app has disk access.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Try again' }));

    await waitFor(() => expect(getPath).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('/Users/example/.cache/lattice/models')).toBeInTheDocument();
  });
});
