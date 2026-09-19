import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { describeReranker, formatDownloadSize, SearchTab } from './SearchTab';
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

  it('hides the truncation knobs until truncation is chosen, then persists them', async () => {
    renderSearchTab();

    const mode = (await screen.findByLabelText(/Vector storage/i)) as HTMLSelectElement;
    expect(mode.value).toBe('none');
    // Dimensions and precision describe a truncation that is not happening.
    expect(screen.queryByLabelText(/Stored dimensions/i)).not.toBeInTheDocument();

    fireEvent.change(mode, { target: { value: 'truncated' } });

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            vectorIndexCompression: expect.objectContaining({
              mode: 'truncated',
              // The remembered dims and precision ride along unchanged.
              dims: 512,
              quantization: 'i8',
            }),
          }),
        })
      );
    });
  });

  it('persists the embedding strategy', async () => {
    renderSearchTab();

    const strategy = (await screen.findByLabelText(/Embedding strategy/i)) as HTMLSelectElement;
    expect(strategy.value).toBe('chunk_first');

    fireEvent.change(strategy, { target: { value: 'late_chunking' } });

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'search',
        updates: { embeddingStrategy: 'late_chunking' },
      });
    });
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

describe('the reranking row', () => {
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

  it('offers the download, with its size, when the model is missing', async () => {
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: {
        installed: false,
        enabled: false,
        active: false,
        modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
        downloadBytes: 90_870_598,
      },
    });

    renderSearchTab();

    expect(await screen.findByRole('button', { name: /Download \(91 MB\)/ })).toBeInTheDocument();
  });

  it('says so when the switch is on but nothing can rerank', async () => {
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: {
        installed: false,
        enabled: true,
        active: false,
        modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
        downloadBytes: 90_870_598,
      },
    });

    renderSearchTab();

    expect(
      await screen.findByText(/Switched on, but the reranker model is not downloaded/)
    ).toBeInTheDocument();
  });

  it('stops offering the download once the model is installed', async () => {
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: {
        installed: true,
        enabled: true,
        active: true,
        modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
        downloadBytes: 90_870_598,
      },
    });

    renderSearchTab();

    await screen.findByText(/A cross-encoder rescores the shortlist/);
    expect(screen.queryByRole('button', { name: /Download/ })).not.toBeInTheDocument();
  });

  it('downloading does not switch reranking on', async () => {
    const downloadReranker = vi.spyOn(VaultAPI, 'downloadReranker').mockResolvedValue({
      ok: true,
      data: {
        installed: true,
        enabled: false,
        active: false,
        modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
        downloadBytes: 90_870_598,
      },
    });
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: {
        installed: false,
        enabled: false,
        active: false,
        modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
        downloadBytes: 90_870_598,
      },
    });

    renderSearchTab();

    await userEvent.click(await screen.findByRole('button', { name: /Download/ }));

    await waitFor(() => expect(downloadReranker).toHaveBeenCalledTimes(1));
    // Acquiring the model is not consent to use it.
    expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    expect(await screen.findByText(/Model ready\. Switch on/)).toBeInTheDocument();
  });
});

describe('describeReranker', () => {
  const status = {
    modelName: 'cross-encoder/ms-marco-MiniLM-L-6-v2',
    downloadBytes: 90_870_598,
  };

  it('says nothing before the status has loaded', () => {
    expect(describeReranker(undefined)).toBeUndefined();
  });

  it('reports a failed download over any other state', () => {
    expect(
      describeReranker(
        { ...status, installed: true, enabled: true, active: true },
        new Error('network unreachable')
      )
    ).toContain('network unreachable');
  });

  it('distinguishes never-downloaded from on-but-unusable', () => {
    const idle = describeReranker({ ...status, installed: false, enabled: false, active: false });
    const broken = describeReranker({ ...status, installed: false, enabled: true, active: false });
    expect(idle).not.toEqual(broken);
    expect(broken).toMatch(/not being reranked/);
  });
});

describe('formatDownloadSize', () => {
  it('rounds to whole megabytes', () => {
    expect(formatDownloadSize(90_870_598)).toBe('91 MB');
  });

  it('switches to gigabytes past a thousand megabytes', () => {
    expect(formatDownloadSize(2_400_000_000)).toBe('2.4 GB');
  });
});
