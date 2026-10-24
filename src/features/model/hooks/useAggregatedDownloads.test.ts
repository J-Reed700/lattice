import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { DownloadState, DownloadStatus } from '@/types/downloads';

import { useAggregatedDownloads } from './useAggregatedDownloads';

const file = (id: string, state: DownloadState, overrides: Partial<DownloadStatus> = {}): DownloadStatus => ({
  id, url: id, destination: `/models/${id}`, state, bytes_downloaded: 50, total_bytes: 100, bytes_per_second: 10,
  percentage: 50, eta_seconds: null, error_message: null, retry_count: 0, created_at: '2026-10-01T00:00:00Z',
  started_at: null, completed_at: null, model_name: 'Herbarium vision', model_id: 'herbarium', ...overrides,
});

const aggregate = (downloads: DownloadStatus[]) => renderHook(() => useAggregatedDownloads(downloads)).result.current;

describe('useAggregatedDownloads', () => {
  it('sums a multi-file model and keeps single files standalone', () => {
    const lone = file('lone', 'Downloading', { model_id: 'solo' });
    const plain = file('plain', 'Completed', { model_id: null });
    const { aggregated, standalone } = aggregate([
      file('a', 'Completed', { bytes_downloaded: 100, bytes_per_second: 0 }),
      file('b', 'Downloading', { bytes_downloaded: 20, total_bytes: 300, bytes_per_second: 30 }),
      lone,
      plain,
    ]);
    expect(standalone).toEqual([plain, lone]);
    expect(aggregated).toHaveLength(1);
    expect(aggregated[0]).toMatchObject({
      model_id: 'herbarium', model_name: 'Herbarium vision', total_bytes: 400, bytes_downloaded: 120, percentage: 30,
      average_speed: 15, state: 'Downloading', error_message: null,
    });
  });

  it.each<[DownloadState[], DownloadState]>([
    [['Failed', 'Downloading'], 'Downloading'],
    [['Paused', 'Failed'], 'Failed'],
    [['Pending', 'Paused'], 'Paused'],
    [['Completed', 'Pending'], 'Pending'],
    [['Completed', 'Cancelled'], 'Pending'],
    [['Completed', 'Completed'], 'Completed'],
  ])('reports %j as %s', (states, expected) => {
    const { aggregated } = aggregate(states.map((state, index) => file(`f${index}`, state)));
    expect(aggregated[0].state).toBe(expected);
  });

  it('carries the first failure message, or a generic one', () => {
    const named = aggregate([file('a', 'Failed', { error_message: 'checksum mismatch' }), file('b', 'Failed', { error_message: 'later' })]);
    expect(named.aggregated[0].error_message).toBe('checksum mismatch');
    const unnamed = aggregate([file('a', 'Failed'), file('b', 'Completed')]);
    expect(unnamed.aggregated[0].error_message).toBe('One or more files failed');
  });

  it('falls back to the model id for a name and to zero progress without sizes', () => {
    const { aggregated } = aggregate([
      file('a', 'Pending', { model_name: '', total_bytes: null, bytes_downloaded: 0 }),
      file('b', 'Pending', { total_bytes: null, bytes_downloaded: 0 }),
    ]);
    expect(aggregated[0]).toMatchObject({ model_name: 'herbarium', total_bytes: 0, percentage: 0 });
  });
});
