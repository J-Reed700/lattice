import { createElement, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useDownloadStore } from '@/features/model/stores/downloadStore';
import { VaultAPI } from '@/lib/api';
import type { DownloadStatus } from '@/types/downloads';
import type { TauriEvents } from '@/types/events';

import { applyDownloadSnapshot, fileNameWithinModel, useDownloadsListener } from './useDownloads';

const queuedDownload = (id: string, filename: string): DownloadStatus => ({
  id,
  url: filename,
  destination: filename,
  state: 'Pending',
  bytes_downloaded: 0,
  total_bytes: 100,
  bytes_per_second: 0,
  percentage: 0,
  eta_seconds: null,
  error_message: null,
  retry_count: 0,
  created_at: '2026-09-16T00:00:00.000Z',
  started_at: null,
  completed_at: null,
  model_name: filename,
  model_id: null,
});

describe('applyDownloadSnapshot', () => {
  it('replaces an early standalone row when its model becomes a batch', () => {
    const earlySession = queuedDownload('session-weights', 'model.safetensors');
    const current = new Map<string, DownloadStatus>([[earlySession.id, earlySession]]);
    const snapshot: TauriEvents.Downloads.Batch = {
      kind: 'batch',
      id: 'Qwen/Qwen3-Embedding-0.6B',
      groupName: 'Qwen/Qwen3-Embedding-0.6B',
      files: [
        { id: 'session-config', filename: 'config.json', bytesDownloaded: 10, totalBytes: 10, status: 'completed' },
        { id: 'session-tokenizer', filename: 'tokenizer.json', bytesDownloaded: 20, totalBytes: 20, status: 'completed' },
        { id: 'session-weights', filename: 'model.safetensors', bytesDownloaded: 100, totalBytes: 100, status: 'completed' },
      ],
      totalFiles: 3,
      completedFiles: 3,
      aggregateBytesDownloaded: 130,
      aggregateTotalBytes: 130,
      aggregateBytesPerSecond: 0,
      aggregatePercentage: 100,
      aggregateEtaSeconds: null,
      status: 'completed',
    };

    const next = applyDownloadSnapshot(current, snapshot);

    expect(next).toHaveLength(3);
    expect(next.has('session-weights')).toBe(false);
    expect(next.get('Qwen/Qwen3-Embedding-0.6B:model.safetensors')).toMatchObject({
      id: 'session-weights',
      state: 'Completed',
      model_id: 'Qwen/Qwen3-Embedding-0.6B',
    });
  });
});

describe('applyDownloadSnapshot after a row finished', () => {
  it('ignores a late progress tick for a completed row', () => {
    const done: DownloadStatus = { ...queuedDownload('s-1', 'model.gguf'), state: 'Completed', percentage: 100 };
    const current = new Map<string, DownloadStatus>([[done.id, done]]);
    const late: TauriEvents.Downloads.Single = {
      kind: 'single',
      id: 's-1',
      filename: 'model.gguf',
      bytesDownloaded: 90,
      totalBytes: 100,
      bytesPerSecond: 10,
      percentage: 90,
      etaSeconds: 1,
      status: 'downloading',
    };

    expect(applyDownloadSnapshot(current, late).get('s-1')).toMatchObject({ state: 'Completed', percentage: 100 });
  });
});

describe('useDownloadsListener', () => {
  it('still listens for progress when the initial download list fails', async () => {
    const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;
    api.listDownloads = vi.fn().mockResolvedValue({ ok: false, error: 'database is locked' });
    vi.mocked(listen).mockClear();
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client: queryClient }, children);

    renderHook(() => useDownloadsListener(), { wrapper });

    await waitFor(() => expect(useDownloadStore.getState().listenerError).toBe('database is locked'));
    const events = vi.mocked(listen).mock.calls.map(([name]) => name);
    expect(events).toEqual(expect.arrayContaining(['download:progress']));
    expect(events).toHaveLength(2);
  });
});

describe('fileNameWithinModel', () => {
  // A sentence-transformers model ships `config.json` and `1_Pooling/config.json`.
  // Keyed by the bare file name, one row replaced the other and a five-file
  // model showed as four.
  it('tells apart two files that share a name inside one model', () => {
    const base = '/Users/me/.cache/lattice/models/qwen3-embedding-0.6b';
    const names = ['config.json', '1_Pooling/config.json'].map((file) =>
      fileNameWithinModel({ destination: `${base}/${file}`, model_id: 'qwen3-embedding-0.6b' })
    );
    expect(names).toEqual(['config.json', '1_Pooling/config.json']);
  });

  it('reads a Windows path the same way', () => {
    expect(fileNameWithinModel({
      destination: 'C:\\models\\minilm\\1_Pooling\\config.json',
      model_id: 'minilm',
    })).toBe('1_Pooling/config.json');
  });

  it('falls back to the file name for a download that belongs to no model', () => {
    expect(fileNameWithinModel({ destination: '/tmp/file.bin', model_id: null })).toBe('file.bin');
  });
});
