import { createElement, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useDownloadStore } from '@/features/model/stores/downloadStore';
import { VaultAPI } from '@/lib/api';
import type { DownloadStatus } from '@/types/downloads';
import type { TauriEvents } from '@/types/events';

import {
  applyDownloadSnapshot,
  fileNameWithinModel,
  useDownloadActions,
  useDownloadsListener,
} from './useDownloads';

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

describe('useDownloadActions', () => {
  const renderActions = () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client: queryClient }, children);
    return renderHook(() => useDownloadActions(), { wrapper }).result;
  };

  it('treats a session the backend no longer has as a stale row, not a failure', async () => {
    const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;
    api.pauseDownload = vi.fn().mockResolvedValue({
      ok: false,
      error: 'Not found: Download session not found: d1',
      details: { code: 'NOT_FOUND', message: 'Not found: Download session not found: d1' },
    });
    api.listDownloads = vi.fn().mockResolvedValue({ ok: true, data: [] });

    await expect(renderActions().current.pauseDownload('d1')).resolves.toBeUndefined();
    expect(api.listDownloads).toHaveBeenCalled();
  });

  it('names the backend reason when an action fails', async () => {
    const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;
    api.resumeDownload = vi.fn().mockResolvedValue({
      ok: false,
      error: 'Network error',
      details: { code: 'NETWORK_ERROR', message: 'Network error', details: 'connection reset' },
    });

    await expect(renderActions().current.resumeDownload('d1'))
      .rejects.toThrow('Failed to resume download: Network error: connection reset');
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
      destination: 'C:\\models\\qwen3-embedding-0.6b\\1_Pooling\\config.json',
      model_id: 'qwen3-embedding-0.6b',
    })).toBe('1_Pooling/config.json');
  });

  it('falls back to the file name for a download that belongs to no model', () => {
    expect(fileNameWithinModel({ destination: '/tmp/file.bin', model_id: null })).toBe('file.bin');
  });
});

describe('applyDownloadSnapshot for a row first seen in an event', () => {
  const single = (overrides: Partial<TauriEvents.Downloads.Single> = {}): TauriEvents.Downloads.Single => ({
    kind: 'single', id: 's-9', filename: 'rerank.gguf', bytesDownloaded: 5, totalBytes: 100, bytesPerSecond: 1,
    percentage: 5, etaSeconds: null, status: 'downloading', ...overrides,
  });

  it('takes its destination, modelId, and name from the payload', () => {
    const next = applyDownloadSnapshot(
      new Map(),
      single({ destination: '/models/rerank/rerank.gguf', modelId: 'rerank-id', modelName: 'Qwen3 Reranker' }),
    );
    expect(next.get('s-9')).toMatchObject({
      destination: '/models/rerank/rerank.gguf',
      model_id: 'rerank-id',
      model_name: 'Qwen3 Reranker',
    });
  });

  it('preserves existing model_id when a single snapshot omits modelId', () => {
    const existing = { ...queuedDownload('s-9', 'rerank.gguf'), model_id: 'existing-id' };
    const next = applyDownloadSnapshot(
      new Map([['s-9', existing]]),
      single({ modelId: undefined }),
    );
    expect(next.get('s-9')?.model_id).toBe('existing-id');
  });

  it('falls back to the file name for an older payload', () => {
    const next = applyDownloadSnapshot(new Map(), single());
    expect(next.get('s-9')).toMatchObject({ destination: 'rerank.gguf', model_name: 'rerank.gguf' });
  });

  it('maps paused and cancelled batch files', () => {
    const batch: TauriEvents.Downloads.Batch = {
      kind: 'batch', id: 'm', groupName: 'M',
      files: [
        { id: 'a', filename: 'a.bin', bytesDownloaded: 1, totalBytes: 10, status: 'paused' },
        { id: 'b', filename: 'b.bin', bytesDownloaded: 1, totalBytes: 10, status: 'cancelled' },
      ],
      totalFiles: 2, completedFiles: 0, aggregateBytesDownloaded: 2, aggregateTotalBytes: 20, aggregateBytesPerSecond: 0,
      aggregatePercentage: 10, aggregateEtaSeconds: null, status: 'paused',
    };
    const next = applyDownloadSnapshot(new Map(), batch);
    expect(next.get('m:a.bin')?.state).toBe('Paused');
    expect(next.get('m:b.bin')?.state).toBe('Cancelled');
  });

  it('preserves errorMessage from single and batch failed file snapshots', () => {
    const singleFailed = applyDownloadSnapshot(
      new Map(),
      single({ status: 'error', errorMessage: 'HTTP 401: Unauthorized' }),
    );
    expect(singleFailed.get('s-9')).toMatchObject({
      state: 'Failed',
      error_message: 'HTTP 401: Unauthorized',
    });

    const batchFailed: TauriEvents.Downloads.Batch = {
      kind: 'batch',
      id: 'm-err',
      groupName: 'M Err',
      files: [
        {
          id: 'f-1',
          filename: 'weights.bin',
          bytesDownloaded: 10,
          totalBytes: 100,
          status: 'error',
          errorMessage: 'HTTP 403: Forbidden',
        },
        {
          id: 'f-2',
          filename: 'config.json',
          bytesDownloaded: 10,
          totalBytes: 10,
          status: 'completed',
        },
      ],
      totalFiles: 2,
      completedFiles: 1,
      aggregateBytesDownloaded: 20,
      aggregateTotalBytes: 110,
      aggregateBytesPerSecond: 0,
      aggregatePercentage: 18,
      aggregateEtaSeconds: null,
      status: 'error',
    };
    const nextBatch = applyDownloadSnapshot(new Map(), batchFailed);
    expect(nextBatch.get('m-err:weights.bin')).toMatchObject({
      state: 'Failed',
      error_message: 'HTTP 403: Forbidden',
    });
    expect(nextBatch.get('m-err:config.json')?.error_message).toBeNull();
  });

  it('clears stale error_message on resumed or retried batch child files and single rows', () => {
    const staleErrorChild = {
      ...queuedDownload('f-1', 'weights.bin'),
      model_id: 'm-err',
      state: 'Paused' as const,
      error_message: 'HTTP 403: Forbidden',
    };
    const staleErrorSingle = {
      ...queuedDownload('s-9', 'rerank.gguf'),
      state: 'Pending' as const,
      error_message: 'HTTP 401: Unauthorized',
    };
    const current = new Map<string, DownloadStatus>([
      ['m-err:weights.bin', staleErrorChild],
      ['s-9', staleErrorSingle],
    ]);

    const nextAfterBatchRetry = applyDownloadSnapshot(current, {
      kind: 'batch',
      id: 'm-err',
      groupName: 'M Err',
      files: [
        {
          id: 'f-1',
          filename: 'weights.bin',
          bytesDownloaded: 10,
          totalBytes: 100,
          status: 'downloading',
        },
      ],
      totalFiles: 1,
      completedFiles: 0,
      aggregateBytesDownloaded: 10,
      aggregateTotalBytes: 100,
      aggregateBytesPerSecond: 5,
      aggregatePercentage: 10,
      aggregateEtaSeconds: 18,
      status: 'downloading',
    });
    expect(nextAfterBatchRetry.get('m-err:weights.bin')).toMatchObject({
      state: 'Downloading',
      error_message: null,
    });

    const nextAfterSingleRetry = applyDownloadSnapshot(
      nextAfterBatchRetry,
      single({ status: 'downloading', errorMessage: undefined }),
    );
    expect(nextAfterSingleRetry.get('s-9')).toMatchObject({
      state: 'Downloading',
      error_message: null,
    });
  });
});
