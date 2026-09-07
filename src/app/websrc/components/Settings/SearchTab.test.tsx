import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchTab } from './SearchTab';
import { VaultAPI } from '../../lib/api';
import { makeAppSettings } from '../../tests/fixtures/appSettings';

const baseSettings = makeAppSettings();

function renderSearchTab() {
  // A fresh client per test so nothing leaks between them.
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<SearchTab />, { wrapper });
}

describe('SearchTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(VaultAPI, 'getSettings').mockResolvedValue({
      ok: true,
      data: structuredClone(baseSettings),
    });
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({
      ok: true,
      data: structuredClone(baseSettings),
    });
  });

  it('loads and renders backend search settings', async () => {
    renderSearchTab();

    expect(await screen.findByRole('heading', { level: 1, name: 'Search' })).toBeInTheDocument();
    expect(await screen.findByText('Retrieval')).toBeInTheDocument();
    expect(screen.getByDisplayValue('10')).toBeInTheDocument();
  });

  it('persists reranking toggle', async () => {
    const user = userEvent.setup();
    renderSearchTab();

    const checkbox = (await screen.findByLabelText(/Rerank results/i)) as HTMLInputElement;
    expect(checkbox).toBeChecked();

    await user.click(checkbox);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'search',
        updates: { enableReranking: false },
      });
    });
  });

  it('shows advanced tuning and persists tuning fields', async () => {
    renderSearchTab();

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced retrieval tuning/i,
    });
    fireEvent.click(advancedToggle);

    const field = await screen.findByLabelText(/KB search min limit/i);
    fireEvent.change(field, { target: { value: '12' } });
    fireEvent.blur(field);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              kbSearchMinLimit: 12,
            }),
          }),
        })
      );
    });
  });

  it('re-reads the repository after a write instead of trusting its own copy', async () => {
    const user = userEvent.setup();

    const stored = makeAppSettings({
      search: { ...baseSettings.search, enableReranking: false, maxResults: 42 },
    });
    const getSettings = vi.spyOn(VaultAPI, 'getSettings');
    getSettings.mockResolvedValueOnce({ ok: true, data: structuredClone(baseSettings) });
    getSettings.mockResolvedValue({ ok: true, data: structuredClone(stored) });
    // The command answers with only what it wrote; the canonical document
    // comes back on the refetch the invalidation triggers.
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({
      ok: true,
      data: structuredClone(baseSettings),
    });

    renderSearchTab();

    const checkbox = (await screen.findByLabelText(/Rerank results/i)) as HTMLInputElement;
    expect(getSettings).toHaveBeenCalledTimes(1);

    await user.click(checkbox);

    await waitFor(() => {
      expect(getSettings).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByDisplayValue('42')).toBeInTheDocument();
  });

  it('normalizes paired bounds before persisting tuning', async () => {
    renderSearchTab();

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced retrieval tuning/i,
    });
    fireEvent.click(advancedToggle);

    const minField = await screen.findByLabelText(/KB search min limit/i);
    fireEvent.change(minField, { target: { value: '120' } });
    fireEvent.blur(minField);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              kbSearchMinLimit: 120,
              kbSearchMaxLimit: 120,
            }),
          }),
        })
      );
    });
  });
});
