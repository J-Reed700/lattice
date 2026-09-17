import { describe, expect, it } from 'vitest';

import { applyDownloadSnapshot } from './useDownloads';

import type { DownloadStatus } from '../types/downloads';
import type { TauriEvents } from '../types/events';

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
