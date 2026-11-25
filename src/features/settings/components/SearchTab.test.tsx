import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { VaultAPI } from '@/lib/api';
import { createTestApi } from '@/tests/apiClient';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadStatus } from '@/types/downloads';

import { chooseRadixOption } from './radixSelectTestUtils';
import { describeReranker, formatDownloadSize, SearchTab } from './SearchTab';

const baseSettings = makeAppSettings();
const listDownloads = vi.fn();

function renderSearchTab() {
  // A fresh client per test so nothing leaks between them.
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ApiProvider client={createTestApi({ listDownloads: listDownloads as never })}>
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    </ApiProvider>
  );
  return render(<SearchTab />, { wrapper });
}

describe('SearchTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listDownloads.mockResolvedValue({ ok: true, data: [] });
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

  it('offers no switch for reranking, which is always on', async () => {
    renderSearchTab();

    expect(await screen.findByText('Reranker')).toBeInTheDocument();
    expect(screen.queryByLabelText(/Rerank/i)).not.toBeInTheDocument();
  });

  it('persists the read web pages switch inside the retrieval tuning', async () => {
    const user = userEvent.setup();
    renderSearchTab();

    const toggle = (await screen.findByLabelText(/Read web pages/i)) as HTMLInputElement;
    expect(toggle).toBeChecked();

    await user.click(toggle);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({ readWebPages: false }),
          }),
        })
      );
    });
  });

  it('shows advanced tuning and persists tuning fields', async () => {
    renderSearchTab();

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced retrieval tuning/i,
    });
    fireEvent.click(advancedToggle);

    const field = await screen.findByLabelText(/Doc shortlist doc min/i);
    fireEvent.change(field, { target: { value: '12' } });
    fireEvent.blur(field);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              docShortlistDocMin: 12,
            }),
          }),
        })
      );
    });
  });

  it('re-reads the repository after a write instead of trusting its own copy', async () => {
    const user = userEvent.setup();

    const stored = makeAppSettings({
      search: { ...baseSettings.search, maxResults: 42 },
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

    const checkbox = (await screen.findByLabelText(/Document summaries/i)) as HTMLInputElement;
    expect(getSettings).toHaveBeenCalledTimes(1);

    await user.click(checkbox);

    await waitFor(() => {
      expect(getSettings).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByDisplayValue('42')).toBeInTheDocument();
  });

  it('hides the truncation knobs until truncation is chosen, then persists them', async () => {
    renderSearchTab();

    const mode = await screen.findByLabelText(/Vector storage/i);
    expect(mode).toHaveTextContent('Full precision');
    // Dimensions and precision describe a truncation that is not happening.
    expect(screen.queryByLabelText(/Stored dimensions/i)).not.toBeInTheDocument();

    await chooseRadixOption(mode, 'Matryoshka truncation');

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

  it('normalizes paired bounds before persisting tuning', async () => {
    renderSearchTab();

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced retrieval tuning/i,
    });
    fireEvent.click(advancedToggle);

    const minField = await screen.findByLabelText(/Doc shortlist doc min/i);
    fireEvent.change(minField, { target: { value: '120' } });
    fireEvent.blur(minField);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              docShortlistDocMin: 120,
              docShortlistDocMax: 120,
            }),
          }),
        })
      );
    });
  });
});

describe('the reranker row', () => {
  const qwen = { modelName: 'Qwen/Qwen3-Reranker-0.6B', downloadBytes: 639_153_184, path: '/models/reranker/qwen3.gguf' };

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
    listDownloads.mockResolvedValue({ ok: true, data: [] });
  });

  it('offers the download, with its size, and says results are not reranked yet', async () => {
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: { ...qwen, installed: false },
    });

    renderSearchTab();

    expect(await screen.findByRole('button', { name: /Download \(639 MB\)/ })).toBeInTheDocument();
    expect(screen.getByText(/not being reranked/)).toBeInTheDocument();
  });

  it('stops offering the download once the model is installed', async () => {
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: { ...qwen, installed: true },
    });

    renderSearchTab();

    await screen.findByText(/A cross-encoder rescores the shortlist/);
    expect(screen.queryByRole('button', { name: /Download/ })).not.toBeInTheDocument();
  });

  it('starts the download without a settings change, and follows it in the download list', async () => {
    const downloadReranker = vi.spyOn(VaultAPI, 'downloadReranker').mockResolvedValue({
      ok: true,
      data: { ...qwen, installed: false },
    });
    vi.spyOn(VaultAPI, 'getRerankerStatus').mockResolvedValue({
      ok: true,
      data: { ...qwen, installed: false },
    });

    renderSearchTab();

    const button = await screen.findByRole('button', { name: /Download/ });
    await waitFor(() => expect(button).toBeEnabled());
    listDownloads.mockResolvedValue({
      ok: true,
      data: [{
        id: 'dl-1', url: 'https://example.org/r.gguf', destination: qwen.path, state: 'downloading', bytes_downloaded: 10,
        total_bytes: 100, bytes_per_second: 1, percentage: 10, eta_seconds: null, error_message: null, retry_count: 0,
        created_at: '2026-10-01T00:00:00Z', started_at: null, completed_at: null, model_name: null, model_id: null,
      }],
    });
    await userEvent.click(button);

    await waitFor(() => expect(downloadReranker).toHaveBeenCalledTimes(1));
    expect(VaultAPI.updateSettings).not.toHaveBeenCalled();
    expect(await screen.findByRole('status', { name: 'Downloading the reranker' })).toHaveTextContent('Downloading… 10%');
    expect(screen.getByText('Reranking turns on when the download finishes.')).toBeInTheDocument();
  });
});

describe('describeReranker', () => {
  const status = {
    modelName: 'Qwen/Qwen3-Reranker-0.6B',
    downloadBytes: 639_153_184,
    path: '/models/reranker/qwen3.gguf',
  };
  const failed = { state: 'Failed', error_message: 'network unreachable' } as DownloadStatus;

  it('says nothing before the status has loaded', () => {
    expect(describeReranker(undefined, 'idle')).toBeUndefined();
  });

  it('reports a failed latest download when idle', () => {
    expect(describeReranker({ ...status, installed: false }, 'idle', failed)).toBe(
      "Couldn't download the reranker: network unreachable"
    );
  });

  it('tells a missing model from a working one', () => {
    expect(describeReranker({ ...status, installed: false }, 'idle')).toMatch(/not being reranked/);
    expect(describeReranker({ ...status, installed: true }, 'ready')).toMatch(/rescores the shortlist/);
    expect(describeReranker({ ...status, installed: true }, 'ready', failed)).toMatch(/rescores the shortlist/);
  });

  it('says it is downloading while live or finishing', () => {
    const text = 'Reranking turns on when the download finishes.';
    expect(describeReranker({ ...status, installed: false }, 'live', failed)).toBe(text);
    expect(describeReranker({ ...status, installed: false }, 'finishing')).toBe(text);
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
